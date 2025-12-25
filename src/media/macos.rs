use crossbeam_channel::Sender;
use std::time::{Duration, Instant};
use std::process::Command;
use super::{MediaController, MediaMonitor, MediaTrackInfo};

// We need base64 decoding to handle the image data from JXA
use base64::{Engine as _, engine::general_purpose};

pub struct MacMediaManager;

impl MacMediaManager {
    pub fn new() -> Self { Self }
}

// === READ-ONLY IMPLEMENTATION ===
// We stub out the control methods to be safe and simple.
impl MediaController for MacMediaManager {
    fn try_play_pause(&self) {
        tracing::debug!("[Media/MacOS] Control disabled: Play/Pause");
    }

    fn try_next(&self) {
        tracing::debug!("[Media/MacOS] Control disabled: Next");
    }

    fn try_prev(&self) {
        tracing::debug!("[Media/MacOS] Control disabled: Previous");
    }
}

impl MediaMonitor for MacMediaManager {
    fn start(&self, tx: Sender<MediaTrackInfo>) {
        std::thread::spawn(move || {
            let mut last_sent_info: Option<MediaTrackInfo> = None;
            tracing::info!("[Media/MacOS] Read-Only Monitor started");

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

                    if last_sent_info.as_ref() != Some(&current_info) {
                        tracing::info!("[Media/MacOS] Update: {} - {}", current_info.artist, current_info.title);
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

        try {
            var artworks = track.artworks();
            if (artworks.length > 0) {
                var rawData = artworks[0].rawData(); 
                artBase64 = toBase64(rawData);
            }
        } catch (e) {}

        return JSON.stringify({
            app: activeApp.name(),
            title: track.name(),
            artist: track.artist(),
            album: track.album(),
            playing: (state === "playing"),
            art: artBase64
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
            let album_art = v["art"].as_str().and_then(|b64| {
                general_purpose::STANDARD.decode(b64).ok()
            });

            Some(RawTrackInfo {
                source_app: v["app"].as_str().unwrap_or("Unknown").to_string(),
                title: v["title"].as_str().unwrap_or("").to_string(),
                artist: v["artist"].as_str().unwrap_or("").to_string(),
                album: v["album"].as_str().unwrap_or("").to_string(),
                is_playing: v["playing"].as_bool().unwrap_or(false),
                album_art,
            })
        },
        Err(_) => None
    }
}