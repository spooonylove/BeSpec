mod fft_processor;
mod shared_state;
mod gui;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{bounded, Receiver};
use std::thread;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::fft_processor::{FFTProcessor, FFTConfig};
use shared_state::SharedState;
use crate::gui::SpectrumApp;

#[derive(Clone)]
struct AudioPacket {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    _timestamp: Instant,
}

impl AudioPacket {
    fn to_mono(&self) -> Vec<f32> {
        if self.channels == 1 {
            return self.samples.clone()
        } 

        // This is processing the time-based stream
        // stream is organized [L, R, L, R...]. This code breaks out the samples,
        // sums them, averages, and slams it right back togther.
        // Gee whiz, rust is effecient.
        self.samples
            .chunks(self.channels as usize)
            .map(|frame| frame.iter().sum::<f32>() / self.channels as f32)
            .collect()
    }
}

fn start_audio_capture(shutdown: Arc<AtomicBool>) -> Receiver<AudioPacket> {
    let (tx, rx) = bounded(10);

    thread::spawn(move || {
        println!("[Capture] Starting audio capture thread");

        let host = cpal::default_host();
        let device = match host.default_output_device() {
            Some(device) => device,
            None => {
                eprintln!("[Capture] ⚠ No output device found!");
                return;
            }
        };

        println!("[Capture] Using device: {}",
            device.name().unwrap_or_else(|_| "Unknown".to_string()));

        let config = match device.default_output_config() {
            Ok(config) => config,
            Err(e) => {
                eprintln!("[Capture] ⚠ Config error: {}", e);
                return;
            }
        };

        let sample_rate = config.sample_rate().0;
        let channels = config.channels();

        println!("[Capture] {} channels at {}Hz", channels, sample_rate);

        let err_fn = |err| eprintln!("[Capture] ⚠ Stream error: {}", err);

        let stream = match config.sample_format(){
            cpal::SampleFormat::F32 => {
                device.build_input_stream(
                    &config.into(),
                    move |data: &[f32], _| {
                        let packet = AudioPacket {
                            samples: data.to_vec(),
                            sample_rate,
                            channels,
                            _timestamp: Instant::now(),
                        };

                        if tx.try_send(packet).is_err(){
                            // FFT thread  can't keep up - drop packet
                        }
                    },
                    err_fn,
                    None,
               )
            }
            _ => {
                eprintln!("[Capture] Unsupported sample format");
                return;
            }
        };

        let stream = match stream {
            Ok(stream) => stream,
            Err(e) => {
                eprintln!("[Capture] Failed to build stream: {}", e);
                return;
            }
        };

        if let Err(e) = stream.play() {
            eprintln!("[Capture] Failed to play stream: {}", e);
            return;
        }

        println!("[Capture] ✓ Audio capture running... ");

        while !shutdown.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(100))
        }

        println!("[Capture] Shutting down...");

        drop(stream);
    });

    rx
}

/// Start FFT Processing thread
fn start_fft_processing(
    rx: Receiver<AudioPacket>,
    shared_state: Arc<Mutex<SharedState>>,
    shutdown: Arc<AtomicBool>
) {
    thread::spawn(move ||{
        println!("[FFT] Starting FFT processing thread...");

        //  Get initial config from shared state
        let config = {
            let state = shared_state.lock().unwrap();
            FFTConfig {
                fft_size: state.config.fft_size,
                sample_rate: 48000,
                num_bars: state.config.num_bars,
                sensitivity: state.config.sensitivity,
                attack_time_ms: state.config.attack_time_ms,
                release_time_ms: state.config.release_time_ms,
                peak_hold_time_ms: state.config.peak_hold_time_ms,
                peak_release_time_ms: state.config.peak_release_time_ms,
            }
            
        };
       
        let mut processor = FFTProcessor::new(config);
        let mut frame_count = 0u64;

        
        // === Performance Tracking ===
        let mut total_process_time = Duration::ZERO;
        let mut min_process_time = Duration::MAX;
        let mut max_process_time = Duration::ZERO;
        // =============================

        loop{
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(packet) => {
                    frame_count += 1;

                    // Convert to mono (FFT expects single channel
                    let mono = packet.to_mono();
                    
                    let process_start = Instant::now();
                    
                    // Process through FFT
                    let (bars, peaks) = processor.process(&mono);
                    let process_time = process_start.elapsed();

                    // Track min/max/total
                    total_process_time += process_time;
                    min_process_time = min_process_time.min(process_time);
                    max_process_time = max_process_time.max(process_time);

                   // Update shared state
                   if let Ok(mut state) = shared_state.lock() {
                        // Update  visualization  data
                        state.visualization.bars = bars;
                        state.visualization.peaks = peaks;
                        state.visualization.timestamp = Instant::now();

                        // Update performance stats
                        state.performance.frame_count = frame_count;
                        state.performance.fft_ave_time = total_process_time / frame_count as u32;
                        state.performance.fft_min_time = min_process_time;
                        state.performance.fft_max_time = max_process_time;

                        // Check if config changed (requires FFT rebuild)
                        let new_config = FFTConfig {
                            fft_size: state.config.fft_size,
                            sample_rate: 48000,
                            num_bars: state.config.num_bars,
                            sensitivity: state.config.sensitivity,
                            attack_time_ms: state.config.attack_time_ms,
                            release_time_ms: state.config.release_time_ms,
                            peak_hold_time_ms: state.config.peak_hold_time_ms,
                            peak_release_time_ms: state.config.peak_release_time_ms,
                        };

                        // Update the processor config (handles resize internally)
                        processor.update_config(new_config);
                            
                   }
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    eprintln!("[FFT] Capture disconnected!");
                    break;
                }
            }
        }

        println!("[FFT] Shutdown (processed {} frames)", frame_count);
        if frame_count > 0 {
            let avg_time = total_process_time / frame_count as u32;
            println!("[FFT] === Final Performance Stats ===");
            println!("[FFT]    Total frames:   {}", frame_count);
            println!("[FFT]    Avg time:       {:?}", avg_time);
            println!("[FFT]    Min time:       {:?}", min_process_time);
            println!("[FFT]    Max time:       {:?}", max_process_time);
            println!("[FFT]    FPS Potential:  {:.1}", 1000.0 / avg_time.as_micros() as f64 * 1000.0 );

            // Calculate what % of frame budge we're using
            let target_frame_time = Duration::from_millis(16);  // 60 FPS = 16.67ms
            let usage_pct = (avg_time.as_micros() as f64 / target_frame_time.as_micros() as f64) * 100.0;
            println!("[FFT]     CPU usage:     {:.1}% of 60fps budget", usage_pct);
        } 
        
    });
}

fn main (){
    println!("=== BeAnal - Rust Audio Spectrum Analyzer ===\n");

    // create shared state
    let shared_state = Arc::new(Mutex::new(SharedState::new()));

    // Get initial window settinghsd from default config
    let (initial_decorations, initial_on_top) = {
        let state = shared_state.lock().unwrap();
        (state.config.window_decorations, state.config.always_on_top)
    };

    // Shutdown signal for audio threads
    let shutdown = Arc::new(AtomicBool::new(false));

    // Start audio capture thread
    let audio_rx = start_audio_capture(shutdown.clone());

    // Start FFT processing thread
    start_fft_processing(audio_rx, shared_state.clone(), shutdown.clone());

    println!("[Main] Starting GUI...\n");

    // Create a mutable viewport_builder
    let mut viewport_builder = egui::ViewportBuilder::default()
        .with_inner_size([800.0, 450.0])
        .with_title("BeAnal - Audio Spectrum Analyzer")
        .with_resizable(true)
        .with_transparent(true)
        .with_decorations(initial_decorations);

    // Conditionally apply 'always on top' setting
    if initial_on_top {
        viewport_builder = viewport_builder.with_always_on_top();
    }

    // Configure and launch GUI
    let options = eframe::NativeOptions{
        viewport: viewport_builder,
        ..Default::default()
    };

    // Run the app  (this blocks until window closes)
    let _result = eframe::run_native(
        "BeAnal",
        options, 
        Box::new(|_cc| Ok(Box::new(SpectrumApp::new(shared_state.clone())))),
    );

    // Signal shutdown to audio threads
    println!("\n[Main] Shutting down audio threads...");
    shutdown.store(true, Ordering::Relaxed);

    // Give threadss time to clean up
    thread::sleep(Duration::from_millis(500));


    println!("[Main] ✓ Shutdown complete");

}

