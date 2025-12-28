use crossbeam_channel::Sender;
use std::time::{Duration, Instant};
use std::process::Command;
use std::io::Read; // Needed for ureq response reading
use super::{MediaController, MediaMonitor, MediaTrackInfo};

// We need base64 decoding for Apple Music, and ureq for Spotify
use base64::{Engine as _, engine::general_purpose};

pub struct MacMediaManager;

impl MacMediaManager {
    pub fn new() -> Self { Self }
}

// === READ-ONLY IMPLEMENTATION ===
impl MediaController for MacMediaManager {
    fn try_play_pause(&self) {}
    fn try_next(&self) {}
    fn try_prev(&self) {}
}

impl MediaMonitor for MacMediaManager {
    fn start(&self, tx: Sender<MediaTrackInfo>) {
        std::thread::spawn(move || {
            let mut last_sent_info: Option<MediaTrackInfo> = None;
            tracing::info!("[Media/MacOS] Monitor started (Smart Mode: Raw + URL)");

            loop {
                if let Some(info) = get_macos_media_info() {
                    let current_info = MediaTrackInfo {
                        title: info.title.clone(),
                        artist: info.artist.clone(),
                        album: info.album.clone(),
                        is_playing: info.is_playing,
                        source_app: info.source_app.clone(),
                        album_art: info.album_art, 
                    };

                    // Send only on change
                    if last_sent_info.as_ref() != Some(&current_info) {
                        tracing::info!("[Media/MacOS] Update: {} - {} (Art: {})", 
                            current_info.artist, 
                            current_info.title,
                            if current_info.album_art.is_some() { "Yes" } else { "No" }
                        );
                        let _ = tx.send(current_info.clone());
                        last_sent_info = Some(current_info);
                    }
                }
                
                std::thread::sleep(Duration::from_secs(2));
            }
        });
    }
}

struct RawTrackInfo {
    title: String,
    artist: String,
    album: String,
    source_app: String,
    is_playing: bool,
    album_art: Option<Vec<u8>>,
}

// Updated JXA: Tries Raw Data first (Apple Music), then URL (Spotify)
const JXA_SCRIPT: &str = r#"
(function() {
    function toBase64(data) {
        if (!data) return null;
        try {
            var nsData = ObjC.unwrap(data);
            var base64Str = nsData.base64EncodedStringWithOptions(0);
            return ObjC.unwrap(base64Str);
        } catch (e) { return null; }
    }

    var appNames = ["Music", "Spotify", "YouTube Music"];
    var activeApp = null;
    
    for (var i = 0; i < appNames.length; i++) {
        try {
            if (Application(appNames[i]).running()) {
                activeApp = Application(appNames[i]);
                break;
            }
        } catch(e) {}
    }

    if (!activeApp) return "null";

    try {
        var state = activeApp.playerState();
        if (state === "stopped") return "null";
        
        var track = activeApp.currentTrack;
        var artBase64 = null;
        var artUrl = null;

        // 1. Try Raw Data (Standard for Apple Music)
        try {
            var artworks = track.artworks();
            if (artworks.length > 0) {
                var rawData = artworks[0].rawData(); 
                artBase64 = toBase64(rawData);
            }
        } catch (e) {}

        // 2. If no raw data, try Artwork URL (Standard for Spotify)
        if (!artBase64) {
            try {
                // Spotify often provides this property
                artUrl = track.artworkUrl();
            } catch (e) {}
        }

        return JSON.stringify({
            app: activeApp.name(),
            title: track.name(),
            artist: track.artist(),
            album: track.album(),
            playing: (state === "playing"),
            art_base64: artBase64,
            art_url: artUrl
        });
    } catch(e) {
        return "null";
    }
})();
"#;

fn get_macos_media_info() -> Option<RawTrackInfo> {
    let output = Command::new("osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg("-e")
        .arg(JXA_SCRIPT)
        .output()
        .ok()?;

    if !output.status.success() { return None; }

    let json_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if json_str == "null" || json_str.is_empty() { return None; }

    match serde_json::from_str::<serde_json::Value>(&json_str) {
        Ok(v) => {
            let mut final_art = None;

            // Strategy 1: Base64 Data (Apple Music)
            if let Some(b64) = v["art_base64"].as_str() {
                if !b64.is_empty() {
                    match general_purpose::STANDARD.decode(b64) {
                        Ok(bytes) => final_art = Some(bytes),
                        Err(_) => tracing::warn!("[Media/MacOS] Failed to decode Base64 art"),
                    }
                }
            }

            // Strategy 2: URL Download (Spotify)
            if final_art.is_none() {
                if let Some(url) = v["art_url"].as_str() {
                    if !url.is_empty() {
                        // tracing::debug!("[Media/MacOS] Fetching art from URL: {}", url);
                        final_art = download_art(url);
                    }
                }
            }

            Some(RawTrackInfo {
                source_app: v["app"].as_str().unwrap_or("Unknown").to_string(),
                title: v["title"].as_str().unwrap_or("").to_string(),
                artist: v["artist"].as_str().unwrap_or("").to_string(),
                album: v["album"].as_str().unwrap_or("").to_string(),
                is_playing: v["playing"].as_bool().unwrap_or(false),
                album_art: final_art,
            })
        },
        Err(e) => {
            tracing::warn!("[Media/MacOS] Failed to parse JXA output: {}", e);
            None
        }
    }
}

fn download_art(url: &str) -> Option<Vec<u8>> {
    let agent = ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(2))
        .timeout_write(Duration::from_secs(2))
        .build();

    match agent.get(url).call() {
        Ok(response) => {
            let mut reader = response.into_reader();
            let mut bytes = Vec::new();
            if reader.read_to_end(&mut bytes).is_ok() {
                return Some(bytes);
            }
        },
        Err(e) => tracing::warn!("[Media/MacOS] Art download failed: {}", e),
    }
    None
}