/// FFT configuration adapter for dynamic sample rate handling
/// Ensures FFT settings are always optimal for the current device's sample rate

use std::collections::HashMap;

/// Fixed FFT size for the application
/// 2048 provides a good balance of frequency resolution and latency:
/// - At 48kHz: 42.7ms latency, 23.4 Hz/bin resolution
/// - At 44.1kHz: 46.4ms latency, 21.5 Hz/bin resolution
/// - At 96kHz: 21.3ms latency, 46.9 Hz/bin resolution
pub const FIXED_FFT_SIZE: usize = 2048;


/// Represents optimal FFT settings for a given sample rate
#[derive(Clone, Debug)]
pub struct FFTSampleRateConfig {
    /// Sample rate in Hz
    pub sample_rate: u32,

    /// fft size (always FIXED_FFT_SIZE)
    pub fft_size: usize,

    /// Number of frequency bins to display
    /// Scaled to provide good frequency resolution at this sample rate
    pub recommended_bars: usize,

    /// Frequency resolution (Hz per bin)
    pub frequency_resolution: f32,

    /// Maximum useful frequency (Nyquist limit)
    pub nyquist_frequency: f32,

    /// Description for logging
    pub description: String,
}

impl FFTSampleRateConfig {
    /// Calculate configuration for any sample rate
    pub fn for_sample_rate(sample_rate: u32) -> Self {
        // Determine FFT size: aim for ~50-100ms of audio
        let fft_size= FIXED_FFT_SIZE;

        // calculate frequency resolution
        let freq_resolution = sample_rate as f32 / fft_size as f32;

        let nyquist = sample_rate / 2;

        // Recommend bar count based on useful frequency range
        // most music is 20Hz-20kHz range        
        let useful_freq_bins = 20000.0 / freq_resolution;
        let recommended_bars = (useful_freq_bins as usize).min(512).max(32);

        let description = format!(
            "{}Hz: FFT={}, bars={}, res={:.1}Hz/bin",
            sample_rate, fft_size, recommended_bars, freq_resolution
        );
        
        FFTSampleRateConfig {
            sample_rate,
            fft_size,
            recommended_bars,
            frequency_resolution: freq_resolution,
            nyquist_frequency: nyquist as f32,
            description,
        }
    }

    /// Print debug infomration about this configuration
    pub fn debug_print(&self) {
        println!("[FFTConfig] {}", self.description);
        println!("  Nyquist: {} Hz", self.nyquist_frequency);
        println!("  Frequency Resolution: {:.2} Hz/bin", self.frequency_resolution);
    }
}

/// Manages FFT configuration based on detected device sample rate
/// Also handles user override of FFT size for viusalization preferences
pub struct FFTConfigManager {
    
    /// Current detected sample rate from device
    current_sample_rate: u32,
    
    /// Base config (from sample rate detection)
    current_config: FFTSampleRateConfig,
    
    /// Cache to avoid recalculation
    config_cache: HashMap<u32, FFTSampleRateConfig>,
}

/// Public result of FFT configuration
/// Everything you need to know about current state
#[derive(Debug, Clone, Default)]
pub struct FFTInfo {
    pub sample_rate: u32,
    pub fft_size: usize,
    pub latency_ms: f32,
    pub frequency_resolution: f32,
    pub recommended_bars: usize,
}


impl FFTConfigManager {
    /// Create a new FFT config manager with default sample rate
    pub fn new(sample_rate: u32)  -> Self {
        let current_config = FFTSampleRateConfig::for_sample_rate(sample_rate);
        let mut config_cache = HashMap::new();
        config_cache.insert(sample_rate, current_config.clone());

        FFTConfigManager {
            current_sample_rate: sample_rate,
            current_config,
            config_cache,
        }
    }

    /// Update to a new sample rate 
    /// Returns true if FFT processor rebuild needed
    pub fn update_sample_rate(&mut self, new_sample_rate: u32) -> bool {
        if new_sample_rate == self.current_sample_rate {
            return false;
        }

        // Check cache first
        let new_config = self
            .config_cache
            .entry(new_sample_rate)
            .or_insert_with(|| FFTSampleRateConfig::for_sample_rate(new_sample_rate));

        println!(
            "[FFTConfigManager] Sample rate: {} Hz → {} Hz",
            self.current_sample_rate, new_sample_rate
        );
      
        self.current_sample_rate = new_sample_rate;
        self.current_config = new_config.clone();

        // Sample rate change affects frequency mapping, so return true
        true
    }
     
    // ======= Query Methods ========
    pub fn info(&self) -> FFTInfo {
        FFTInfo {
            sample_rate: self.current_sample_rate,
            fft_size: FIXED_FFT_SIZE,
            latency_ms: self.latency_ms(),
            frequency_resolution: self.current_config.frequency_resolution,
            recommended_bars: self.current_config.recommended_bars,
        }  
    }

    /// Get the effective FFT size (override or auto)
    pub fn get_fft_size(&self) -> usize {
        FIXED_FFT_SIZE
    }

    /// Get current sample rate
    pub fn get_sample_rate(&self) -> u32 {
        self.current_sample_rate
    }

    /// Get current base configuration
    pub fn get_current_config(&self) -> &FFTSampleRateConfig {
        &self.current_config
    }

    /// Calculate latency in milliseconds
    pub fn latency_ms(&self) -> f32 {
        (FIXED_FFT_SIZE as f32 / self.current_sample_rate as f32) * 1000.0
    }

    /// Get a short latency warning emoji based on current state
    /// REturn: ("emoji", "description")
    pub fn latency_indicator(&self) ->(&'static str, &'static str) {
        let latency = self.latency_ms();
        match latency {
            l if l < 20.0 => ("⚡", "very responsive"),
            l if l < 50.0 => ("🟢", "responsive"),
            l if l < 100.0 => ("🟡", "moderate"),
            _ => ("🔴", "high latency"),
        }
    }

    /// Get frequency of a specific  bin
    pub fn frequency_for_bin(&self, bin_index: usize) -> f32 {
        bin_index as f32 * self.current_config.frequency_resolution
    }

    /// Get bin index for a specific frequency
    pub  fn bin_for_frequency(&self, frequency: f32) -> usize {
        (frequency / self.current_config.frequency_resolution) as usize
    }
}

// =============== Tests ==================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_fft_size() {
        // All sample rates should use the same FFT size
        let configs = vec![
            FFTSampleRateConfig::for_sample_rate(44100),
            FFTSampleRateConfig::for_sample_rate(48000),
            FFTSampleRateConfig::for_sample_rate(96000),
            FFTSampleRateConfig::for_sample_rate(192000),
        ];

        for config in configs {
            assert_eq!(config.fft_size, FIXED_FFT_SIZE);
            println!("✓ {}", config.description)
        }
    }

    #[test]
    fn test_frequency_resolution_varies_with_sample_rate() {
        let config_48k = FFTSampleRateConfig::for_sample_rate(48000);
        let config_96k = FFTSampleRateConfig::for_sample_rate(96000);

        // Higher sample rate = higher frequency resolution
        assert!(config_96k.frequency_resolution > config_48k.frequency_resolution);

        // 48kHz: 48000 / 2048 = ~23.4 Hz/bin
        assert!((config_48k.frequency_resolution - 23.4375).abs() < 0.01);

        // 96kHz: 96000 / 2048 = ~46.9 Hz/bin
        assert!((config_96k.frequency_resolution - 46.875).abs() < 0.01);
    }

    #[test]
    fn test_config_manager_initialization() {
        let manager = FFTConfigManager::new(48000);
        assert_eq!(manager.get_sample_rate(), 48000);
        assert_eq!(manager.get_fft_size(), FIXED_FFT_SIZE);
    }

    #[test]
    fn test_sample_rate_change() {
        let mut manager = FFTConfigManager::new(48000);

        // Switch to 96kHz - should trigger rebuild (FFT size change)
        let changed: bool = manager.update_sample_rate(96000);
        assert!(changed);
        assert_eq!( manager.get_sample_rate(), 96000);
        assert_eq!(manager.get_fft_size(), FIXED_FFT_SIZE);

        // No Change!
        let changed = manager.update_sample_rate(96000);
        assert!(!changed);
        
        // Switch back
        let changed = manager.update_sample_rate(48000);
        assert!(changed);
    }

    #[test]
    fn test_latency_calculation() {
        let manage_48k = FFTConfigManager::new(48000);
        let manage_96k: FFTConfigManager = FFTConfigManager::new(96000);

        // 48kHz: 2048 / 48000 * 1000 = 42.67ms
        assert!((manage_48k.latency_ms() - 42.67).abs() < 0.1);

        // 96kHz: 2048 / 96000 * 1000 = 21.33ms        
        assert!((manage_96k.latency_ms() - 21.33).abs() < 0.1);
        
    }

    #[test]
    fn test_fft_info_struct() {
        let manager = FFTConfigManager::new(48000);
        let info = manager.info();

        assert_eq!(info.sample_rate, 48000);
        assert_eq!(info.fft_size, FIXED_FFT_SIZE);
        assert!((info.latency_ms - 42.67).abs() < 0.1);
        assert!((info.frequency_resolution - 23.4375).abs() < 0.01);
    }

     #[test]
    fn test_frequency_bin_mapping() {
        let manager = FFTConfigManager::new(48000);

        // Test round-trip
        let bin = manager.bin_for_frequency(1000.0);
        let freq_back = manager.frequency_for_bin(bin);
        assert!((freq_back - 1000.0).abs() < 30.0);

        let dc_bin = manager.bin_for_frequency(0.0);
        assert_eq!(dc_bin, 0);
    }
    #[test]
    fn test_latency_indicator() {
        // High sample rate = low latency
         let manager_96k = FFTConfigManager::new(96000);
         let (emoji, _) = manager_96k.latency_indicator();
         assert_eq!(emoji, "⚡");

         // Standard sample rate = responsive
         let manager_48k = FFTConfigManager::new(48000);
         let (emoji, _) = manager_48k.latency_indicator();
         assert_eq!(emoji, "🟢");

         let manager_16k = FFTConfigManager::new(16000);
         let (emoji, _) = manager_16k.latency_indicator();
        assert_eq!(emoji, "🔴");
    }
}
        
