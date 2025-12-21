// Manually declar the module path so we don't have to restructure the whole project
// ...common trick for testing modules in isolation
#[path = "../media/mod.rs"]
mod media;

use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;
use crossbeam_channel::bounded;
use media::{PlatformMedia, MediaMonitor, MediaController, MediaTrackInfo};

fn print_track_info(info: &MediaTrackInfo) {
    println!("\n🎵 NOW PLAYING 🎵");
    println!("   App:    {}", info.source_app);
    println!("   Track:  {}", info.title);
    println!("   Artist: {}", info.artist);
    println!("   Album:  {}", info.album);

    // Check for album art
    match &info.album_art {
        Some(bytes) => println!("   Art:    [Image data Found: {} bytes]", bytes.len()),
        None => println!("   Art:    [No Image Data]"),
    }

    println!("   State:  {}", if info.is_playing { "▶ Playing" } else { "⏸ Paused" });
}

fn main() {
    println!("========================================");
    println!("   BeAnal Media Integration Test CLI    ");
    println!("========================================");
    println!("Commands:");
    println!("  [i] info         Show current track info");
    println!("  [p] play/pause   Toggle playback");
    println!("  [n] next         Skip to next track");
    println!("  [b] back         Skip to previous track");
    println!("  [q] quit         Exit");
    println!("----------------------------------------");

    // 1. Initialize the Platform Manager
    let manager = Arc::new(PlatformMedia::new());

    // 2. Setup Channel
    let (tx, rx) = bounded::<MediaTrackInfo>(10);

    // 3. Start Monitoring Thread
    println!("[*] Starting monitor thread...");

    manager.start(tx);

    
    // Give the background thread a tiny moment to fetch the first state
    // so it appears before the prompt.
    std::thread::sleep(Duration::from_millis(250));

    let mut last_known_info: Option<MediaTrackInfo> = None;

    // 4. Input Loop
    loop {
        // Non-blocking check for media updates
        // Drain the channel to get the latest info
        let mut new_update = false;
        while let Ok(info) = rx.try_recv() {
            last_known_info = Some(info.clone());
            print_track_info(&info);
            new_update = true;
        }

        if new_update {
            // If we just printed a big block of text, reprint the prompt
            print!("> ");
            io::stdout().flush().unwrap();
        }
        
        
        // --- Blocking Input Handling ---
        if !new_update {
            print!("> ");
            io::stdout().flush().unwrap();
        }

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_ok() {
            let cmd = input.trim();
            match cmd {
                "i" | "info" => {
                    if let Some(info) = &last_known_info {
                        print_track_info(info);
                    } else {
                        println!("[INFO] No track info available yet.");
                    }
                },
                "p" | "play" => {
                    println!("[CMD] Toggling Play/Pause");
                    manager.try_play_pause();
                },
                "n" | "next" => {
                    println!("[CMD] Skipping to Next Track");
                    manager.try_next();
                },
                "b" | "back" => {
                    println!("[CMD] Skipping to Previous Track");
                    manager.try_prev();
                },
                "q" | "quit" => {
                    println!("[CMD] Quitting");
                    break;
                },
                "" => {}, // Ignore empty enter
                _ => println!("Unknown command. Use i,p, n, b, or q."),
                
            }
        }
    }
}