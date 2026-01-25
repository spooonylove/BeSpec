use crossbeam_channel::Sender;
use windows::Storage::Streams::DataReader;
use std::time::Duration;
use super::{MediaController, MediaMonitor, MediaTrackInfo};

// We use the `windows-media` crate for media control and monitoring
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus;

use std::sync::OnceLock;
use tokio::runtime::Runtime;

#[derive(Clone)]
pub struct WindowsMediaManager;

static CONTROLLER_RUNTIME: OnceLock<Runtime> = OnceLock::new();

impl WindowsMediaManager {
    pub fn new() -> Self { Self}

    // Helper to get the current session using a throw-away Tokio runtime
    fn with_session<F>(callback: F)
    where F: FnOnce(&windows::Media::Control::GlobalSystemMediaTransportControlsSession)
    {
        // 1. Get or Init the runtime (lazy load)
        let rt = CONTROLLER_RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create Windows Media runtime")
        });

        // 2. Execute the async IPC call on the shared runtime
        rt.block_on(async {
            // We use RequestAsync() to get the manager.
            let manager_result = 
                GlobalSystemMediaTransportControlsSessionManager::RequestAsync();
            if let Ok(async_op) = manager_result {
                if let Ok(manager) = async_op.await {
                    if let Ok(session) = manager.GetCurrentSession(){
                        callback(&session);
                    }
                }
            }     

        });
    }
}

/// Helper functionto clean up Windows App Ids
fn clean_app_name(raw_id: &str) -> String {
    let stage1 = raw_id.split('!').last().unwrap_or(raw_id);
    let stage2 = stage1.split('.').next().unwrap_or(stage1);
    let mut chars = stage2.chars();
    match chars.next() {
        None => String::new(),
        Some(f) => format!("{}{}", f.to_uppercase(), chars.as_str().to_lowercase()),
    }
}

impl MediaController for WindowsMediaManager {
    fn try_play_pause(&self) {
        tracing::debug!("[Media/Windows] Toggling Play/Pause");
        Self::with_session(|s| { let _ = s.TryTogglePlayPauseAsync(); });
    }

    fn try_next(&self) {
        tracing::debug!("[Media/Windows] Skipping Next");
        Self::with_session(|s| { let _ = s.TrySkipNextAsync(); });
    }

    fn try_prev(&self) {
        tracing::debug!("[Media/Windows] Skipping Previous");
        Self::with_session(|s| { let _ = s.TrySkipPreviousAsync(); });
    }
}


impl MediaMonitor for WindowsMediaManager {
    fn start(&self, tx: Sender<MediaTrackInfo>) {
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

            // State tracking to prevent duplicate spam
            let mut last_sent_info: Option<MediaTrackInfo> = None;
            
            // --- LAZY LOADING STATE ---
            let mut cached_art: Option<Vec<u8>> = None;
            // We use (Title, Artist) as a composite key to detect track changes
            let mut cached_key: Option<(String, String)> = None;

            tracing::info!("[Media/Windows] Background monitor started");

            // 1. Acquire the System Manager ONCE. 
            //    RequestAsync is an expensive IPC call. We should not do this in a loop.
            let manager = rt.block_on(async {
                match GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
                    Ok(op) => op.await.ok(),
                    Err(e) => {
                        tracing::error!("[Media/Windows] Failed to request SessionManager: {}", e);
                        None
                    }
                }
            });

            // If we couldn't get the manager, we can't do anything. Abort.
            let manager = match manager {
                Some(m) => m,
                None => return, 
            };

            // Polling loop
            loop {
                // 2. Poll the existing manager for the current session.
                if let Ok(session) = manager.GetCurrentSession() {
                    
                    rt.block_on(async {
                        // Source App ID
                        let app_id_raw = session.SourceAppUserModelId().ok().map(|h| h.to_string()).unwrap_or_default();
                        let clean_app = clean_app_name(&app_id_raw);

                        // Playback Info
                        let is_playing = session.GetPlaybackInfo().ok()
                            .and_then(|i| i.PlaybackStatus().ok()) 
                            .map(|s| s == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
                            .unwrap_or(false);
                        
                        // Metadata
                        if let Ok(op) = session.TryGetMediaPropertiesAsync() {
                            if let Ok(props) = op.await {
                                let title = props.Title().ok().map(|h| h.to_string()).unwrap_or_default();
                                let artist = props.Artist().ok().map(|h| h.to_string()).unwrap_or_default();
                                let album = props.AlbumTitle().ok().map(|h| h.to_string()).unwrap_or_default();
                                
                                let current_key = (title.clone(), artist.clone());
                                let mut album_art_data = None;

                                // --- LAZY LOAD LOGIC ---
                                if Some(&current_key) == cached_key.as_ref() {
                                    // Same track? Reuse the bytes from memory.
                                    album_art_data = cached_art.clone();
                                } else {
                                    // New track? Fetch the thumbnail.
                                    // tracing::debug!("[Media/Windows] New track detected, fetching art...");
                                    
                                    if let Ok(thumb_ref) = props.Thumbnail() {
                                        if let Ok(stream_op) = thumb_ref.OpenReadAsync() {
                                            if let Ok(stream) = stream_op.await {
                                                let size = stream.Size().unwrap_or(0);
                                                if size > 0 {
                                                    if let Ok(reader) = DataReader::CreateDataReader(&stream) {
                                                        if let Ok(load_op) = reader.LoadAsync(size as u32) {
                                                            if load_op.await.is_ok() {
                                                                let mut bytes = vec![0u8; size as usize];
                                                                if reader.ReadBytes(&mut bytes).is_ok() {
                                                                    album_art_data = Some(bytes);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    // Update cache
                                    cached_key = Some(current_key);
                                    cached_art = album_art_data.clone();
                                }

                                if !title.is_empty() {
                                    let current_info = MediaTrackInfo {
                                        title,
                                        artist,
                                        album,
                                        is_playing,
                                        source_app: clean_app,
                                        album_art: album_art_data,
                                    };

                                    if last_sent_info.as_ref() != Some(&current_info) {
                                        tracing::info!("[Media/Windows] Update: {} - {} ({})", 
                                            current_info.artist, 
                                            current_info.title, 
                                            current_info.source_app
                                        );
                                        let _ = tx.send(current_info.clone());
                                        last_sent_info = Some(current_info);
                                    }
                                }
                            }
                        }
                    });
                }

                // Poll every second
                std::thread::sleep(Duration::from_millis(1000));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_app_name_standard_exe() {
        assert_eq!(clean_app_name("Spotify.exe"), "Spotify");
        assert_eq!(clean_app_name("chrome.exe"), "Chrome"); 
        assert_eq!(clean_app_name("firefox"), "Firefox");
    }

    #[test]
    fn test_clean_app_name_uwp() {
        let raw = "Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic";
        assert_eq!(clean_app_name(raw), "Microsoft"); 
    }

    #[test]
    fn test_clean_app_name_edge_cases() {
        assert_eq!(clean_app_name(""), "");
        assert_eq!(clean_app_name("My.Cool.App.exe"), "My");
        assert_eq!(clean_app_name("simple"), "Simple");
    }
}