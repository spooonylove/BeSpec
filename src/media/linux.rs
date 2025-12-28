use crossbeam_channel::Sender;
use egui::Response;
use std::time::{Duration, Instant};
use std::fs;
use std::path::PathBuf;
use std::io::Read;
use super::{MediaController, MediaMonitor, MediaTrackInfo};
use mpris::{PlayerFinder, PlaybackStatus};

pub struct LinuxMediaManager;

impl LinuxMediaManager {
    pub fn new() -> Self { Self }
}

/// Helper function to load album art from a file:// URL
fn load_art_from_url(art_url: &str) -> Option<Vec<u8>> {
    // 1. Handle Local Files
    if art_url.starts_with("file://") {
        // Strip the schema
        let path_str = art_url.trim_start_matches("file://");

        // Basic URL decode
        let decoded_path = url_decode(path_str);
        let path = PathBuf::from(&decoded_path);

        if path.exists() {
            match fs::read(&path) {
                Ok(bytes) => {
                    // Changed to DEBUG (hidden by default)
                    tracing::debug!("[Media/Linux] Loaded art from file: {:?}", path);
                    return Some(bytes);
                },
                Err(e) => {
                    tracing::warn!("[Media/Linux] Failed to read art file {:?}: {}", path, e);
                }
            }
        } else {
            // Keep WARN: frequent "File not found" usually means our URL decoding is wrong
            tracing::warn!("[Media/Linux] File not found at path: {:?} (Original: '{}')", path, art_url);
        }
    } 
    // 2. Handle HTTP/HTTPS (Common with Spotify/Browsers)
    else if art_url.starts_with("http://") || art_url.starts_with("https://") {
        // Use a timeout so we don't hang the monitor thread if internet is slow
        let agent = ureq::AgentBuilder::new()
            .timeout_read(Duration::from_secs(3))
            .timeout_write(Duration::from_secs(3))
            .build();

        match agent.get(art_url).call() {
            Ok(response) => {
                let mut reader = response.into_reader();
                let mut bytes = Vec::new();
                if let Ok(_) = reader.read_to_end(&mut bytes) {
                    // Changed to DEBUG
                    tracing::debug!("[Media/Linux] Downloaded art from URL: '{}'", art_url);
                    return Some(bytes);
                } else {
                    tracing::warn!("[Media/Linux] Failed to read art data from URL response: '{}'", art_url);
                }
            },
            Err(e) => {
                tracing::warn!("[Media/Linux] Failed to download art from URL '{}': {}", art_url, e);
            }
        }
    }
    // 3. Unknown Scheme
    else {
         tracing::warn!("[Media/Linux] Unknown art URL scheme: '{}'", art_url);
    }

    None
}

/// Minimal URL decoder for file paths
fn url_decode(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.clone().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    output.push(byte as char);
                    chars.next(); 
                    chars.next(); 
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
                let _ = player.play_pause();
            } else if let Ok(players) = finder.find_all() {
                if let Some(player) = players.into_iter().next() {
                    let _ = player.play_pause();
                }
            }
        }
    }

    fn try_next(&self) {
        if let Ok(finder) = PlayerFinder::new() {
            if let Ok(player) = finder.find_active() {
                let _ = player.next();
            }
        }
    }

    fn try_prev(&self) {
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

            let mut last_sent_info: Option<MediaTrackInfo> = None;
            let mut last_debug_print = Instant::now(); 

            tracing::info!("[Media/Linux] Monitor thread started");

            loop {
                // Find active player or fallback to first available
                let player_opt = finder.find_active().ok()
                    .or_else(|| {
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
                            
                            // Load Art with new logging
                            let album_art = meta.art_url().and_then(load_art_from_url);

                            let current_info = MediaTrackInfo {
                                title,
                                artist,
                                album,
                                is_playing,
                                source_app: identity,
                                album_art,
                            };
                            
                            if last_sent_info.as_ref() != Some(&current_info) {
                                tracing::info!("[Media/Linux] Update: {} - {} (Art: {})", 
                                    current_info.artist, 
                                    current_info.title,
                                    if current_info.album_art.is_some() { "Yes" } else { "No" }
                                );
                                let _ = tx.send(current_info.clone());
                                last_sent_info = Some(current_info);
                            }
                        }
                    },
                    None => {
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