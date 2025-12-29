#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console on Windows in release

mod audio_capture;
mod audio_device;
mod fft_config;
mod fft_processor;
mod gui;
mod shared_state;
mod media;

use core::panic;
use std::thread;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::fs;

use time::macros::format_description;
use tracing_subscriber::fmt::time::OffsetTime;
use tracing_subscriber::EnvFilter;

use crossbeam_channel::bounded;
use directories::ProjectDirs;

use crate::audio_device::AudioDeviceEnumerator;
use crate::fft_processor::{FFTProcessor, FFTConfig};
use crate::shared_state::{SILENCE_DB, VisualMode};
use shared_state::SharedState;
use crate::gui::SpectrumApp;
use crate::audio_capture::{AudioCaptureManager, AudioPacket};
use crate::fft_config::{FFTConfigManager, FIXED_FFT_SIZE};
use crate::media::{PlatformMedia, MediaMonitor};

// ========================================================================
// AUDIO CAPTURE THREAD
// ========================================================================
//    Uses AudioCaptureManager for defvice enumeration and auto-detection

fn start_audio_capture(
    shutdown: Arc<AtomicBool>,
    shared_state: Arc<Mutex<SharedState>>
) -> crossbeam_channel::Receiver<AudioPacket> {
    
    let (tx, rx) = bounded(10);

    thread::spawn(move || {
        tracing::info!("[Capture] Starting audio capture thread");

        // 1. Initial Device List Population
        tracing::info!("[Capture] 🔍 Initializing audio device list...");
        if let Ok(devices) = AudioCaptureManager::list_devices() {
            let mut state = shared_state.lock().unwrap();
            state.audio_devices = devices.iter().map(|d| d.name.clone()).collect();

            tracing::info!("[Capture] ✓ Found {} audio devices", state.audio_devices.len());
            for (i, name) in state.audio_devices.iter().enumerate() {
                tracing::info!("[Capture]    {}: {}", i, name);
            }
        } else {
            tracing::error!("[Capture] ❌ Failed to enumerate initial audio devices");
        }

        // 2. Initial Device Selection
        let initial_device = {
            shared_state.lock().unwrap().config.selected_device.clone()
        };
        tracing::info!("[Capture] Target device: {}", initial_device);

        // 3. Create Audio Capture Manager
        let mut capture = if initial_device == "Default" {
            AudioCaptureManager::new().unwrap_or_else(|e|{
                tracing::error!("[Capture] ❌ Critical: Failed to create default audio device: {}", e);
                panic!("Audio init failed");
            })
        } else {
            AudioCaptureManager::with_device_id(&initial_device).unwrap_or_else(|_|{
                tracing::info!("[Capture] ⚠️ Saved device not found, falling back to System Default ");
                AudioCaptureManager::new().expect("Failed to init default device")
            })
        };

        // Start capturing
        if let Err(e) = capture.start_capture() {
            tracing::error!("[Capture] ❌ Failed to start capture: {}", e);
            return;
        }
        tracing::info!("[Capture] ✓ Audio capture thread started");

        // Keep receiving audio packets and forward them
        while !shutdown.load(Ordering::Relaxed) {

            // === CHECK FLAGS ===
            // Verify flags everty cycle (~100ms timeout below)
            let (needs_refresh, new_device_req) = {
                if let Ok(mut state) = shared_state.try_lock() {
                    let refresh = state.refresh_devices_requested;
                    let change = if state.device_changed {
                        Some(state.config.selected_device.clone())
                    } else {
                        None
                    };

                    // Reset flags
                    if refresh { state.refresh_devices_requested = false; }
                    if change.is_some() { state.device_changed = false;}
                    (refresh, change)
                } else {
                    (false, None)
                }
            };

            // === ACTION: REFRESH === 
            if needs_refresh {
                tracing::info!("[Capture] 🔄 Manual refresh requested. Scanning hardware...");
                let start = Instant::now();

                if let Ok(devices) = AudioCaptureManager::list_devices() {
                    if let Ok(mut state) = shared_state.lock() {
                        state.audio_devices = devices.iter().map(|d| d.name.clone()).collect();
                        tracing::info!("[Capture] ✓ Scan complete in {:.2}ms, Found {} audio devices",
                            start.elapsed().as_secs_f32() * 1000.0,
                            state.audio_devices.len()
                        );
                    }
                } else {
                    tracing::error!("[Capture] ⚠️ Device scan failed");
                }
            }
            
            // === ACTION: DEVICE CHANGE ===
            if let Some(new_name) = new_device_req {
                tracing::info!("[Capture] 🔄 Audio device change requested: {}", new_name);
                
                let result = if new_name == "Default" {
                    if let Ok((_, info)) = AudioDeviceEnumerator::get_default_device() {
                        tracing::info!("[Capture] Resolving 'Default' -> '{}'", info.id);
                        capture.switch_device(&info.id)
                    } else {
                        Err(crate::audio_device::AudioDeviceError::DeviceNotFound("Default".into()))
                    }
                } else {
                    capture.switch_device(&new_name)
                };

                match result {
                    Ok(_) => tracing::info!("[Capture] ✓ Switched to new device: {}", new_name),
                    Err(e) => tracing::error!("[Capture] ❌ Failed to switch device: {}", e),
                }
            }
            
            // === PROCESS AUDIO ===
            match capture.receiver().recv_timeout(Duration::from_millis(100)) {
                Ok(packet) => {
                    // Forward to FFT thread
                    let _ = tx.try_send(packet);
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    eprint!("[Capture] ⚠️ Stream disconnected unexpectedly");
                    break;
                },
            }
        }

        tracing::info!("[Capture] Shutting down...");
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
        tracing::info!("[FFT] Starting FFT processing thread...");

              
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
                        tracing::info!(
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
                        tracing::info!(
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
                        tracing::info!(
                            "[FFT] 🔄 Sample rate changed: {} Hz → {} Hz",
                            fft_config.get_sample_rate(),
                            packet.sample_rate
                        );

                        //Update FFT config 
                        fft_config.update_sample_rate(packet.sample_rate);

                        
                        // Rebuild FFT processor with new FFT size
                        let info = fft_config.info();
                        tracing::info!(
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

                    // Convert to mono (FFT expects single channel)
                    let mono = packet.to_mono();
                    
                    let mode  = { shared_state.lock().unwrap().config.visual_mode };

                    match mode {
                        VisualMode::Oscilloscope => {
                            // === SCOPE MODE: BYPASS FFT ===
                            // Just normalize/copy raw samples directly to visualization
                            // We might want to decimate or window here if the packet is huge.
                            let mut state = shared_state.lock().unwrap();
                            state.visualization.waveform = mono;
                            state.visualization.bars.fill(SILENCE_DB);
                        }
                        _ => {
                            // A. Start the timer!
                            let process_start = Instant::now();

                            // B. Heavy Math (FFT)
                            let (bars, peaks) = processor.process(&mono);

                            // C. Stop Timer
                            let process_time = process_start.elapsed();

                            // D. Track Performance Stats
                            total_process_time += process_time;
                            min_process_time = min_process_time.min(process_time);
                            max_process_time = max_process_time.max(process_time);

                            // E. Update shared state
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
                                    tracing::debug!(
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
                                            tracing::info!{
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
                                    tracing::debug!("[FFT]♻️ Recreating processor for new bar count: {}", new_config.num_bars);
                                    *processor = FFTProcessor::new(new_config);
                                } else {
                                    tracing::debug!("[FFT]🔧 Updating processor config");
                                    processor.update_config(new_config);
                                }
                            }
                        }
                    }
                }
                
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // if we haven't received audio for 100ms, the stream is likely 
                    // stopped, switching, or silent. Reset bars to silence
                    if let Ok(mut state) = shared_state.lock() {
                        // Optimization: check frist bar to see if we are already silent
                        let current_silence = shared_state::SILENCE_DB;
                        let needs_clear = state.visualization.bars.first()
                            .map_or(true, |&v| v > current_silence);
                        if needs_clear {
                            // fill with silence
                            state.visualization.bars.fill(current_silence);
                            state.visualization.peaks.fill(current_silence);
                            state.visualization.timestamp = Instant::now();
                        }
                    }
                    continue;
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    tracing::error!("[FFT] Capture disconnected!");
                    break;
                }
            }
        }

        tracing::info!("[FFT] Shutdown (processed {} frames)", frame_count);
        if frame_count > 0 {
            let avg_time = total_process_time / frame_count as u32;
            tracing::info!("[FFT] === Final Performance Stats ===");
            tracing::info!("[FFT]    Total frames:   {}", frame_count);
            tracing::info!("[FFT]    Avg time:       {:?}", avg_time);
            tracing::info!("[FFT]    Min time:       {:?}", min_process_time);
            tracing::info!("[FFT]    Max time:       {:?}", max_process_time);
            tracing::info!("[FFT]    FPS Potential:  {:.1}", 1000.0 / avg_time.as_micros() as f64 * 1000.0 );

            // Calculate what % of frame budge we're using
            let target_frame_time = Duration::from_millis(16);  // 60 FPS = 16.67ms
            let usage_pct = 
                (avg_time.as_micros() as f64 / target_frame_time.as_micros() as f64) * 100.0;
            tracing::info!("[FFT]     CPU usage:     {:.1}% of 60fps budget", usage_pct);
        } 
        
    });
}

// ========================================================================
// Load Icon to Memory
// ========================================================================
fn load_icon() -> Option<Arc<egui::IconData>> {
    // 1. Embed the bytes. Path is relative to this file (src/main.rs)
    //    So we go up one level (..) to root, then into assets/
    let icon_bytes = include_bytes!("../assets/icon.png");

    // 2. Decode the bytes into an image
    //    We use the `image` crate to handle PNG decoding
    match image::load_from_memory(icon_bytes) {
        Ok(image) => {
            let image = image.into_rgba8();
            let (width, height) = image.dimensions();
            let rgba = image.into_raw();

            // 3. Return the data in egui's expected format
            Some(Arc::new(egui::IconData {
                rgba,
                width,
                height,
            }))
        }
        Err(e) => {
            tracing::error!("[Main] ❌ Failed to load icon : {}", e);
            None
        }
    }
}

fn main (){
    
    // =====================================================================
    // 1. Setup cross-platforing logging
    // =====================================================================

    // Determine the correct data directory for the current OS
    // Windows: %APPDATA%\BeAnal
    // Linux: ~/.local/share/beanal
    // macOs: ~/Library/Application Support/BeAnal
    let log_dir = if let Some(proj_dirs) = ProjectDirs::from("", "", "BeAnal") {
        proj_dirs.data_dir().join("logs")
    } else {
        // Fallback to local directory wif we can't find the home folder
        std::path::PathBuf::from("logs")
    };

    // Ensure the directoy exists (otherwise logging will fail)
    if let Err(e) = fs::create_dir_all(&log_dir) {
        tracing::error!("[Main] ❌ Failed to create log directory {:?}: {}", log_dir, e);
    }

    // Set up the file appender
    let file_appender = tracing_appender::rolling::daily(&log_dir, "beanal.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Get local offset. Fall back to UTC if it fails
    let offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);

    // Define the time format for logs (human readable, folks!)
    let timer = OffsetTime::new(
        offset,
        format_description!("[hour]:[minute]:[second]"),
    );

    // Set up Logic Level Filter
    // This scheks the "RUST_LOG" environment variable for configuration
    // If not set, it defaults to "info" level logging
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));


    // Initialize tracing
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_timer(timer)
        .with_env_filter(env_filter) // No ANSI codes in log files
        .init();

    // Log startup info
    tracing::info!("=== BeAnal Startup ===");
    tracing::info!("Platform: {}", std::env::consts::OS);
    tracing::info!("Log Directory: {:?}", log_dir);
    
    // ========================================================================
    // 2. INITIALIZE APP STATE
    // ========================================================================

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
    let audio_rx = start_audio_capture(shutdown.clone(), shared_state.clone());

    // Start FFT processing thread
    start_fft_processing(audio_rx, shared_state.clone(), shutdown.clone());

    // Start Media Monitoring thread
    tracing::info!("[Main] Starting Media Monitor...");
    let media_manager = Arc::new(PlatformMedia::new());
    let (media_tx, media_rx) = bounded(10);
    media_manager.start(media_tx);


    tracing::info!("[Main] Starting GUI...\n");

    // Load the icon
    let app_icon = load_icon();

    // Create a mutable viewport_builder
    let mut viewport_builder = egui::ViewportBuilder::default()
        .with_inner_size(initial_size)
        .with_title("BeAnal - Audio Spectrum Analyzer")
        .with_resizable(true)
        .with_transparent(true)
        .with_decorations(initial_decorations);

    // Apply icon if loaded
    if let Some(icon) = app_icon {
        viewport_builder = viewport_builder.with_icon(icon);
    }

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
        Box::new(|_cc| Ok(Box::new(SpectrumApp::new(
            shared_state.clone(),
            media_rx,
            media_manager.clone()
        )))),
    );

    // The window has closed. Now we force a save to sensure settings persist
    tracing::info!("[Main] Saving configuration...");
    if let Ok(state) = shared_state.lock() {
        state.config.save();
    }

    // Signal shutdown to audio threads
    tracing::info!("[Main] Shutting down audio threads...");
    shutdown.store(true, Ordering::Relaxed);

    // Give threadss time to clean up
    thread::sleep(Duration::from_millis(500));


    tracing::info!("[Main] ✓ Shutdown complete");

}

