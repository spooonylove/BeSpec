mod audio_capture;
mod audio_device;
mod fft_config;
mod fft_processor;
mod gui;
mod shared_state;

use std::thread;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::bounded;

use crate::fft_processor::{FFTProcessor, FFTConfig};
use shared_state::SharedState;
use crate::gui::SpectrumApp;
use crate::audio_capture::{AudioCaptureManager, AudioPacket};
use crate::fft_config::{FFTConfigManager, FIXED_FFT_SIZE};

// ========================================================================
// AUDIO CAPTURE THREAD
// ========================================================================
//    Uses AudioCaptureManager for defvice enumeration and auto-detection

fn start_audio_capture(shutdown: Arc<AtomicBool>) -> crossbeam_channel::Receiver<AudioPacket> {
    let (tx, rx) = bounded(10);

    thread::spawn(move || {
        println!("[Capture] Starting audio capture thread");

        // Create Capture Manager (uses default device)
        let mut capture = match AudioCaptureManager::new() {
            Ok(mgr) => mgr,
            Err(e) => {
                eprintln!("[Capture] ❌ Failed to create audio capture manager: {}", e);
                return;
            }
        };

        // Start capturing
        if let Err(e) = capture.start_capture() {
            eprintln!("[Capture] ❌ Failed to start capture: {}", e);
            return;
        }

        println!("[Capture] ✓ Audio capture thread started");

        let audio_rx = capture.receiver();

        // Keep receiving audio packets and forward them
        while !shutdown.load(Ordering::Relaxed) {
            match audio_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(packet) => {
                    // Forward to FFT thread
                    if tx.try_send(packet).is_err() {
                        // FFT thread can't keep up, drop packet
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    eprint!("[Capture] Audio stream disconnected!");
                    break;
                }
            }
        
        }

        println!("[Capture] Shutting down...");
        capture.stop_capture();
    });

    rx
}


// ========================================================================
// FFT PROCESSING THREAD
// ========================================================================
fn start_fft_processing(
    rx: crossbeam_channel::Receiver<AudioPacket>,
    shared_state: Arc<Mutex<SharedState>>,
    shutdown: Arc<AtomicBool>
) {
    thread::spawn(move || {
        println!("[FFT] Starting FFT processing thread...");

              
        let mut processor: Option<FFTProcessor> = None;
        let mut fft_config: Option<FFTConfigManager> = None;
        let mut frame_count= 0u64;
        
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

                    // ====== Initialization: First packet tells us the sample rate
                    if processor.is_none() || fft_config.is_none() {
                        println!(
                            "[FFT] 🎵 First audio packet received at {} Hz",
                            packet.sample_rate
                        );
                    
                        // Initialize FFT config with ACTUAL device sample rates!
                        let new_fft_config = FFTConfigManager::new(packet.sample_rate);

                        // Get initial settings from shared state
                        let config: FFTConfig = {
                            let state = shared_state.lock().unwrap();
                            FFTConfig {
                                fft_size: FIXED_FFT_SIZE,
                                sample_rate: packet.sample_rate,
                                num_bars: state.config.num_bars,
                                sensitivity: state.config.sensitivity,
                                attack_time_ms: state.config.attack_time_ms,
                                release_time_ms: state.config.release_time_ms,
                                peak_hold_time_ms: state.config.peak_hold_time_ms,
                                peak_release_time_ms: state.config.peak_release_time_ms,
                                use_peak_aggregation: state.config.use_peak_aggregation,
                            }
                        };

                        let new_processor = FFTProcessor::new(config);
                        
                        let info = new_fft_config.info();
                        println!(
                            "[FFT] ✓ Initialized: {} Hz, FFT size: {}, latency: {:.2}ms, mode: {}",
                                info.sample_rate, info.fft_size, info.latency_ms,
                                if new_processor.get_config().use_peak_aggregation { "Peak" } else { "Average" }
                        );
                    
                        processor = Some(new_processor);
                        fft_config = Some(new_fft_config);
                    }
                    
                    // At this point, both FFT configuration and the FFT Processor
                    // should be initialized
                    let processor = match processor.as_mut(){
                        Some(p) => p,
                        None => continue, //Shouldn't happen, but be safe
                    };

                    let fft_config  = match fft_config.as_mut(){
                        Some(c) => c,
                        None => continue, //Shouldn't happen, but be safe
                    };

                    // ==== CRITICAL: Handle sample rate changes =====
                    // If device sample rate changed, update FFT config
                    if packet.sample_rate != fft_config.get_sample_rate() {
                        println!(
                            "[FFT] 🔄 Sample rate changed: {} Hz → {} Hz",
                            fft_config.get_sample_rate(),
                            packet.sample_rate
                        );

                        //Update FFT config 
                        fft_config.update_sample_rate(packet.sample_rate);

                        
                        // Rebuild FFT processor with new FFT size
                        let info = fft_config.info();
                        println!(
                            "[FFT] ⚙️  Rebuilding FFT: {} Hz, latency: {:.2}ms",
                            info.sample_rate, info.latency_ms
                        );

                        let new_config = {
                            let state = shared_state.lock().unwrap();
                            FFTConfig {
                                fft_size: FIXED_FFT_SIZE, 
                                sample_rate: info.sample_rate,
                                num_bars: state.config.num_bars,
                                sensitivity: state.config.sensitivity,
                                attack_time_ms: state.config.attack_time_ms,
                                release_time_ms: state.config.release_time_ms,
                                peak_hold_time_ms: state.config.peak_hold_time_ms,
                                peak_release_time_ms: state.config.peak_release_time_ms,
                                use_peak_aggregation: state.config.use_peak_aggregation,
                            }
                        };

                        *processor = FFTProcessor::new(new_config);
                         
                    }

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
                   let pending_config_update = {
                        let mut state = shared_state.lock().unwrap();
                        // Update  visualization  data
                        state.visualization.bars = bars;
                        state.visualization.peaks = peaks;
                        state.visualization.timestamp = Instant::now();

                        // Update performance stats
                        state.performance.frame_count = frame_count;
                        state.performance.fft_ave_time = total_process_time / frame_count as u32;
                        state.performance.fft_min_time = min_process_time;
                        state.performance.fft_max_time = max_process_time;
                        state.performance.fft_info = fft_config.info();

                        

                        // Check if any config parameters changed
                        // 1. Check for changes that require a rebuild
                        let needs_update = state.config.num_bars != state.visualization.bars.len();

                        let config_differs = |current: &FFTConfig| -> bool {
                            state.config.sensitivity != current.sensitivity ||
                            state.config.attack_time_ms != current.attack_time_ms ||
                            state.config.release_time_ms != current.release_time_ms ||
                            state.config.peak_hold_time_ms != current.peak_hold_time_ms ||
                            state.config.peak_release_time_ms != current.peak_release_time_ms ||
                            state.config.use_peak_aggregation != current.use_peak_aggregation
                        };
                                              
                        
                        if needs_update {
                            //Major change - needs FFT rebuild
                            println!(
                                "[FFT] Config change requires rebuild (bar count: {} → {})",
                                state.visualization.bars.len(),
                                state.config.num_bars
                            );
                         
                            Some(FFTConfig {
                                fft_size: FIXED_FFT_SIZE,
                                sample_rate: fft_config.get_sample_rate(),
                                num_bars: state.config.num_bars,
                                sensitivity: state.config.sensitivity,
                                attack_time_ms: state.config.attack_time_ms,
                                release_time_ms: state.config.release_time_ms,
                                peak_hold_time_ms: state.config.peak_hold_time_ms,
                                peak_release_time_ms: state.config.peak_release_time_ms,
                                use_peak_aggregation: state.config.use_peak_aggregation,
                            })
                        } else {
                            // Check for minor config changes that don't require a rebuild

                            let current = processor.get_config();

                            if config_differs(&current) {
                                // Log specific changes for debugging
                                if state.config.use_peak_aggregation != current.use_peak_aggregation {
                                    println!{
                                        "[FFT] Aggregation mode changed: {} → {}",
                                        if current.use_peak_aggregation { "Peak" } else { "Average" },
                                        if state.config.use_peak_aggregation { "Peak" } else { "Average" }
                                    };
                                }
                            

                                Some(FFTConfig {
                                    fft_size: FIXED_FFT_SIZE,
                                    sample_rate: fft_config.get_sample_rate(),
                                    num_bars: state.config.num_bars,
                                    sensitivity: state.config.sensitivity,
                                    attack_time_ms: state.config.attack_time_ms,
                                    release_time_ms: state.config.release_time_ms,
                                    peak_hold_time_ms: state.config.peak_hold_time_ms,
                                    peak_release_time_ms: state.config.peak_release_time_ms,
                                    use_peak_aggregation: state.config.use_peak_aggregation,
                                })
                            } else {
                                None
                            }
                        }
                    };
                    // Apply confiig update if needed
                    if let Some(new_config) = pending_config_update {
                        if new_config.num_bars != processor.get_config().num_bars {
                            println!("[FFT]♻️ Recreating processor for new bar count: {}", new_config.num_bars);
                            *processor = FFTProcessor::new(new_config);
                        } else {
                            println!("[FFT]🔧 Updating processor config");
                            processor.update_config(new_config);
                        }
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
            let usage_pct = 
                (avg_time.as_micros() as f64 / target_frame_time.as_micros() as f64) * 100.0;
            println!("[FFT]     CPU usage:     {:.1}% of 60fps budget", usage_pct);
        } 
        
    });
}

fn main (){
    println!("=== BeAnal - Rust Audio Spectrum Analyzer ===\n");
    println!("    FFT Size: {} (fixed)\n", FIXED_FFT_SIZE);

    // create shared state
    let shared_state = Arc::new(Mutex::new(SharedState::new()));

    // Get initial window settinghs from default config
    let (initial_decorations, initial_on_top, initial_size, initial_pos) = {
        let state = shared_state.lock().unwrap();
        (
            state.config.window_decorations, 
            state.config.always_on_top,
            state.config.window_size,
            state.config.window_position
        )
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
        .with_inner_size(initial_size)
        .with_title("BeAnal - Audio Spectrum Analyzer")
        .with_resizable(true)
        .with_transparent(true)
        .with_decorations(initial_decorations);

    // Apply position if saved
    if let Some(pos) = initial_pos {
        viewport_builder = viewport_builder.with_position([pos[0], pos[1]]);
    }

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

    // The window has closed. Now we force a save to sensure settings persist
    println!("[Main] Saving configuration...");
    if let Ok(state) = shared_state.lock() {
        state.config.save();
    }

    // Signal shutdown to audio threads
    println!("\n[Main] Shutting down audio threads...");
    shutdown.store(true, Ordering::Relaxed);

    // Give threadss time to clean up
    thread::sleep(Duration::from_millis(500));


    println!("[Main] ✓ Shutdown complete");

}

