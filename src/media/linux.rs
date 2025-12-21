use crossbeam_channel::Sender;
use std::time::Duration;
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
        let path_str = art_url.time_start_matches("file://");

        // Basic URL decode (replace %20 with spaces, etc)
        // Since we don't want to add the 'url' crate just for this, we do a quick pass
        let decoded_path = url_decode(path_str);
        let path = PathBuf::from(decoded_path);

        if path.exists() {
            return fs::read(path).ok();
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
        if let Ok(finder) = PlayerFinder::new() {
            if let Ok(player) = finder.find_active() {
                let _ = player.try_play_pause();
            }
        }
    }

    fn try_next(&self) {
        if let Ok(finder) = PlayerFinder::new() {
            if let Ok(player) = finder.find_active() {
                let _ = player.try_next();
            }
        }
    }

    fn try_prev(&self) {
        if let Ok(finder) = PlayerFinder::new() {
            if let Ok(player) = finder.find_active() {
                let _ = player.try_prev();
            }
        }
    }
}

impl MediaMonitor for LinuxMediaManager {
    fn start(&self, tx: Sender<MediaTrackInfo>) {
        std::thread::spawn(move || {
            let finder = match PlayerFinder::new() {
                Ok(f) => f,
                Err(_) => {
                    println!("Linux Media Monitor: Failed to create PlayerFinder.");
                    return;
                }
            };

            let mut last_seen_info: Option<MediaTrackInfo> = None;

            loop {
                // Find active player (eg. Spotify, V:C, Chrome)
                if let Ok(player) = finder.find_active() {
                    // Get playback status
                    let identity = player.identity().to_string();
                    
                    let is_playing = player.get_playback_status().ok() == Some(PlaybackStatus::Playing);

                    // Get metadata
                    if let Ok(meta) = player.get_metadata() {
                       let title = meta.title().unwrap_or("Unknown").to_string();
                       let artist = meta.artists().map(|a| a.join(", ")).unwrap_or("Unknown Artist".to_string());

                       let album = meta.album_name().unwrap_or_default().to_string();

                       // Attempt to load Album Art
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
                       if last_seen_info.as_ref() != Some(&current_info) {
                           let _ = tx.send(current_info.clone());
                           last_seen_info = Some(current_info);
                       }
                    }
                }

                // Polling interval
                std::thread::sleep(Duration::from_millis(1000));
            }
        });
    }
}