use crossbeam_channel::Sender;
// Removed unused imports: egui::Response, Instant
use std::time::Duration; 
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
        let path_str = art_url.trim_start_matches("file://");
        let decoded_path = url_decode(path_str);
        let path = PathBuf::from(&decoded_path);

        if path.exists() {
            match fs::read(&path) {
                Ok(bytes) => return Some(bytes),
                Err(e) => tracing::warn!("[Media/Linux] Failed to read art file {:?}: {}", path, e),
            }
        }
    } 
    // 2. Handle HTTP/HTTPS (Common with Spotify/Browsers)
    else if art_url.starts_with("http://") || art_url.starts_with("https://") {
        let agent = ureq::AgentBuilder::new()
            .timeout_read(Duration::from_secs(3))
            .timeout_write(Duration::from_secs(3))
            .build();

        match agent.get(art_url).call() {
            Ok(response) => {
                let mut reader = response.into_reader();
                let mut bytes = Vec::new();
                if let Ok(_) = reader.read_to_end(&mut bytes) {
                    return Some(bytes);
                }
            },
            Err(e) => tracing::warn!("[Media/Linux] Failed to download art: {}", e),
        }
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
                    chars.next(); chars.next(); 
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
            if let Ok(player) = finder.find_active() { let _ = player.play_pause(); }
            else if let Ok(players) = finder.find_all() {
                if let Some(player) = players.into_iter().next() { let _ = player.play_pause(); }
            }
        }
    }

    fn try_next(&self) {
        if let Ok(finder) = PlayerFinder::new() {
            if let Ok(player) = finder.find_active() { let _ = player.next(); }
        }
    }

    fn try_prev(&self) {
        if let Ok(finder) = PlayerFinder::new() {
            if let Ok(player) = finder.find_active() { let _ = player.previous(); }
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
            
            // --- CACHE STATE ---
            let mut cached_art_url: Option<String> = None;
            let mut cached_art_bytes: Option<Vec<u8>> = None;

            tracing::info!("[Media/Linux] Monitor thread started");

            loop {
                // Find active player or fallback to first available
                let player_opt = finder.find_active().ok()
                    .or_else(|| finder.find_all().ok().and_then(|l| l.into_iter().next()));

                match player_opt {
                    Some(player) => {
                        let identity = player.identity().to_string();
                        let is_playing = player.get_playback_status().ok() == Some(PlaybackStatus::Playing);

                        if let Ok(meta) = player.get_metadata() {
                            let title = meta.title().unwrap_or("Unknown Title").to_string();
                            let artist = meta.artists().map(|a| a.join(", ")).unwrap_or("Unknown Artist".to_string());
                            let album = meta.album_name().unwrap_or_default().to_string();
                            
                            // --- LAZY ART LOADING ---
                            let current_url_opt = meta.art_url().map(|s| s.to_string());
                            
                            // If URL changed (or went from None to Some, or Some to None)
                            if current_url_opt != cached_art_url {
                                // Load new
                                if let Some(url) = &current_url_opt {
                                    cached_art_bytes = load_art_from_url(url);
                                } else {
                                    cached_art_bytes = None;
                                }
                                cached_art_url = current_url_opt;
                            }
                            
                            // Use cached directly (Fixes "unused assignment" warning)
                            let final_art = cached_art_bytes.clone();

                            let current_info = MediaTrackInfo {
                                title,
                                artist,
                                album,
                                is_playing,
                                source_app: identity,
                                album_art: final_art,
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
                        std::thread::sleep(Duration::from_millis(500));
                    }
                }
                
                std::thread::sleep(Duration::from_millis(1000));
            }
        });
    }
}