use crossbeam_channel::Sender;
use std::time::{Duration, Instant};
use std::fs;
use std::path::PathBuf;
use super::{MediaController, MediaMonitor, MediaTrackInfo};
use mpris::{PlayerFinder, PlaybackStatus};

pub struct LinuxMediaManager;

impl LinuxMediaManager {
    pub fn new() -> Self { Self }
}

/// Helper function to load album art from a file:// URL
fn load_art_from_url(art_url: &str) -> Option<Vec<u8>> {
    // most linux playters return "file::///path/to/image.jpg"
    if art_url.starts_with("file://") {
        // Strip the schema
        let path_str = art_url.trim_start_matches("file://");

        // Basic URL decode (replace %20 with spaces, etc)
        // Since we don't want to add the 'url' crate just for this, we do a quick pass
        let decoded_path = url_decode(path_str);
        let path = PathBuf::from(decoded_path);

        if path.exists() {
            match fs::read(&path) {
                Ok(bytes) => {
                    tracing::debug!("[Media/Linux] Loaded art from {:?}", path);
                    return Some(bytes);
                },
                Err(e) => {
                    tracing::warn!("[Media/Linux] Failed to read art file {:?}: {}", path, e);
                }
            }
        } else {
            tracing::debug!("[Media/Linux] Art file does not exist: {:?}", path);
        }
    }
    None
}

/// Minimal URL decoder for file paths (handles spaces/special chars)
fn url_decode(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.clone().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    output.push(byte as char);
                    chars.next(); // skip 1
                    chars.next(); // skip 2
                    continue;
                }
            }
        }
        output.push(c);
    }
    output
}

impl MediaController for LinuxMediaManager {
    fn try_play_pause(&self) {
        tracing::debug!("[Media/Linux] Toggling Play/Pause");
        if let Ok(finder) = PlayerFinder::new() {
            // Try active first, then fallback to any
            if let Ok(player) = finder.find_active() {
                let _ = player.play_pause();
            } else if let Ok(players) = finder.find_all() {
                if let Some(player) = players.into_iter().next() {
                    let _ = player.play_pause();
                }
            }
        }
    }

    fn try_next(&self) {
        tracing::debug!("[Media/Linux] Skipping Next");
        if let Ok(finder) = PlayerFinder::new() {
            if let Ok(player) = finder.find_active() {
                let _ = player.next();
            }
        }
    }

    fn try_prev(&self) {
        tracing::debug!("[Media/Linux] Skipping Previous");
        if let Ok(finder) = PlayerFinder::new() {
            if let Ok(player) = finder.find_active() {
                let _ = player.previous();
            }
        }
    }
}

impl MediaMonitor for LinuxMediaManager {
    fn start(&self, tx: Sender<MediaTrackInfo>) {
        std::thread::spawn(move || {
            let finder = match PlayerFinder::new() {
                Ok(f) => f,
                Err(e) => {
                    tracing::error!("[Media/Linux] Failed to create PlayerFinder: {}", e);
                    return;
                }
            };

            // FIX: Declare state variables BEFORE the loop
            let mut last_sent_info: Option<MediaTrackInfo> = None;
            let mut last_debug_print = Instant::now(); // Used for debug throttling

            tracing::info!("[Media/Linux] Monitor thread started");

            loop {
                // STRATEGY: 
                // 1. Try to find the "Active" player (one that is strictly playing)
                // 2. If none, grab the first available player (e.g., paused Chrome/Spotify)
                let player_opt = finder.find_active().ok()
                    .or_else(|| {
                        // Fallback: Get list of all players and take the first one
                        match finder.find_all() {
                            Ok(list) => list.into_iter().next(),
                            Err(e) => {
                                if last_debug_print.elapsed() > Duration::from_secs(5) {
                                    tracing::debug!("[Media/Linux] find_all() failed: {}", e);
                                    last_debug_print = Instant::now();
                                }
                                None
                            }
                        }
                    });

                match player_opt {
                    Some(player) => {
                        let identity = player.identity().to_string();

                        let is_playing = player.get_playback_status().ok() == Some(PlaybackStatus::Playing);

                        if let Ok(meta) = player.get_metadata() {
                            let title = meta.title().unwrap_or("Unknown Title").to_string();
                            let artist = meta.artists().map(|a| a.join(", ")).unwrap_or("Unknown Artist".to_string());
                            let album = meta.album_name().unwrap_or_default().to_string();
                            
                            let album_art = meta.art_url().and_then(load_art_from_url);

                            let current_info = MediaTrackInfo {
                                title,
                                artist,
                                album,
                                is_playing,
                                source_app: identity,
                                album_art,
                            };
                            
                            // Send only on change
                            if last_sent_info.as_ref() != Some(&current_info) {
                                tracing::info!("[Media/Linux] Update: {} - {}", current_info.artist, current_info.title);
                                let _ = tx.send(current_info.clone());
                                last_sent_info = Some(current_info);
                            }
                        }
                    },
                    None => {
                        // Debug print every 5 seconds if no players are found
                        if last_debug_print.elapsed() > Duration::from_secs(5) {
                            tracing::trace!("[Media/Linux] No active players found");
                            last_debug_print = Instant::now();
                        }
                    }
                }
                
                std::thread::sleep(Duration::from_millis(1000));
            }
        });
    }
}

// === UNIT TESTS ===
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_decode_spaces() {
        assert_eq!(url_decode("Hello%20World"), "Hello World");
    }

    #[test]
    fn test_url_decode_symbols() {
        assert_eq!(url_decode("AC%2FDC%20Rocks"), "AC/DC Rocks");
        assert_eq!(url_decode("Fish%20%26%20Chips"), "Fish & Chips");
    }

    #[test]
    fn test_url_decode_paths() {
        assert_eq!(url_decode("home/user/Music/My%20Song.mp3"), "home/user/Music/My Song.mp3");
    }

    #[test]
    fn test_url_decode_incomplete_escape() {
        // Should handle cases where % is not followed by 2 hex digits
        assert_eq!(url_decode("100%"), "100%");
        assert_eq!(url_decode("Scale%2"), "Scale%2");
    }
}