use crossbeam_channel::Sender;
use std::time::{Duration, Instant};
use std::process::Command;
use std::io::Read; 
use super::{MediaController, MediaMonitor, MediaTrackInfo};

use base64::{Engine as _, engine::general_purpose};

pub struct MacMediaManager;

impl MacMediaManager {
    pub fn new() -> Self { Self }
}

impl MediaController for MacMediaManager {
    fn try_play_pause(&self) {}
    fn try_next(&self) {}
    fn try_prev(&self) {}
}

impl MediaMonitor for MacMediaManager {
    fn start(&self, tx: Sender<MediaTrackInfo>) {
        std::thread::spawn(move || {
            let mut last_sent_info: Option<MediaTrackInfo> = None;
            
            // --- CACHE STATE ---
            let mut cached_art: Option<Vec<u8>> = None;
            let mut cached_key: (String, String) = (String::new(), String::new());

            tracing::info!("[Media/MacOS] Monitor started (Smart Mode: Priority Scan)");

            loop {
                // Pass current known track info to optimized parser
                if let Some(mut info) = get_macos_media_info(&cached_key.0, &cached_key.1) {
                    
                    let current_key = (info.title.clone(), info.artist.clone());

                    // Cache Logic
                    if info.album_art.is_some() {
                        cached_art = info.album_art.clone();
                        cached_key = current_key;
                    } else if current_key == cached_key {
                        info.album_art = cached_art.clone();
                    } else {
                         cached_art = None;
                         cached_key = current_key;
                    }

                    // UNCOMMENT TO VERIFY CACHE HITS
                    // if info.album_art.is_some() && current_key == cached_key {
                    //      tracing::trace!("[Media] Cache Hit: Skipping art fetch");
                    // }

                    let current_info = MediaTrackInfo {
                        title: info.title,
                        artist: info.artist,
                        album: info.album,
                        is_playing: info.is_playing,
                        source_app: info.source_app,
                        album_art: info.album_art, 
                    };

                    if last_sent_info.as_ref() != Some(&current_info) {
                        tracing::info!("[Media/MacOS] Update: {} - {} (App: {})", 
                            current_info.artist, 
                            current_info.title,
                            current_info.source_app
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

// === UPDATED JXA SCRIPT ===
// Now iterates ALL apps to find the one that is PLAYING.
// Priority: Playing > Paused > Stopped (Ignore)
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
    var bestApp = null;
    var bestState = "stopped"; // 0 priority

    // 1. Find the best candidate
    for (var i = 0; i < appNames.length; i++) {
        try {
            var app = Application(appNames[i]);
            if (app.running()) {
                var state = app.playerState(); // playing, paused, stopped
                
                if (state === "playing") {
                    bestApp = app;
                    bestState = "playing";
                    break; // We found a winner, stop looking
                }
                
                // Keep looking, but remember this one if we don't find a playing one
                if (state === "paused" && bestState === "stopped") {
                    bestApp = app;
                    bestState = "paused";
                }
            }
        } catch(e) {}
    }

    if (!bestApp || bestState === "stopped") return "null";

    // 2. Extract Data from Best App
    try {
        var track = bestApp.currentTrack;
        var artBase64 = null;
        var artUrl = null;

        try {
            var artworks = track.artworks();
            if (artworks.length > 0) {
                var rawData = artworks[0].rawData(); 
                artBase64 = toBase64(rawData);
            }
        } catch (e) {}

        if (!artBase64) {
            try { artUrl = track.artworkUrl(); } catch (e) {}
        }

        return JSON.stringify({
            app: bestApp.name(),
            title: track.name(),
            artist: track.artist(),
            album: track.album(),
            playing: (bestState === "playing"),
            art_base64: artBase64,
            art_url: artUrl
        });
    } catch(e) {
        return "null";
    }
})();
"#;

fn get_macos_media_info(known_title: &str, known_artist: &str) -> Option<RawTrackInfo> {
    let output = Command::new("osascript")
        .arg("-l").arg("JavaScript").arg("-e").arg(JXA_SCRIPT)
        .output()
        .ok()?;

    if !output.status.success() { return None; }

    let json_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if json_str == "null" || json_str.is_empty() { return None; }

    match serde_json::from_str::<serde_json::Value>(&json_str) {
        Ok(v) => {
            let title = v["title"].as_str().unwrap_or("").to_string();
            let artist = v["artist"].as_str().unwrap_or("").to_string();
            
            // Check for cache hit
            let is_same_track = title == known_title && artist == known_artist;
            if is_same_track {
                 return Some(RawTrackInfo {
                    source_app: v["app"].as_str().unwrap_or("Unknown").to_string(),
                    title,
                    artist,
                    album: v["album"].as_str().unwrap_or("").to_string(),
                    is_playing: v["playing"].as_bool().unwrap_or(false),
                    album_art: None, // Signal cache
                });
            }

            let mut final_art = None;

            if let Some(b64) = v["art_base64"].as_str() {
                if !b64.is_empty() {
                    match general_purpose::STANDARD.decode(b64) {
                        Ok(bytes) => final_art = Some(bytes),
                        Err(_) => tracing::warn!("[Media/MacOS] Failed to decode Base64 art"),
                    }
                }
            }

            if final_art.is_none() {
                if let Some(url) = v["art_url"].as_str() {
                    if !url.is_empty() {
                        final_art = download_art(url);
                    }
                }
            }

            Some(RawTrackInfo {
                source_app: v["app"].as_str().unwrap_or("Unknown").to_string(),
                title,
                artist,
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