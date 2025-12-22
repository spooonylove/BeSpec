use crossbeam_channel::Sender;
use std::time::{Duration, Instant};
use super::{MediaController, MediaMonitor, MediaTrackInfo};
use mediaremote_rs::{self, MediaRemote, RemoteCommand};

pub struct MacMediaManager;

impl MacMediaManager {
    pub fn new() -> Self { Self }
}

impl MediaController for MacMediaManager {
    fn try_play_pause(&self) {
        tracing::debug!("[Media/MacOS] Toggling Play/Pause");
        // Using the mediaremote crate to send commands
        if let Ok(client) = MediaRemote::new() {
            if let Err(e) = client.send_command(RemoteCommand::TogglePlayPause) {
                tracing::warn!("[Media/MacOS] Failed to send TogglePlayPause command: {:?}", e);
            }
        }
    }

    fn try_next(&self) {
        tracing::debug!("[Media/MacOS] Skipping Next");
        if let Ok(client) = MediaRemote::new() {
            if let Err(e) = client.send_command(RemoteCommand::NextTrack) {
                tracing::warn!("[Media/MacOS] Failed to send NextTrack command: {:?}", e);
            }
        }
    }

    fn try_prev(&self) {
        tracing::debug!("[Media/MacOS] Skipping Previous");
        if let Ok(client) = MediaRemote::new() {
            if let Err(e) = client.send_command(RemoteCommand::PreviousTrack) {
                tracing::warn!("[Media/MacOS] Failed to send PreviousTrack command: {:?}", e);
            }
        }
    }
}

impl MediaMonitor for MacMediaManager {
    fn start(&self, tx: Sender<MediaTrackInfo>) {
        std::thread::spawn(move || {
            // Attempt to connect to the MediaRemote framework
            let client = match MediaRemote::new() {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("[Media/MacOS] Failed to initialize MediaRemote: {:?}", e);
                    return;
                }
            };

            let mut last_sent_info: Option<MediaTrackInfo> = None;
            let mut last_debug_print = Instant::now();

            tracing::info!("[Media/MacOS] Monitor thread started");

            loop {
                // Fetch current info
                // Note: mediaremote-rs might return an error if nothing is playing
                //  or permission denied
                match client.get_now_playing_info() {
                    Ok(info) => {
                        let title = info.title.unwrap_or_default();
                        let artist = info.artist.unwrap_or_default();
                        let album = info.album.unwrap_or_default();

                        // App Bundle ID (eg "com.spotify.client")
                        let bundle_id = info.client_bundle_identifier.unwrap_or_default();
                        let source_app = clean_bundle_id(&bundle_id);

                        // Artwork comes directly as bytes!
                        let album_art = info.artwork_data;

                        // Playback state is often an enum, we simplify to bool
                        // there isn't a clear way to pull play/pause from MacOS MediaRemote
                        //     We pull playback speed. if its greater than 0.0 (paused), 
                        //     we consider it playing.
                        let is_playing = info.playback_speed.unwrap_or(0.0) > 0.0;

                        if !title.is_empty() {
                            let current_info = MediaTrackInfo {
                                title,
                                artist,
                                album,
                                is_playing,
                                source_app,
                                album_art,
                            };

                            if last_sent_info.as_ref() != Some(&current_info) {
                                tracing::info!("[Media/MacOS] Update: {} - {}", current_info.artist, current_info.title);
                                let _ = tx.send(current_info.clone());
                                last_sent_info = Some(current_info);
                            }
                        }
                    },
                    Err(_) => {
                        // Silent failre is common when nothing is playing
                        if last_debug_print.elapsed() > Duration::from_secs(60) {
                            tracing::debug!("[Media/MacOS] No active media info retrieved");
                            last_debug_print = Instant::now();
                        }
                    }
                }

                std::thread::sleep(Duration::from_millis(1000));
            }
        });
    }
}

/// Helper to make bundle IDs readable
/// com.spotify.client -> Spotify
/// com.apple.Music -> Apple Music
fn clean_bundle_id(bundle_id: &str) -> String {
    if bundle_id.is_empty() { return "Unknown".to_string(); }

    // 1. Specific Overrides (for names with spaces or unique casing)
    if bundle_id == "com.apple.Music" {
        return "Apple Music".to_string();
    }

    // 2. Generic Parser: Split by dot, read backwards, skip generic words
    bundle_id.split('.')
        .rev()
        .find(|&part| !matches!(part.to_lowercase().as_str(), "com" | "org" | "net" | "io" | "client" | "player" | "app" | "beta" | "stable"))
        .map(|part| {
            // Heuristic: if short (eg "vlc", "mpv"), assume acronmym -> "VLC"
            if part.len() <= 3 {
                return part.to_uppercase();
            }
            // Title Case: "spotift" -> "Spotify"
            let mut chars = parts.chars();
            match chars.next() {
                Some(f) => format!("{}{}", f.to_uppercase(), chars.as_str()),
                None => part.to_string(),
            }
        })
        .unwrap_or_else(|| bundle_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_bundle_id() {
        assert_eq!(clean_bundle_id("com.spotify.client"), "Spotify");
        assert_eq!(clean_bundle_id("com.apple.Music"), "Apple Music");
        assert_eq!(clean_bundle_id("org.videolan.vlc"), "VLC");
        assert_eq!(clean_bundle_id("com.custom.MyApp"), "MyApp");
    }
}