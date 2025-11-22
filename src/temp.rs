// In src/main.rs

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
                        let config = {
                            let state = shared_state.lock().unwrap();
                            FFTConfig {
                                fft_size: new_fft_config.get_fft_size(),
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

                        //Update FFT config (returns true if rebuild needed)
                        let needs_rebuild = fft_config.update_sample_rate(packet.sample_rate);

                        if needs_rebuild {
                            // Rebuild FFT processor with new FFT size
                            let info = fft_config.info();
                            println!(
                                "[FFT] ⚙️  Rebuilding FFT: {} Hz, FFT size: {}, latency: {:.2}ms",
                                info.sample_rate, info.fft_size, info.latency_ms
                            );

                            let new_config = {
                                let state = shared_state.lock().unwrap();
                                FFTConfig {
                                    fft_size: info.fft_size, 
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
                        // We need to compare with what the processor is currently using
                        // Since we can't access processor.config directly, we track the 
                        // current FFT siize from our fft_config manager
                        let current_fft_size = fft_config.get_fft_size();

                        // 1. Check for changes that require a rebuild
                        let needs_update = state.config.fft_size != current_fft_size ||
                                            state.config.num_bars != state.visualization.bars.len();

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
                            println!("[FFT] Config change requires rebuild (FFT size or bar count");
                            Some(FFTConfig {
                                fft_size: state.config.fft_size,
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

                            let current =  processor.get_config();

                            if config_differs(current) {
                                // Log specific changes for debugging
                                if state.config.use_peak_aggregation != current.use_peak_aggregation {
                                    println!(
                                        "[FFT] Aggregation mode changed: {} -> {}",
                                        if current.use_peak_aggregation { "Peak" } else { "Average" },
                                        if state.config.use_peak_aggregation { "Peak" } else { "Average" }
                                    );
                                }

                                Some(FFTConfig {
                                    fft_size: current_fft_size,
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
                    }; // <--- Added closing brace and semicolon for 'pending_config_update'

                    // Apply config update if needed
                    if let Some(new_config) = pending_config_update {
                        processor.update_config(new_config);
                    }

                } // <--- Added closing brace for 'Ok(packet)' match arm

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