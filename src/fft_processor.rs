use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;
use crate::{fft_config::FIXED_FFT_SIZE, shared_state::SILENCE_DB};

// === GLOBAL CONSTANTS FOR MAPPING  ===
// These define the "physics" of how wer map frequncies to visual bars
// We exposed them publicly so the GUI can use them, too.
pub const MAPPING_LINEAR_PROPORTION: f64 = 0.15; // Target 15% Bass (Your choice)
pub const MAPPING_KNEE_FREQ: f64 = 500.0;            // 0-500Hz is Linear
pub const MAPPING_MAX_FREQ: f64 = 20000.0;           // Hard limit at 20kHz
// ===================

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
            fft_size: FIXED_FFT_SIZE,
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
type BarToBinMap = Vec<f64>;

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
        let peak_hold_timers = vec![0.0; config.num_bars];

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

        // step 6: Update peaks
        let peaks = self.update_peaks(&smoothed_bars, delta_ms);

        (smoothed_bars, peaks)
    }

    #[allow(dead_code)]
    /// Update configuration (e.g., user changed the number of bars)
    pub fn update_config(&mut self, config: FFTConfig) {

        // Sample Rate chanmge triggers a full rebuild, not an update

        if config.num_bars != self.config.num_bars {
            self.last_bar_heights.resize(config.num_bars, SILENCE_DB);
            self.peak_levels.resize(config.num_bars, SILENCE_DB);
            self.peak_hold_timers.resize(config.num_bars, 0.0);
            
            // Recomput the mapping
            self.bar_to_bin_map = Self::compute_bar_mapping(&config);
        }

        self.config = config;
    }

    /// Public Helper: Calculate frequency for a specific bar index
    /// Centralized logic to ensure GUI and Audio math always match
    pub fn calculate_bar_frequency(
        bar_index: usize,
        total_bars: usize,
        sample_rate: u32,
        fft_size: usize,
    ) -> f32 {
        let freq_res = sample_rate as f64 / fft_size as f64;
        let linear_bar_count = (total_bars as f64 * MAPPING_LINEAR_PROPORTION).round() as usize;

        // 1. Check linear region
        if bar_index < linear_bar_count {
            let t = (bar_index + 1) as f64 / linear_bar_count as f64;
            return (t * MAPPING_KNEE_FREQ) as f32;
        }

        // 2. Check log region
        let log_bar_count = total_bars - linear_bar_count;
        let log_index = bar_index - linear_bar_count;

        let t = (log_index + 1) as f64 / log_bar_count as f64;
        let min_log_freq = MAPPING_KNEE_FREQ.max(freq_res); // Start where lineaer left off

        (min_log_freq * (MAPPING_MAX_FREQ / min_log_freq).powf(t)) as f32
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

    /// Convert FFT output to dB magnitudes with sensitivity
    /// 
    /// Normalization strategy:
    /// - FFT output scales with FFT size, so we normalize by sqrt(N) for energy preservation
    /// - Hann window reduces energy by ~0.5, so we correct by 2.0
    /// - We use sqrt(N) instead of N/2 because we want ENERGY scaling, not amplitude
    ///   This preserves the dynamic range between loud and quiet frequency content
    /// - Sensitivity is applied as a pre-log multiplier to maintain perceptual linearity
    ///
    /// For a 2048-point FFT:
    /// - sqrt(2048) ≈ 45.25
    /// - Combined factor: 2.0 / 45.25 ≈ 0.044
    /// - A full-scale sine produces ~22.6 magnitude → ~0.996 normalized → ~0 dB ✓
    /// - But real music with spread energy stays dynamic!
    fn compute_magnitudes(&self) -> Vec<f32> {
       // Hann window correction (window averages 0.5, so multiply by 2)
        const HANN_CORRECTION: f32 = 2.0;
        
        // Use sqrt(N) normalization for energy-preserving scaling
        // This is gentler than N/2 and preserves inter-bin dynamics
        let fft_normalization = 1.0 / (self.config.fft_size as f32).sqrt();
        
        // Combined normalization factor
        let normalization = HANN_CORRECTION * fft_normalization;

        self.output_buffer
            .iter()
            .map(|&mag| {
                // 1. Apply normalization (energy-preserving)
                let normalized = mag * normalization;
                
                // 2. Apply sensitivity BEFORE log (preserves dynamic range perception)
                //    sensitivity > 1.0 = boost quiet content
                //    sensitivity < 1.0 = reduce overall level  
                //    sensitivity = 1.0 = calibrated for loud mastered music (~0 dBFS peaks)
                let adjusted = normalized * self.config.sensitivity;

                // 3. Convert to dB scale
                //    Full scale (1.0) → 0 dB
                //    -6 dB per halving of amplitude
                20.0 * (adjusted + 1e-10).log10()
            })
            .collect()
    }

    /// Perform a linear-log hybrid mapping of the FFT data to visualization bars
    fn compute_bar_mapping(config: &FFTConfig) -> BarToBinMap {
        let mut map = Vec::with_capacity(config.num_bars);

        let frequency_resolution = config.sample_rate as f64 / config.fft_size as f64;
        
        let linear_bar_count = (config.num_bars as f64 * MAPPING_LINEAR_PROPORTION).round() as usize;
        let log_bar_count = config.num_bars - linear_bar_count;

        // === LINEAR SECTION (0 Hz to ) ===
        for i in 0..linear_bar_count {
            let freq_target = (i + 1) as f64 / linear_bar_count as f64 * MAPPING_KNEE_FREQ;
            let bin_pos = freq_target / frequency_resolution;
            map.push(bin_pos);
        }
    
        // === LOGARITHMIC SECTION ===
        let min_log_bars = MAPPING_KNEE_FREQ.max(frequency_resolution);

        for i in 0..log_bar_count {
            let t = (i + 1) as f64 / log_bar_count as f64;
            // True Logarithmic Interpolation
            let freq_target = min_log_bars * (MAPPING_MAX_FREQ / min_log_bars).powf(t);
            let bin_pos = freq_target / frequency_resolution;
            map.push(bin_pos);
        
        }

        map
    }

    fn interpolate_hermite(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
        let c0 = y1;
        let c1 = 0.5 * (y2 - y0);
        let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let c3 = 0.5 * (y3 - y0 + 3.0 * (y1 - y2)); 
        
        ((c3 * t + c2) * t + c1) * t + c0
    }

    // Group FFT bin data into visualization bars
    fn group_bins(&self, magnitudes: &[f32]) -> Vec<f32> {
        let max_bin_idx = magnitudes.len().saturating_sub(1);

        self.bar_to_bin_map
            .iter()
            .map(|&bin_pos| {
                if bin_pos < 0.0 || bin_pos >= max_bin_idx as f64 {
                    return SILENCE_DB;
                }

                let idx = bin_pos.floor() as usize;
                let t = (bin_pos - idx as f64) as f32;

                // Get surrounding bins for interpolation
                let y1 = magnitudes[idx];
                let y2 = if idx + 1 <= max_bin_idx { magnitudes[idx + 1] } else { y1 };
                let y0 = if idx > 0 { magnitudes[idx -1] } else { y1 };
                let y3 = if idx + 2 <= max_bin_idx { magnitudes[idx + 2] } else { y2 };

                // Hermite interpolation
                Self::interpolate_hermite(y0, y1, y2, y3, t)
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




            