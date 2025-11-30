use realfft::{RealFftPlanner, RealToComplex, num_traits::Float};
use std::sync::Arc;
use crate::shared_state::SILENCE_DB;


// configure for FFT processing and visualization
#[derive(Clone)]
pub struct FFTConfig{
    pub fft_size: usize,
    pub sample_rate: u32,
    pub num_bars: usize,
    pub sensitivity: f32,               // User-configurable gain
    pub attack_time_ms: f32,            // Bar rise speed
    pub release_time_ms: f32,           // bar fall speed
    pub peak_hold_time_ms: f32,         // duration of peak hold
    pub peak_release_time_ms: f32,      // peak fall speed
    pub use_peak_aggregation: bool,     // bar aggregation peak vs average
}

impl Default for FFTConfig {
    fn default() -> Self{
        Self {
            fft_size: 1024,
            sample_rate: 48000,
            num_bars: 64,
            sensitivity: 1.0,
            attack_time_ms: 200.0,
            release_time_ms: 200.0,
            peak_hold_time_ms: 1500.0,
            peak_release_time_ms: 1500.0,
            use_peak_aggregation: true,
        }
     }
}

/// Maps visual bars to FFT bin ranges (start_bin, end_bin)
type BarToBinMap = Vec<(usize, usize)>;

/// Main FFT processor - handles windowing, FFT, and bar mapping
pub struct FFTProcessor{
    config: FFTConfig,

    // FFT State (reusable, no per-frame allocation)
    fft: Arc<dyn RealToComplex<f32>>,
    input_buffer: Vec<f32>,     // Windowed inptut samples
    output_buffer: Vec<f32>,    // FFT magnitude output
    scratch_buffer: Vec<num_complex::Complex<f32>>,   // Scratch space for FFT

    // Hann Window (precomputed, never changes)
    hann_window: Vec<f32>,

    // Bar mapping (linear + log hybrid)
    bar_to_bin_map: BarToBinMap,

    // Smoothing state (persists between frames)
    last_bar_heights: Vec<f32>,
    peak_levels: Vec<f32>,
    peak_hold_timers: Vec<f32>, // Time remaining for peak hold (ms)

    // Frame Timing for smooth interpoloations
    last_frame_time: std::time::Instant,
}

impl FFTProcessor {
    /// Create a new FFT processor with a given configuration
    pub fn new(config: FFTConfig) -> Self {

        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(config.fft_size);
        
        // Allocate all buffers upfront (no runtime allocations)
        let input_buffer = vec![0.0; config.fft_size];
        let output_buffer = vec![0.0; config.fft_size / 2 + 1];
        let scratch_buffer = fft.make_scratch_vec();

        // Precompute Hann Window
        let hann_window = Self::compute_hann_window(config.fft_size);

        // Initialize bar mapping
        let bar_to_bin_map = Self::compute_bar_mapping(&config);

        // Initialize smoothing state
        let last_bar_heights = vec![SILENCE_DB; config.num_bars];
        let peak_levels = vec![SILENCE_DB; config.num_bars];
        let peak_hold_timers = vec![SILENCE_DB; config.num_bars];

        Self {
            config,
            fft,
            input_buffer,
            output_buffer,
            scratch_buffer,
            hann_window,
            bar_to_bin_map,
            last_bar_heights,
            peak_levels,
            peak_hold_timers,
            last_frame_time: std::time::Instant::now(),
        }
    }

    /// Process audio samples and return bar heights
    /// 
    /// Returns: (bar_heights, peak_heights)
    pub fn process(&mut self, samples: &[f32]) -> (Vec<f32>, Vec<f32>) {
        // Calculate delta time for smoothing
        let now = std::time::Instant::now();
        let delta_ms = now.duration_since(self.last_frame_time).as_secs_f32() * 1000.0;
        self.last_frame_time = now;

        // step 1: Copy samples to input buffer and apply windowing
        self.apply_window(samples);
        
        // step 2: Perform FFT
        self.compute_fft();

        // Step 3: Convert to magnitudes (dB scale)
        let magnitudes = self.compute_magnitudes();

        // Step 4:
        let raw_bars = self.group_bins(&magnitudes);

        // Step 5: Apply smoothing (attack/release)
        let smoothed_bars = self.apply_smoothing(&raw_bars, delta_ms);

        // step 6: Update peals
        let peaks = self.update_peaks(&smoothed_bars, delta_ms);

        (smoothed_bars, peaks)
    }

    #[allow(dead_code)]
    /// Update configuration (e.g., user changed the number of bars)
    pub fn update_config(&mut self, config: FFTConfig) {

        // check if we need to re-calculate the map
        // we do this if the bar count changes or the knee frequency changes        
        let need_remap = config.num_bars != self.config.num_bars;

        if need_remap {
            // Resize arrays if the bar count changed
            if config.num_bars != self.config.num_bars {
                self.last_bar_heights.resize(config.num_bars, SILENCE_DB);
                self.peak_levels.resize(config.num_bars, SILENCE_DB);
                self.peak_hold_timers.resize(config.num_bars, 0.0);
            }

            // Recomput the mapping
            self.bar_to_bin_map = Self::compute_bar_mapping(&config);
        }

        self.config = config;
    }

    // ============ Private Implementation ============

    // Precompute Hann Window Function
    fn compute_hann_window(size: usize) -> Vec<f32> {
        (0..size)
            .map(|i| {
                let angle = 2.0 * std::f32::consts::PI * i as f32 / (size - 1) as f32;
                0.5 * (1.0 - angle.cos())
            })
            .collect()
    }

    // Apply Hann Window to input samples
    fn apply_window(&mut self, samples: &[f32]) {
        let len = samples.len().min(self.config.fft_size);

        // copy and window
        for i in 0..len {
            self.input_buffer[i] = samples[i] * self.hann_window[i];
        }

        // zero-pad if needed
        for i in len..self.config.fft_size {
            self.input_buffer[i] = 0.0;
        }

    }

    /// Compute FFT (modifies output_buffer in place)
    fn compute_fft(&mut self) {
        // realfft requires complex output, but we only need magnitudes
        let mut spectrum = self.fft.make_output_vec();

        self.fft 
            .process_with_scratch(&mut self.input_buffer, &mut spectrum, &mut self.scratch_buffer)
            .expect("FFT processing failed");
        
        // Store magnitudes in output_buffer
        for (i, complex) in spectrum.iter().enumerate() {
            self.output_buffer[i] = complex.norm();
        }
    }

    /// Convert FFT output to dB magniotudes with sensitivity
    fn compute_magnitudes(&self) -> Vec<f32> {
        // Magnitude computation is a key point of visualization, and we have to 
        // make a selection here on how to scale the raw FFT output.  Technically
        // more accurate, we should normalize by fft_size/2 to get true amplitudes.
        // However, for visualization purposes, we currently are correcting more
        // aggressively to get a better dynamic range, and a more responsive, 
        // visually appealing result.

        const HANN_WINDOW_CORRECTION: f32 = 2.0; // Compensate for Hann window energy loss

        self.output_buffer
            .iter()
            .map(|&mag| {
                
                //1. Normalize the FFT output first
                let corrected_mag = mag * HANN_WINDOW_CORRECTION;

                //2. Apply sensitivity
                
                // Apply sensitivity (user gain)
                let  adjusted = corrected_mag * self. config.sensitivity;

                //3. Convert to dB scale
                // add small epsilon to avoid log(0)
                20.0 * (adjusted + 1e-10).log10()
            })
            .collect()
    }

    /// Perform a linear-log hybrid mapping of the FFT data to visualization bars
    fn compute_bar_mapping(config: &FFTConfig) -> BarToBinMap {
        let mut map = Vec::with_capacity(config.num_bars);

        let max_fft_bin = config.fft_size / 2;
        let frequency_resolution = config.sample_rate as f64 / config.fft_size as f64;
        let nyquist_freq = config.sample_rate as f64 / 2.0;

        // === ADPATIIVE PARAMETERS  ===
        // Instead of fixed proportions, adapt based on available data

        // Calculate how many bins we have for low frequencies (up to KNEE_FREQ)
        const TARGET_LINEARE_FREQ: f64 = 500.0; // Target knee frequency
        let bins_in_linear_range = (TARGET_LINEARE_FREQ / frequency_resolution).ceil() as usize;

        // Determine optimal bar allocation to prevent starvation
        // Rule: Each bar should have at least 1 bin of unique data
        let max_linear_bars = bins_in_linear_range.min(config.num_bars / 3); // limit to 1/3 of bars

        //  Adaptively set lineer propposrt based on available bins
        let lineaer_bar_proportion = if bins_in_linear_range < 10 {
            // Very low resolution: use pure logarithmic
            0.0
        } else if bins_in_linear_range < 20 {
            // low resolution: minimal linear
            (max_linear_bars as f64 / config.num_bars as f64).min(0.1) 
        } else {
            // Good resolution: can afford more lineaer bars
            (max_linear_bars as f64 / config.num_bars as f64).min(0.2)
        };

        let linear_bar_count = (config.num_bars as f64 * lineaer_bar_proportion).round()  as usize;
        let log_bar_count = config.num_bars - linear_bar_count;

        println!("[Bar Mapping] Resolution: {:.2} Hz/bin", frequency_resolution);
        println!("[Bar Mapping] Bins in 0-500Hz: {}", bins_in_linear_range);
        println!("[Bar Mapping] Adaptive allocation: {} linear, {} log bars", 
             linear_bar_count, log_bar_count);

        // === LINEAR SECTION (0 Hz to ) ===
        // Bars are linearly distributed across frequncy
        if linear_bar_count > 0  && bins_in_linear_range > linear_bar_count {
            // Distribute bars evenly across availabe low-frequency bins
            for i in 0..linear_bar_count {
                let start = (i * bins_in_linear_range) / linear_bar_count.min(max_fft_bin - 1);
                let end = ((i + 1) * bins_in_linear_range / linear_bar_count)
                    .min(max_fft_bin)
                    .max(start + 1); // Ensure at least 1 bin per bar

                map.push((start, end));
            }
        }
        

        // === LOGARITHMIC SECTION ===
        let actual_log_bars = config.num_bars - map.len();
        if actual_log_bars > 0 {
            let start_bin = if map.is_empty() { 0 } else { map.last().unwrap().1 };
            let start_freq = start_bin as f64 * frequency_resolution;

            // Use octave-based distribution for musical relevance
            // each octave gets exponentially more bars
            let octaves = (nyquist_freq / start_freq.max(frequency_resolution)).log2();

            for i in 0..actual_log_bars {
                let prev_end = if i == 0 {
                    start_bin
                } else {
                    map.last().unwrap().1
                };

                // Exponential distribution across remaining spectrum
                let t = (i + 1) as f64 / actual_log_bars as f64;

                // use a power function for smooth exponential growth
                // adjust the exponent for different visual emphasis
                let exponent = 2.0;  // Highher = more bars for higher frequencies
                let freq_normalized = t.powf(exponent);

                let target_freq = start_freq + (nyquist_freq - start_freq) * freq_normalized;
                let target_bin = (target_freq / frequency_resolution).round() as usize;

                let end = target_bin.min(max_fft_bin).max(prev_end + 1);
                
                map.push((prev_end, end));
            }

            // Ensure we reach the end of the spectrum
            if let Some(last_bar) = map.last_mut() {
                last_bar.1 = max_fft_bin;
            }
        }

        // === STARVATION CHECK ===
    #[cfg(debug_assertions)]
    {
        let mut single_bin_bars = 0;
        let mut multi_bin_bars = 0;
        let mut max_bin_span = 0;
        
        for &(start, end) in &map {
            let span = end - start;
            if span == 1 {
                single_bin_bars += 1;
            } else {
                multi_bin_bars += 1;
            }
            max_bin_span = max_bin_span.max(span);
        }
        
        println!("\n=== Mapping Quality Report ===");
        println!("Single-bin bars: {} ({:.1}%)", 
                 single_bin_bars, 
                 single_bin_bars as f64 / map.len() as f64 * 100.0);
        println!("Multi-bin bars:  {} ({:.1}%)", 
                 multi_bin_bars,
                 multi_bin_bars as f64 / map.len() as f64 * 100.0);
        println!("Max bin span: {} bins", max_bin_span);
        
        // Warn about potential issues
        if single_bin_bars as f64 / map.len() as f64 > 0.5 {
            println!("⚠️  Warning: >50% of bars are single-bin (low frequency resolution)");
            println!("   Consider: Increasing FFT size or reducing bar count");
        }
        
        if max_bin_span > 50 {
            println!("⚠️  Warning: Some bars span {} bins (may lose detail)", max_bin_span);
            println!("   Consider: Increasing bar count for better high-frequency detail");
        }
    }

        map
    }

    // Group FFT bin data into visualization bars
    fn group_bins(&self, magnitudes: &[f32]) -> Vec<f32> {
        self.bar_to_bin_map
            .iter()
            .map(|&(start, end)| {

                
                if self.config.use_peak_aggregation {
                    // PEAK MODE: take the maximum value in the range
                    // this creates a more dramatic, responsive visual effect.
                    magnitudes[start..end]
                        .iter()
                        .copied()
                        .fold(f32::NEG_INFINITY, f32::max)
                } else {
                    // AVERAGE MODE: take the average value in the range
                    // this creates a smoother, more stable visual effect.
                    let sum: f32 = magnitudes[start..end].iter().sum();
                    let count = (end - start) as f32;
                    sum / count.max(1.0)
                }
            })
            .collect()
    }

    // Apply attack/releaser smoothing
    fn apply_smoothing(&mut self, raw_bars: &[f32], delta_ms: f32) -> Vec<f32> {
        let attack_factor = (delta_ms / self.config.attack_time_ms).min(1.0);
        let release_factor = (delta_ms / self.config.release_time_ms).min(1.0);

        for (i, &raw) in raw_bars.iter().enumerate() {
            let last = self.last_bar_heights[i];
            
            // if new value is higher, use attack time
            // if new value is lower, use release time
            let smoothed = if raw > last {
                last + (raw - last) * attack_factor
            } else {
                last + (raw - last) * release_factor
            };

            self.last_bar_heights[i] = smoothed;
        }

        self.last_bar_heights.clone()
    }

    fn update_peaks(&mut self, bars: &[f32], delta_ms: f32) -> Vec<f32> {
        for (i, &bar_height) in bars.iter().enumerate() {
            // if current bar exceeds peak, reset the peak
            if bar_height > self.peak_levels[i] {
                self.peak_levels[i] = bar_height;
                self.peak_hold_timers[i] = self.config.peak_hold_time_ms;
            } else{
                // decrement the hold timer
                self.peak_hold_timers[i] -= delta_ms;

                // if the hold expired, let peak fall!
                if self.peak_hold_timers[i] <= 0.0 {
                    let release_factor = (delta_ms / self.config.peak_release_time_ms).min(1.0);
                    self.peak_levels[i] -= (self.peak_levels[i] - bar_height) * release_factor;

                    // Never fall below current bar
                    if self.peak_levels[i] < bar_height {
                       self.peak_levels[i] = bar_height;
                    }
                }
                
            }
        }

        self.peak_levels.clone()
    }

    // Get a copy of the current configuration
    pub fn get_config(&self) -> FFTConfig {
        self.config.clone()
    }
}

// ===========  Tests ===============
#[cfg(test)]
mod tests {
    use crate::AudioPacket;

    use super::*;

    /// Helper to convert bin index to frequency
    fn bin_to_freq(bin: usize, sample_rate: u32, fft_size: usize) -> f32 {
        bin as f32 * (sample_rate as f32 / fft_size as f32)
    }

    #[test]
    fn test_musical_mapping_audit() {
        // Standard Setup: 48kHz, 1024 FFT (46.875 Hz/bin), 150 bars
        let config = FFTConfig {
            sample_rate: 48000,
            fft_size: 1024,
            num_bars: 150,
            ..Default::default()
        };
        
        let map = FFTProcessor::compute_bar_mapping(&config);
        
        println!("\n=== MAPPING AUDIT (48kHz / 1024 FFT / 150 Bars) ===");
        println!("Resolution: {:.2} Hz per bin", 48000.0 / 1024.0);
        
        let mut linear_count = 0;
        let mut log_count = 0;
        let mut transition_freq = 0.0;

        for (i, &(start, end)) in map.iter().enumerate() {
            let freq_start = bin_to_freq(start, config.sample_rate, config.fft_size);
            let freq_end = bin_to_freq(end, config.sample_rate, config.fft_size);
            let width_hz = freq_end - freq_start;
            let width_bins = end - start;

            // Detect the transition (Linear bars usually have small, constant bin widths)
            // Log bars grow rapidly.
            let region = if freq_start < 500.0 {
                linear_count += 1;
                "LIN" 
            } else {
                if log_count == 0 { transition_freq = freq_start; }
                log_count += 1;
                "LOG"
            };

            // Print every 5th bar to keep output readable, but ALWAYS print around the knee
            if i % 10 == 0 || (freq_start > 400.0 && freq_start < 600.0) {
                println!(
                    "Bar {:03} [{}] | {:>4} - {:>4} bins | {:>7.1} - {:>7.1} Hz | Width: {:>5.1} Hz",
                    i, region, start, end, freq_start, freq_end, width_hz
                );
            }
        }

        println!("===================================================");
        println!("Linear Bars: {} (Target: ~{})", linear_count, (150.0 * 0.4) as usize);
        println!("Log Bars:    {}", log_count);
        println!("Knee Freq:   ~{:.1} Hz (Target: 500.0 Hz)", transition_freq);
        println!("===================================================\n");

        // ASSERTIONS
        // 1. Check if we actually allocated ~40% of bars
        let expected_linear = (config.num_bars as f64 * 0.40).round() as usize;
        assert_eq!(linear_count, expected_linear, "Linear bar count mismatch");

        // 2. Check if the knee is roughly correct (allow 1 bin of slop)
        let bin_res = 48000.0 / 1024.0;
        assert!(
            (transition_freq - 500.0).abs() < bin_res * 2.0, 
            "Knee frequency {:.1} Hz is too far from 500.0 Hz", transition_freq
        );
    }

    #[test]
    fn test_mapping_continuity() {
        let config = FFTConfig::default();
        let map = FFTProcessor::compute_bar_mapping(&config);

        // Ensure no gaps between bars
        for i in 0..map.len() - 1 {
            let current_end = map[i].1;
            let next_start = map[i+1].0;
            
            assert!(
                next_start >= current_end, 
                "Bar overlapping at index {}: {} vs {}", i, current_end, next_start
            );
            
            // Note: Small gaps are technically allowed in some log mapping algorithms 
            // to maintain integer bin boundaries, but ideally they should be contiguous.
            assert!(
                next_start - current_end <= 1, 
                "Large gap detected between bar {} and {}", i, i+1
            );
        }
    }
    
    #[test]
    fn test_hann_window() {
        let window = FFTProcessor::compute_hann_window(1024);

        let size = 1024;

        // small tolerance for floating point comparisons
        let epsilon = 1e-5; 

        // Test 1:  Window should start at 0.0
        let expected_start = 0.0;
        assert!(
            (window[0] - expected_start).abs() < epsilon,
            "Window start was {}, expected {}", window[0], expected_start
        );


        // test 2: Window should end at 0.0
        let expected_end = 0.0;
        assert!(
            (window[size - 1] - expected_end).abs() < epsilon,
            "Window end was {}, expected {}", window[size - 1], expected_end
        );

        // Test 3: Window should peak at 1.0 in the middle
        let expected_peak = 1.0;
        assert!(
            (window[size / 2] - expected_peak).abs() < epsilon,
            "Window peak was {}, expected {}", window[size / 2], expected_peak
        );
        
        
    }

    #[test]
    fn test_mono_conversion() {
        let packet = AudioPacket{
            samples: vec![0.5, 0.3, 0.7, 0.1],
            sample_rate: 48000,
            channels: 2,
            timestamp: std::time::Instant::now(),
        };

        let mono = packet.to_mono();

        assert_eq!(mono.len(), 2);
        assert_eq!(mono[0], (0.5 + 0.3)/ 2.0);
        assert_eq!(mono[1], (0.7 + 0.1) / 2.0);
    
    }

    #[test]
    fn test_smoothing_attack() {
        let mut config = FFTConfig::default();
        config. num_bars = 4;
        config.attack_time_ms = 100.0;
        config.release_time_ms = 100.0;

        let mut processor = FFTProcessor::new(config);

        // First frame: bars should rise quickly
        let raw_bars = vec![10.0, 20.0, 30.0, 40.0];
        let smoothed_bars = processor.apply_smoothing(&raw_bars, 10.0);
        
        // Should be 10% of the way there (10ms / 100ms attack time)
        assert!(smoothed_bars[0] > SILENCE_DB, "Bar did not rise from silence");
        assert!(smoothed_bars[0] < 10.0, "Bar rose too fast (instant attack)");
    } 

    #[test]
    fn test_peak_hold() {
        let mut config = FFTConfig::default();
        config.num_bars = 2;
        config.peak_hold_time_ms = 500.0;
        
        let mut processor = FFTProcessor::new(config);

        // High bar value
        let bars = vec![50.0, 50.0];
        let peaks = processor.update_peaks(&bars, 10.0);

        assert_eq!(peaks[0], 50.0);
        
        // Lower bar value, but peak should hold
        let bars = vec![30.0, 30.0];
        let peaks = processor.update_peaks(&bars, 10.0);
        assert_eq!(peaks[0], 50.0);
    }
    
}




            