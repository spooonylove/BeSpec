use crossbeam_channel::Sender;
use std::time::Duration;
use super::{MediaController, MediaMonitor, MediaTrackInfo};

// We use the `windows-media` crate for media control and monitoring
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus;

#[derive(Clone)]
pub struct WindowsMediaManager;

impl WindowsMediaManager {
    pub fn new() -> Self { Self}

    // Helper to get the current session using a throw-away Tokio runtime
    fn with_session<F>(callback: F)
    where F: FnOnce(&windows::Media::Control::GlobalSystemMediaTransportControlsSession)
    {
        // Create a temporary runtime for the async Windows call
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let manager_result = GlobalSystemMediaTransportControlsSessionManager::RequestAsync();
            if let Ok(async_op) = manager_result {
                if let Ok(manager) = async_op.await {
                    if let Ok(session) = manager.GetCurrentSession() {
                        callback(&session);
                    }
                }
            }
        });
    }
}

/// Helper functionto clean up Windows App Ids
/// Extacts "Spotify" from  "Spotify.exe" or "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"
fn clean_app_name(raw_id: &str) -> String {
    // 1. Handle Package IDs (e.g. "Micorosoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic")
    // We take the part after the last '!' character
    let stage1 = raw_id.split('!').last().unwrap_or(raw_id);

    // 2. Handle Executables (e.g. "Spotiy.exe")
    // We take the part before the first '.' 
    let stage2 = stage1.split('.').next().unwrap_or(stage1);

    // 3. Option: Fix capitalization (e.g. "SPOTIFY" -> "Spotify")
    // for now, just return as is
    stage2.to_string()
}

impl MediaController for WindowsMediaManager {
    fn try_play_pause(&self) {
        Self::with_session(|s| { let _ = s.TryTogglePlayPauseAsync(); });
    }

    fn try_next(&self) {
        Self::with_session(|s| { let _ = s.TrySkipNextAsync(); });
    }

    fn try_prev(&self) {
        Self::with_session(|s| { let _ = s.TrySkipPreviousAsync(); });
    }
}


impl MediaMonitor for WindowsMediaManager {
    fn start(&self, tx: Sender<MediaTrackInfo>) {
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

            // State tracking to prevent duplicate spam
            let mut last_sent_info: Option<MediaTrackInfo> = None;

            // Polling loop (simple and robuts)
            loop {
                rt.block_on(async {
                    // 1. Get manager
                    let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync().ok() {
                        Some(op) => op.await.ok(),
                        None => None,
                    };
                    
                    if let Some(mgr) = manager {
                        if let Ok(session) = mgr.GetCurrentSession() {
                            // 2. Get Info
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

                                    if !title.is_empty() {
                                        let current_info = MediaTrackInfo {
                                            title,
                                            artist,
                                            album,
                                            is_playing,
                                            source_app: clean_app,
                                        };

                                        // Only send if the datda is different from last sent
                                        if last_sent_info.as_ref() != Some(&current_info) {
                                            let _ = tx.send(current_info.clone());
                                            last_sent_info = Some(current_info);
                                        }
                                    }
                                }
                            }
                        }
                    }
                });

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
    fn test_clean_app_name_exe() {
        assert_eq!(clean_app_name("Spotify.exe"), "Spotify");
        assert_eq!(clean_app_name("chrome.exe"), "chrome");
    }

    #[test]
    fn test_clean_app_name_uwp() {
        // UWP apps often have complex IDs
        let raw = "Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic";
        assert_eq!(clean_app_name(raw), "Microsoft"); 
        // Note: Logic splits at '.', so 'Microsoft.ZuneMusic' becomes 'Microsoft'.
        // You can tweak the helper function if you prefer 'Microsoft ZuneMusic'
    }

    #[test]
    fn test_clean_app_name_simple() {
        assert_eq!(clean_app_name("Firefox"), "Firefox");
    }
}
                                  

