/// FFT configuration adapter for dynamic sample rate handling
/// Ensures FFT settings are always optimal for the current device's sample rate

use std::collections::HashMap;
use lazy_static::lazy_static;

/// Represents optimal FFT settings for a given sample rate
#[derive(Clone, Debug)]
pub struct FFTSampleRateConfig {
    /// Sample rate in Hz
    pub sample_rate: u32,

    /// Optimal FFT size for this sample rate
    /// Generally: FFT size should be ~50-100ms of audio
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
        /// Determine FFT size: aim for ~50-100ms of audio
        let fft_size = match sample_rate{
            8000..=16000 => 512,        // ~32-64ms at 8-16kHz
            16001..=32000 => 1024,      // ~32-64ms at 15-32kHz
            32001..=48000 => 2048,      // ~42-85ms at 32-48kHz
            48001..=96000 => 4096,      // ~42-85ms at 48-96kHz
            _ => 8192,                  // ~85ms+ at 96kHz+
        };

        // calculate frequency resolution
        let freq_resolution = sample_rate as f32 / fft_size as f32;

        // Recommend bar count based on useful frequency range
        // most music is 20Hz-20kHz range
        let nyquist = sample_rate / 2;
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

    /// Get a preset configuration for common sample rates
    pub fn get_preset(sample_rate: u32) -> Option<Self> {
        PRESET_CONFIGS
            .get(&sample_rate)
            .cloned()
    }

    /// Print debug infomration about this configuration
    pub fn debug_print(&self) {
        println!("[FFTConfig] {}", self.description);
        println!("  Nyquist: {} Hz", self.nyquist_frequency);
        println!("  Frequency Resolution: {:.2} Hz/bin", self.frequency_resolution);
    }
}


/// Preset FFT configuration for common sample rates
lazy_static::lazy_static! {
    static ref PRESET_CONFIGS: HashMap<u32, FFTSampleRateConfig> = {
        let configs = vec![
            // Common Sample Rates
            (44100, FFTSampleRateConfig {
                sample_rate: 44100,
                fft_size: 2048,
                recommended_bars: 150,
                frequency_resolution: 21.53,
                nyquist_frequency: 22050.0,
                description: "44.1 kHz (CD quality)".to_string(),
            }),
            (48000, FFTSampleRateConfig {
                sample_rate: 48000,
                fft_size: 2048,
                recommended_bars: 150,
                frequency_resolution: 23.44,
                nyquist_frequency: 24000.0,
                description: "48 kHz (professional audio)".to_string(),
            }),
            (96000, FFTSampleRateConfig {
                sample_rate: 96000,
                fft_size: 4096,
                recommended_bars: 200,
                frequency_resolution: 23.44,
                nyquist_frequency: 48000.0,
                description: "96 kHz (high-resolution)".to_string(),
            }),
            (192000, FFTSampleRateConfig {
                sample_rate: 192000,
                fft_size: 8192,
                recommended_bars: 256,
                frequency_resolution: 23.44,
                nyquist_frequency: 96000.0,
                description: "192 kHz (ultra high-resolution)".to_string(),
            }),
            (32000, FFTSampleRateConfig {
                sample_rate: 32000,
                fft_size: 1024,
                recommended_bars: 100,
                frequency_resolution: 31.25,
                nyquist_frequency: 16000.0,
                description: "32 kHz (narrowband)".to_string(),
            }),
            (16000, FFTSampleRateConfig {
                sample_rate: 16000,
                fft_size: 512,
                recommended_bars: 64,
                frequency_resolution: 31.25,
                nyquist_frequency: 8000.0,
                description: "16 kHz (telephony)".to_string(),
            }),
        ];
        configs.into_iter().collect()
    };
}

/// Valid FFT size range (all powers of 2)
const MIN_FFT_SIZE: usize = 256;
const MAX_FFT_SIZE: usize = 16384;

/// Manages FFT configuration based on detected device sample rate
/// Also handles user override of FFT size for viusalization preferences
pub struct FFTConfigManager {
    
    /// Current detected sample rate from device
    current_sample_rate: u32,
    
    /// Base config (from sample rate detection)
    current_config: FFTSampleRateConfig,
    
    /// Cache to avoid recalculation
    config_cache: HashMap<u32, FFTSampleRateConfig>,

    /// Users manual FFT size override (if dictated by user vice auto-calculated)
    fft_size_override: Option<usize>,
}

/// Public result of FFT configuration
/// Everything you need to know about current state
#[derive(Debug, Clone, Default)]
pub struct FFTInfo {
    pub sample_rate: u32,
    pub fft_size: usize,
    pub recommended_fft_size: usize,
    pub is_overridden: bool,
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
            fft_size_override: None,

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

        let old_size = self.current_config.fft_size;
        let new_size = new_config.fft_size;
        
        // Only rebuild if the auto-selected FFT size changed
        let needs_rebuild = if self.fft_size_override.is_some() {
            // if User has override, sample rate change doesn't affect FFT size
            false
        } else {
            old_size != new_size
        };

        self.current_sample_rate = new_sample_rate;

        self.current_config = new_config.clone();

        println!(
            "[FFTConfigManager] Sample rate: {} Hz → {} Hz (rebuild: {})",
            new_sample_rate, old_size, needs_rebuild
        );
        needs_rebuild
    }
     

    /// User manually sets FFT size (eg, via UI slider)
    /// Returns true if this requires FFT processor rebuild 
    pub fn set_override(&mut self, fft_size: usize) -> Result<bool, String> {
        
        if !Self::is_valid_fft_size(fft_size) {
            return Err(format!(
                "Invalid FFT size: {} (must be a power of 2, {}-{})",
                fft_size, MIN_FFT_SIZE, MAX_FFT_SIZE
            ));
        }

        let old_size = self.get_fft_size();
        let new_size = fft_size;
        let needs_rebuild = old_size != new_size;

        self.fft_size_override = Some(fft_size);

        println!(
            "[FFTConfigManager] FFT override: {} → {} (rebuild: {})",
            old_size, new_size, needs_rebuild
        );

        Ok(needs_rebuild)
    }

    /// Clear user override and go back to automatic sizing
    /// Returns true if FFT processor rebuild needed
    pub fn clear_override(&mut self) -> bool {
        if self.fft_size_override.is_none() {
            return false; // no change
        }

        let old_size = self.get_fft_size();
        self.fft_size_override = None;
        let new_size = self.get_fft_size();

        let needs_rebuild = old_size != new_size;

        println!(
            "[FFTConfigManager] FFT override cleared (rebuild: {})",
            needs_rebuild
        );

        needs_rebuild
    }

    // ======= Query Methods ========
    pub fn info(&self) -> FFTInfo {
        let fft_size = self.get_fft_size();
        let latency_ms = self.latency_ms();

        FFTInfo {
            sample_rate: self.current_sample_rate,
            fft_size,
            recommended_fft_size: self.current_config.fft_size,
            is_overridden: self.fft_size_override.is_some(),
            latency_ms,
            frequency_resolution: self.current_config.frequency_resolution,
            recommended_bars: self.current_config.recommended_bars,
        }  
    }

    /// Get the effective FFT size (override or auto)
    pub fn get_fft_size(&self) -> usize {
        self.fft_size_override
            .unwrap_or(self.current_config.fft_size)
    }

    /// Get recommended FFT size (what we'd use if no override)
    pub fn get_recommended_fft_size(&self) -> usize {
        self.current_config.fft_size
    }

    /// Check if user has overridden FFT size
    pub fn has_override(&self) -> bool {
        self.fft_size_override.is_some()
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
        let fft_size = self.get_fft_size();
        (fft_size as f32 / self.current_sample_rate as f32) * 1000.0
    }

    /// Get a short latency warning emoji based on current state
    /// REturn: ("emoji", "description")
    pub fn latency_warning(&self) ->(&'static str, &'static str) {
        let latency = self.latency_ms();
        match latency {
            l if l < 10.0 => ("⚡", "very snappy"),
            l if l < 30.0 => ("🟢", "responsive"),
            l if l < 85.0 => ("🟡", "good detail"),
            _ => ("🔴", "may lag"),
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

    /// Get all valid FFT sizes aas slice (compute once, shared)
    pub fn valid_fft_sizes() -> &'static [usize] {
        &VALID_FFT_SIZES
    }

    /// Check if an FFT size is valid (power of 2, in range)
    fn is_valid_fft_size(size: usize) -> bool {
        size.is_power_of_two() && size >= MIN_FFT_SIZE && size <= MAX_FFT_SIZE
    }
}


/// Pre-computed valid FFT sizes
lazy_static::lazy_static! {
    static ref VALID_FFT_SIZES: Vec<usize> = {
        let mut sizes = Vec::new();
        let mut size = MIN_FFT_SIZE;
        while size <= MAX_FFT_SIZE {
            sizes.push(size);
            size *= 2;
        }
        sizes
    };
}

// =============== Tests ==================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fft_config_for_sample_rate() {
        let configs = vec![
            (8000, 512),
            (16000, 512),
            (32000, 1024),
            (44100, 2048),
            (48000, 2048),
            (96000, 4096),
            (192000, 8192),
        ];
        
        for (rate, expected_fft) in configs {
            let config = FFTSampleRateConfig::for_sample_rate(rate);
            assert_eq!(config.sample_rate, rate);
            assert_eq!(config.fft_size, expected_fft);
            assert_eq!(config.nyquist_frequency, (rate / 2) as f32);
            println!("✓ {}", config.description);

        }
    }

    #[test]
    fn test_config_manager_initialization() {
        let manager = FFTConfigManager::new(48000);
        assert_eq!(manager.get_sample_rate(), 48000);
        assert_eq!(manager.get_fft_size(), 2048);
        assert!(!manager.has_override());

    }

    #[test]
    fn test_sample_rate_change_without_override() {
        let mut manager = FFTConfigManager::new(48000);

        // Switch to 96kHz - should trigger rebuild (FFT size change)
        let rebuild = manager.update_sample_rate(96000);
        assert!(rebuild);
        assert_eq!(manager.get_fft_size(), 4096);

        // switch back to 48kHz - should trigger rebuild
        let rebuild = manager.update_sample_rate(48000);
        assert!(rebuild);
        assert_eq!(manager.get_fft_size(), 2048);

        // No Change - no rebuild
        let rebuild = manager.update_sample_rate(48000);
        assert!(!rebuild);
    }

    #[test]
    fn test_sample_rate_change_with_override() {
        let mut manager = FFTConfigManager::new(48000);

        // Invalid: not power of 2
        assert!(manager.set_override(3000).is_err());
        assert!(!manager.has_override());

        // Invalid: too small
        assert!(manager.set_override(128).is_err());

        // Invalid: too large
        assert!(manager.set_override(32768).is_err());

        // Valid
        assert!(manager.set_override(2048).is_ok());
        assert!(manager.has_override());
    }

    #[test]
    fn test_override_trigger_rebuild() {
        let mut manager = FFTConfigManager::new(48000);
        assert_eq!(manager.get_fft_size(), 2048);
        
        // Set override to different size - rebuild needed
        let rebuild = manager.set_override(4096).unwrap();
        assert!(rebuild);

        // set override to same size - no rebuild
        let rebuild = manager.set_override(4096).unwrap();
        assert!(!rebuild);

        // Clear override - rebuild needed (back to 2048)
        let rebuild = manager.clear_override();
        assert!(rebuild);
        assert_eq!(manager.get_fft_size(), 2048);
    }

    #[test]
    fn test_latency_calculation() {
        let mut manager = FFTConfigManager::new(48000);
        let latency = manager.latency_ms();
        assert!((latency - 42.67).abs() < 1.0);

        manager.set_override(1024).unwrap();
        let latency = manager.latency_ms();
        assert!((latency - 21.33).abs() < 1.0);

        manager.set_override(8192).unwrap();
        let latency = manager.latency_ms();
        assert!((latency - 170.67).abs() < 1.0);
    }

    #[test]
    fn test_latency_warning() {
        let mut manager = FFTConfigManager::new(48000);

         manager.set_override(512).unwrap();
         let (emoji, _) = manager.latency_warning();
         assert_eq!(emoji, "⚡");

         manager.set_override(2048).unwrap();
         let (emoji, _) = manager.latency_warning();
         assert_eq!(emoji, "🟢");

         manager.set_override(8192).unwrap();
         let (emoji, _) = manager.latency_warning();
        assert_eq!(emoji, "🔴");
    }

    #[test]
    fn test_fft_info_struct() {
        let mut manager = FFTConfigManager::new(48000);
        let info = manager.info();

        assert_eq!(info.sample_rate, 48000);
        assert_eq!(info.fft_size, 2048);
        assert_eq!(info.recommended_fft_size, 2048);
        assert!(!info.is_overridden);

        manager.set_override(4096).unwrap();
        let info = manager.info();

        assert_eq!(info.fft_size, 4096);
        assert!(info.is_overridden);
    }

    #[test]
    fn test_frequency_bin_mapping() {
        let manager = FFTConfigManager::new(48000);

        let bin = manager.bin_for_frequency(1000.0);
        let freq_back = manager.frequency_for_bin(bin);
        assert!((freq_back - 1000.0).abs() < 50.0);

        let dc_bin = manager.bin_for_frequency(0.0);
        assert_eq!(dc_bin, 0);
    }

    #[test]
    fn test_valid_fft_sizes() {
        let sizes = FFTConfigManager::valid_fft_sizes();

        assert_eq!(sizes[0], 256);
        assert_eq!(sizes[sizes.len() - 1], 16384);

        for &size in sizes {
            assert!(size.is_power_of_two());
        }
    }

}
        
