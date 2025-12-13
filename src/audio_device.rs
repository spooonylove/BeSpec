/// Audio device enumeration and stream management
/// Hanldes device discovery, sample rate detectino, and dynmaic device switching
/// 

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::Device;
use std::fmt;

/// Represents a single audio output device with metadata
#[derive(Clone, Debug)]
pub struct AudioDeviceInfo {
    /// Unique identifier for the device
    pub id: String,

    /// Human-readable device name
    pub name: String,

    ///  Sample Rate(s) supported by this device (Hz)
    #[allow(dead_code)]
    pub sample_rates: Vec<u32>,

    /// Default/recommended sample rate for this device (Hz)
    pub default_sample_rate: u32,

    /// Number of output channels
    pub channels: u16,

    /// Whether this is the system default device
    pub is_default: bool,
}

impl fmt::Display for AudioDeviceInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let default_indicator = if self.is_default { " (default)" } else {""};
        write!(
            f,
            "{}{} - {} ch @ {} Hz",
            self.name, default_indicator, self.channels, self.default_sample_rate
        )
    }
}

/// Error types for audio device operateions
#[derive(Debug, Clone)]
pub enum AudioDeviceError {
    NoDevicesFound,
    DeviceNotFound(String),
    UnsupportedFormat,
    StreamCreationFailed(String),
    ConfigurationError(String),
}

impl fmt::Display for AudioDeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioDeviceError::NoDevicesFound => write!(f, "No audio devices found"),
            AudioDeviceError::DeviceNotFound(id) => write!(f, "Device not found: {}", id),
            AudioDeviceError::UnsupportedFormat => write!(f, "Unsupported sample format"),
            AudioDeviceError::StreamCreationFailed(msg) => {
                write!(f, "Stream creation failed: {}", msg)
            }
            AudioDeviceError::ConfigurationError(msg) => {
                write!(f, "Configuration error: {}", msg)
            }
        }
    }
}

impl std::error::Error for AudioDeviceError {}

/// Enumerates all available audio output devices and their capabilities
pub struct AudioDeviceEnumerator;

impl AudioDeviceEnumerator {
    /// get all available audio output devices
    pub fn enumerate_devices() -> Result<Vec<AudioDeviceInfo>, AudioDeviceError> {
        let host = cpal::default_host();
        let default_device = host.default_output_device();

        let mut devices = Vec::new();

        // Iterate through all output devices
        for device in host
            .output_devices()
            .map_err(|_| AudioDeviceError::NoDevicesFound)? 
        {
            match Self::extract_device_info(&device, default_device.as_ref()) {
                
                Ok(info) => devices.push(info),
                Err(e) => {
                    tracing::error!("[Audio] Failed to enumerate device: {}", e);
                    continue;
                }
            }
        }

        if devices.is_empty() {
            return Err(AudioDeviceError::NoDevicesFound);
        }

        Ok(devices)
    }

    /// Extract metadata from a device
    fn extract_device_info(
        device: &Device,
        default_device: Option<&Device>,
    ) -> Result<AudioDeviceInfo, AudioDeviceError> {
        let name = device
            .name()
            .unwrap_or_else(|_| "Unknown".to_string());

        let is_default = default_device
            .as_ref()
            .map(|d| {
                d.name()
                    .ok()
                    .map(|d_name| d_name == name)
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        let config = device
            .default_output_config()
            .map_err(|_| AudioDeviceError::ConfigurationError(
                format!("Could not get config for device: {}", name)
            ))?;

        let default_sample_rate = config.sample_rate().0;
        let channels = config.channels();

        // Discover supported sample rates
        let sample_rates = Self::get_sample_rates(device)?;

        Ok(AudioDeviceInfo {
            id: name.clone(),
            name,
            sample_rates,
            default_sample_rate,
            channels,
            is_default,
        })
    }

    /// Discover all supported sample rates for a device
    /// Tests common sample rates and returns those that are supported
    fn get_sample_rates(device: &Device) -> Result<Vec<u32>, AudioDeviceError> {
        // 1. Just get the current default. this is what we must use for loopback.
        // the covers 99.9% of use cases
        if let Ok(config) = device.default_output_config() {
            return Ok(vec![config.sample_rate().0]);
        }

        // 2. Fallback we slow scan!
        // Common professional and consumer sample rates
        let common_rates = vec![
            8000, 11025, 16000,
            22050, 32000, 44100,
            48000, 88200, 96000,
            176400, 192000,
        ];

        let mut supported_rates = Vec::new();

        for &rate in &common_rates {

            // Check if this configuration is supported
            let is_supported = device
                .supported_output_configs()
                .ok()
                .and_then(|mut configs| {
                    configs.find(|c| {
                        c.channels() == 2 &&
                        c.min_sample_rate() <= cpal::SampleRate(rate) &&
                        c.max_sample_rate() >= cpal::SampleRate(rate)
                    })
                })
                .is_some();

            if is_supported {
                supported_rates.push(rate);
            }
        }

        Ok(supported_rates)
    }

    /// Get a specific device by ID
    pub fn get_device_by_id(device_id: &str) -> Result<Device, AudioDeviceError> {
        let host = cpal::default_host();
        let devices = host
            .output_devices()
            .map_err(|_| AudioDeviceError::NoDevicesFound)?;

        for device in devices {
            if let Ok(name) = device.name() {
                if name == device_id {
                    return Ok(device);
                }
            }
        }

        Err(AudioDeviceError::DeviceNotFound(device_id.to_string()))
    }

    /// Get the default output device
    pub fn get_default_device() -> Result<(Device, AudioDeviceInfo), AudioDeviceError> {
        let host  = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioDeviceError::NoDevicesFound)?;

         let info = Self::extract_device_info(&device, Some(&device))?;

         Ok((device, info))
    }
}


// ================== Tests ===================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enumerate_devices() {
        match AudioDeviceEnumerator::enumerate_devices() {

            Ok(devices) => {
                assert!(!devices.is_empty(), "Should find at least one audio device");

                for device in &devices {
                    assert!(   
                        !device.name.is_empty(),
                        "Device name should not be empty"
                    );
                    assert!(
                        device.channels > 0,
                        "Device should have at least one channel"
                    );
                    assert!(
                        !device.sample_rates.is_empty(),
                        "Device should have at least one sample rate"
                    );
                    assert!(
                        device.default_sample_rate > 0,
                        "Default sample rate should be greater than zero"
                    );
                    assert!(
                        device.sample_rates.contains(&device.default_sample_rate),
                        "Default sample rate should be in supported rates"
                    );
                }

                tracing::info!("Found {} device(s)", devices.len());
                for device in devices {
                    tracing::info!("   {}", device);
                    tracing::info!(
                        "     Supported Rates : {} Hz",
                        device
                            .sample_rates
                            .iter()
                            .map(|r| r.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
            Err(e) => {
                tracing::error!("Error enumerating devices: {}", e);
            }
        }
    }

    #[test]
    fn test_get_default_device() {
        match AudioDeviceEnumerator::get_default_device() {
            Ok((_device, info)) => {
                tracing::info!("Default device: {}", info);
                assert!(!info.name.is_empty());
                assert!(info.is_default);
                assert!(info.channels > 0);
                assert!(info.default_sample_rate > 0);
            }
            Err(e) => {
                tracing::error!("Error getting default device: {}", e);
            }
        }

    }

    #[test]
    fn test_sample_rate_discovery() {
        match AudioDeviceEnumerator::get_default_device() {
            Ok((_device, info)) => {
                tracing::info!("Default device: {}", info.name);
                tracing::info!("Supported sample rates: {:?}", info.sample_rates);

                // Verify that common rates are tested
                assert!(!info.sample_rates.is_empty());
                
                // Verify rates are sorted and unique
                let mut sorted = info.sample_rates.clone();
                sorted.sort();
                sorted.dedup();
                assert_eq!(info.sample_rates.len(), sorted.len(), "Sample rates should be unique");
            }
            Err(e) => {
                tracing::error!("Error: {}", e);
            }
        }
    }
}