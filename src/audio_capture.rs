/// Enhanced audio capture with dynamic sample rate detection
/// Manages audio streams and adjusts FFT configuriation based on actual device capabilities
/// 

use cpal::traits::{DeviceTrait, StreamTrait};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::audio_device::{AudioDeviceEnumerator, AudioDeviceInfo, AudioDeviceError};

/// Audio packet containing raw samples and metadata
#[derive(Clone, Debug)]
pub struct AudioPacket {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    #[allow(dead_code)]
    pub timestamp: Instant,
}

impl AudioPacket {
    /// Convert multi-channel audio to mono by averaging channels
    pub fn to_mono(&self) -> Vec<f32> {
        
        // If already mono, easy day, return!
        if self.channels == 1{
            return self.samples.clone();
        }

        // If more than one channel, chunk the stream out **by the number of chunks**
        // then sum and devide accordingly. it averages >= 2 channels of audio.
        self.samples
            .chunks(self.channels as usize)
            .map(|frame| frame.iter().sum::<f32>() / self.channels as f32)
            .collect()
    
    }

    /// Get the duration of audio in this packet (in seconds)
    #[allow(dead_code)]
    pub fn duration_secs(&self) -> f32 {
        let num_samples = self.samples.len() / self.channels as usize;
        num_samples as f32 / self.sample_rate as f32
    }
}

/// Handles audio capture from a specific device
pub struct AudioCaptureManager {
    /// Information about the currently active device
    device_info: Arc<Mutex<AudioDeviceInfo>>,

    /// Sender for audio packets
    tx: Sender<AudioPacket>,

    /// Receiver for audio packets
    rx: Receiver<AudioPacket>,

    /// Shutdown signal
    shutdown: Arc<AtomicBool>,

    /// Handle to the capture thread
    capture_thread: Option<thread::JoinHandle<()>>,
}

impl AudioCaptureManager {
    /// Create a new audio capture manager with default device
    pub fn new() -> Result<Self, AudioDeviceError> {
        let (_device, device_info) = AudioDeviceEnumerator::get_default_device()?;
        Self::with_device(device_info)
    }

    /// Create a capture manager with a specific device ID
    pub fn with_device_id(device_id: &str) -> Result<Self, AudioDeviceError> {
        let _device = AudioDeviceEnumerator::get_device_by_id(device_id)?;
        let devices = AudioDeviceEnumerator::enumerate_devices()?;
        let device_info = devices
            .into_iter()
            .find(|d| d.id == device_id)
            .ok_or_else(|| AudioDeviceError::DeviceNotFound(device_id.to_string()))?;

        Ok(Self::with_device(device_info)?)
    }

    /// Create a capture manager with device info
    fn with_device(device_info: AudioDeviceInfo) -> Result<Self, AudioDeviceError> {
        let (tx, rx) = bounded(16);
        let shutdown = Arc::new(AtomicBool::new(false));

        Ok(AudioCaptureManager {
            device_info: Arc::new(Mutex::new(device_info)),
            tx,
            rx,
            shutdown,
            capture_thread: None,
        })  
    }
        
    /// Start capturing audio
    pub fn start_capture(&mut self) -> Result<(), AudioDeviceError> {
        let device_info = self.device_info.lock().unwrap().clone();
        let tx = self.tx.clone();
        let shutdown = Arc::clone(&self.shutdown);
        
        let handle = thread::spawn(move || {
            if let Err(e) = Self::capture_loop(&device_info, tx, &shutdown) {
                tracing::error!("[AudioCapture] Error: {}", e);
            }
        });

        self.capture_thread = Some(handle);
        Ok(())
    }

    /// The main capture loop
    fn capture_loop(
        device_info: &AudioDeviceInfo,
        tx: Sender<AudioPacket>,
        shutdown: &Arc<AtomicBool>,
    ) -> Result<(), AudioDeviceError> {
        
        // ============================================================================
        // STEP 1: GET THE AUDIO DEVICE
        // ============================================================================
        let _host = cpal::default_host();
        let device = AudioDeviceEnumerator::get_device_by_id(&device_info.id)?;


        // ============================================================================
        // STEP 2: GET THE DEVICE CONFIGURATION
        // ============================================================================
        
        // Ask the device: "What's your default configuration?"
        // This tells us sample rate, bit depth, channels, etc.
        // Why default? Because we're capturing system audio (not recording input)
        let config = device
            .default_output_config()
            .map_err(|_| AudioDeviceError::ConfigurationError(
                "Failed to get stream config".to_string(),
            ))?;
        

        // Extract useful info from the config
        let sample_rate = config.sample_rate().0;    // e.g., 48000 Hz
        let channels = config.channels();           // e.g., 2 (stereo)


        tracing::info!(
            "[AudioCapture] Starting capture: {} @ {} Hz, {} channels",
            device_info.id, sample_rate, channels
        );

        let stream_config = config.config();

        // ============================================================================
        // STEP 3: BUILD THE AUDIO STREAM
        // ============================================================================
        //
        // CPAL supports different audio formats (F32, I16, U16).
        // We need to handle each format differently because they're different data types.
        // This is a match statement - pick the right handler based on the sample format.
        //

       
        let stream = match config.sample_format() {

             // ========== CASE 1: F32 (32-bit floating point) ==========
            // This is the "native" format - samples are already in the -1.0 to +1.0 range
            cpal::SampleFormat::F32 => {
                device
                // Build an input stream with these parameters:
                // &stream_config    = device configuration (sample rate, channels, etc.)
                // callback function = what to do when audio data arrives
                // error handler     = what to do if something goes wrong
                // None              = no extra platform-specific options
                    .build_input_stream(
                        &stream_config,

                        // *** THE CALLBACK FUNCTION ***
                        // This runs every time the audio system has a buffer of samples ready.
                        // It happens hundreds of times per second!
                        // 
                        // Parameters:
                        //   data: &[f32]  = raw audio samples from the device
                        //   _info         = metadata (we ignore it with _)
                        move |data: &[f32], _| {
                            // Wrap the raw samples in our AudioPacket struct
                            let packet = AudioPacket {
                                samples: data.to_vec(),
                                sample_rate,
                                channels,
                                timestamp: Instant::now(),
                            };

                            if tx.try_send(packet).is_err() {
                                // The channel buffer is full - FFT thread can't keep up
                                // This is expected under heavy load, so we just drop this packet
                                // (The FFT thread will catch up eventually)

                            }
                        },
                        |err| tracing::error!("[AudioCapture] Stream Error: {}", err),
                        None,

                    )
                    .map_err(|e| AudioDeviceError::StreamCreationFailed(e.to_string()))?
            }

            // ========== CASE 2: I16 (16-bit signed integer) ==========
            // Samples are in the range -32768 to +32767
            // We need to convert to floating point (-1.0 to +1.0)
            cpal::SampleFormat::I16 => {
                device
                    .build_input_stream(
                        &stream_config,
                        move |data: &[i16], _| {
                            // Convert each i16 sample to f32 in the -1.0 to +1.0 range
                            // Division by 32768.0 is the magic number for i16 normalization
                            let float_samples: Vec<f32> = data
                                .iter()
                                .map(|&s| s as f32 / 32768.0)
                                .collect();

                            let packet = AudioPacket {
                                samples: float_samples,
                                sample_rate,
                                channels,
                                timestamp: Instant::now(),
                            };

                            if tx.try_send(packet).is_err() {
                                // The channel buffer is full - FFT thread can't keep up
                            }
                        },
                        |err| tracing::error!("[AudioCapture] Stream Error: {}", err),
                        None,
                    )
                    .map_err(|e| AudioDeviceError::StreamCreationFailed(e.to_string()))?
            }
            // ========== CASE 3: U16 (16-bit unsigned integer) ==========
            // Samples are in the range 0 to 65535 (signed at midpoint 32768)
            // We need to convert to floating point (-1.0 to +1.0)
            cpal::SampleFormat::U16 => {
                device
                    .build_input_stream(
                        &stream_config,
                        move |data: &[u16], _| {
                            // Convert each u16 sample to f32
                            // First divide by 32768 to get 0.0-2.0 range
                            // Then subtract 1.0 to get -1.0 to +1.0 range
                            let float_samples: Vec<f32> = data
                                .iter()
                                .map(|&s| (s as f32 / 32768.0) - 1.0)
                                .collect();

                            let packet = AudioPacket {
                                samples: float_samples,
                                sample_rate,
                                channels,
                                timestamp: Instant::now(),
                            };

                            if tx.try_send(packet).is_err() {
                                // The channel buffer is full - FFT thread can't keep up
                                
                            }
                        },
                        |err| tracing::error!("[AudioCapture] Stream Error: {}", err),
                        None,
                    )
                    .map_err(|e| AudioDeviceError::StreamCreationFailed(e.to_string()))?
            }
            _ => {
                return Err(AudioDeviceError::UnsupportedFormat);
            }
        };

        // ============================================================================
        // STEP 4: START THE STREAM
        // ============================================================================
        // 
        // At this point, the stream is created but NOT YET RUNNING.
        // We need to call .play() to actually start receiving audio data.
        //
        stream
            .play()
            .map_err(|e| AudioDeviceError::StreamCreationFailed(e.to_string()))?;

        tracing::info!("[AudioCapture] ✓ Audio stream started successfully");

        // ============================================================================
        // STEP 5: KEEP THE STREAM ALIVE
        // ============================================================================
        //
        // The stream is now running and the callback function is being called
        // hundreds of times per second.
        // 
        // This loop just keeps the thread alive and running until shutdown is signaled.
        // We check shutdown every 10ms - if true, we exit and clean up.
        //
        while !shutdown.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(10));
        }

        tracing::info!("[AudioCapture] Shutting down...");

        // ============================================================================
        // STEP 6: CLEANUP
        // ============================================================================
        //
        // When the loop exits (shutdown was signaled), we drop the stream.
        // Dropping the stream automatically stops it from running.
        drop(stream);

        Ok(())
    }

    /// get the packet receiver
    pub fn receiver(&self) -> Receiver<AudioPacket> {
        self.rx.clone()
    }

    /// get the current device info
    #[allow(dead_code)]
    pub fn device_info(&self) -> AudioDeviceInfo {
        self.device_info.lock().unwrap().clone()
    }

    /// Switch to a different audio device (can be called while capturing)
    pub fn switch_device(&mut self, device_id: &str) -> Result<(), AudioDeviceError> {
        let _device = AudioDeviceEnumerator::get_device_by_id(device_id)?;
        let devices = AudioDeviceEnumerator::enumerate_devices()?;
        let new_device_info = devices
            .into_iter()
            .find(|d| d.id == device_id)
            .ok_or_else(|| AudioDeviceError::DeviceNotFound(device_id.to_string()))?;

        // Stop the current capture
        self.stop_capture();

        // Update the device info
        *self.device_info.lock().unwrap() = new_device_info;

        // Restart capture with the new device
        self.start_capture()?;

        Ok(())
    }

    /// Stop capturing audio
    pub fn stop_capture(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);

        if let Some(handle) = self.capture_thread.take() {
            let _ = handle.join();
        }

        // Reset shutdown flag for potential restart
        self.shutdown.store(false, Ordering::Relaxed);
    }

    /// List all available devices
    pub fn list_devices() -> Result<Vec<AudioDeviceInfo>, AudioDeviceError> {
        AudioDeviceEnumerator::enumerate_devices()
    }
}

impl Drop for AudioCaptureManager {
    fn drop(&mut self) {
        self.stop_capture();
    }
}

// ========== Tests ============

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_packet_to_mono() {

        // stereo packet (L=1.0, R=0.5, L=0.5, R=0.25)
        let packet = AudioPacket {
            samples: vec![1.0, 0.5, 0.5, 0.25],
            sample_rate: 48000,
            channels: 2,
            timestamp: Instant::now(),
        };

        let mono = packet.to_mono();
        assert_eq!(mono.len(), 2);
        assert_eq!(mono[0], 0.75);
        assert_eq!(mono[1], 0.375);
    }

    #[test]
    fn test_audio_packet_mono_passthrough() {
        let samples = vec![0.1, 0.2, 0.3, 0.4]; 
        let packet = AudioPacket {
            samples: samples.clone(),
            sample_rate: 44100,
            channels: 1,
            timestamp: Instant::now(),
        };
    
        let mono = packet.to_mono();
        assert_eq!(mono, samples);
    }
    
    #[test]
    fn test_audio_packet_duration() {
        // 2 seconds of stereo audio at 48kHz
        let samples = vec![0.0; 48000 * 2 * 2]; // 48000 samples/sec * 2 sec * 2 channels

        let packet = AudioPacket {
            samples,
            sample_rate: 48000,
            channels: 2,
            timestamp: Instant::now(),
        };

        let duration = packet.duration_secs();
        assert!((duration - 2.0).abs() < 0.01);        
        
    }

    #[test]
    #[ignore]
    fn test_capture_manager_creation() {
        // This test will only work if audio devices are available
        match AudioCaptureManager::new() {
            Ok(manager) => {
                let device_info = manager.device_info();
                tracing::info!("Created capture manager for: {}", device_info);
                assert!(!device_info.name.is_empty());
            }
            Err(e) => {
                tracing::info!("Note: No audio device available for testing: {}", e);
            }
        }
    }

    #[test]
    #[ignore]
    fn test_list_devices() {
        match AudioCaptureManager::list_devices() {
            Ok(devices) => {
                tracing::info!("Found {} audio devices:", devices.len());
                for device in devices {
                    tracing::info!("{}", device);
                }
            }
            Err(e) => {
                tracing::info!("Error enumerating devices: {}", e);
            }
        }
    }
}
