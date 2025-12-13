/// FFT configuration adapter for dynamic sample rate handling
/// Ensures FFT settings are always optimal for the current device's sample rate


/// Fixed FFT size for the application
/// 2048 provides a good balance of frequency resolution and latency:
/// - At 48kHz: 42.7ms latency, 23.4 Hz/bin resolution
/// - At 44.1kHz: 46.4ms latency, 21.5 Hz/bin resolution
/// - At 96kHz: 21.3ms latency, 46.9 Hz/bin resolution
pub const FIXED_FFT_SIZE: usize = 2048;

/// Public result of FFT configuration
/// Everything you need to know about current state
#[derive(Debug, Clone, Default)]
pub struct FFTInfo {
    pub sample_rate: u32,
    pub fft_size: usize,
    pub latency_ms: f32,
    pub frequency_resolution: f32,
}

/// Manages FFT configuration based on detected device sample rate
/// Also handles user override of FFT size for viusalization preferences
pub struct FFTConfigManager {
    current_sample_rate: u32,
    frequency_resolution: f32,
}

impl FFTConfigManager {
    /// Create a new FFT config manager with default sample rate
    pub fn new(sample_rate: u32) -> Self {
        Self {
            current_sample_rate: sample_rate,
            frequency_resolution: Self::calc_resolution(sample_rate),
        }
    }

    /// Update to a new sample rate 
    /// Returns true if FFT processor rebuild needed
    pub fn update_sample_rate(&mut self, new_sample_rate: u32) -> bool {
        if new_sample_rate == self.current_sample_rate {
            return false;
        }

        tracing::info!(
            "[FFTConfigManager] Sample rate: {} Hz → {} Hz",
            self.current_sample_rate, new_sample_rate
        );
      
        self.current_sample_rate = new_sample_rate;
        self.frequency_resolution = Self::calc_resolution(new_sample_rate);
        true
    }
     
    // ======= Query Methods ========
    pub fn info(&self) -> FFTInfo {
        FFTInfo {
            sample_rate: self.current_sample_rate,
            fft_size: FIXED_FFT_SIZE,
            latency_ms: (FIXED_FFT_SIZE as f32 / self.current_sample_rate as f32) * 1000.0,
            frequency_resolution: self.frequency_resolution,
        }  
    }

    /// Get current sample rate
    pub fn get_sample_rate(&self) -> u32 {
        self.current_sample_rate
    }

    fn calc_resolution(rate: u32) -> f32 {
        rate as f32 / FIXED_FFT_SIZE as f32
    }
}

// =============== Tests ==================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
   fn test_resolution_varies_with_sample_rate() {
    let manager_48k = FFTConfigManager::new(48000);
    let manager_96k = FFTConfigManager::new(96000);

    // Higher sample rate =  higher frequency resolution (wider bins)
    // 96000 / 2048 = 46.875 Hz per bin
    // 48000 / 2048 = 23.437 Hz per bin
    assert!(manager_96k.info().frequency_resolution > manager_48k.info().frequency_resolution);

    // Verify calculation accuracy
    assert!((manager_48k.info().frequency_resolution - 23.4375).abs() < 0.01);
   }
   
   #[test]
   fn test_intialization() {
    let manager = FFTConfigManager::new(48000);
    let info = manager.info();

    assert_eq!(info.sample_rate, 48000);
    assert_eq!(info.fft_size, FIXED_FFT_SIZE);
   }

   #[test]
    fn test_sample_rate_update() {
        let mut manager = FFTConfigManager::new(48000);

        // 1. Update to new rate -> Should return true (changed)
        let changed = manager.update_sample_rate(96000);
        assert!(changed);
        assert_eq!(manager.get_sample_rate(), 96000);

        // 2. Update to same rate -> Should return false (no change)
        let changed = manager.update_sample_rate(96000);
        assert!(!changed);
        
        // 3. Update back -> Should return true
        let changed = manager.update_sample_rate(44100);
        assert!(changed);
        assert_eq!(manager.get_sample_rate(), 44100);
    }

    #[test]
    fn test_latency_calculation() {
        let manager = FFTConfigManager::new(48000);
        // 2048 samples / 48000 samples/sec = 0.04266 seconds = 42.67ms
        assert!((manager.info().latency_ms - 42.67).abs() < 0.1);
    }
}
        
