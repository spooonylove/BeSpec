use crossbeam_channel::Sender;

// Module datastructre is self-contained for media handling
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaTrackInfo {
    pub title: String,
    pub artist: String,
    pub album: String, 
    pub is_playing: bool,
    pub source_app: String,
    pub album_art: Option<Vec,u8>>,
}

/// Trait for controlling media playback (Commands)
pub trait MediaController: Send + Sync {
    fn try_play_pause(&self);
    fn try_next(&self);
    fn try_prev(&self);
}

/// Trait for monitoring media state (Events)
pub trait MediaMonitor {
    /// Starts the background listener thread
    /// Updates are sent via the provided channel.
    fn start(&self, tx: Sender<MediaTrackInfo>);
}

// ==============================================================
// OS SELECTION FACTORY
// ==============================================================

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub type PlatformMedia = windows::WindowsMediaManager;

/*
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub type PlatformMedia = linux::LinuxMediaManager;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub type PlatformMedia = macos::MacOSMediaManager;

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod dummy;
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub type PlatformMedia = dummy::DummyMediaManager;
*/

// Fallback for unsupported OS (Currently catches Linux/Mac too)
#[cfg(not(any(target_os = "windows")))] // Removed linux/macos from this check so they fall here
mod dummy;
#[cfg(not(any(target_os = "windows")))]
pub type PlatformMedia = dummy::DummyMediaManager;