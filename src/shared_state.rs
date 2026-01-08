use std::time::{Duration, Instant};
use crate::fft_config::FFTInfo;
use serde::{Serialize, Deserialize};
use std::fs;
use std::path::PathBuf;
use directories::ProjectDirs;

pub const SILENCE_DB: f32 = -140.0;

/// Main Shared state container -- wrapped in Arc<Mutx<>> for thread safety
/// 
///  This struct is shared between:
///  - FFT thread (writes visualization data, reads config)
///  - GUI thread (reads visualization data, writes config)

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum VisualMode {
    SolidBars,
    SegmentedBars,
    LineSpectrum,
    Oscilloscope,
}


#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum MediaDisplayMode {
    FadeOnUpdate,   // Fade for N seconds the fade
    AlwaysOn,       // Always visible
    Off,            // Hidden
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum ThemeFont{
    Standard,       // Clean, sans-serif
    Monospace,      // Retro / Code-style 
}

// =====================================================================================
// Color Profile Architecture
// =====================================================================================

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct ColorProfile {
    pub name: String, 

    // Visualization Colors
    pub low: Color32,
    pub high: Color32, 
    pub peak: Color32,
    
    // Window Enviornment
    pub background: Color32,

    // Text Color for Overlays
    pub text: Color32,

    // Inspector Colors
    pub inspector_bg: Color32,
    pub inspector_fg: Color32,
}

impl Default for ColorProfile {
    fn default() -> Self {
        Self {
            name: "Winamp".to_string(),
            low: Color32::from_rgb(50, 205, 50),    // LimeGreen
            high: Color32::from_rgb(255, 255, 0),   // Yellow
            peak: Color32::from_rgb(255, 0, 0),     // Red
            background: Color32::from_rgb(0, 0, 0),  // Black
            text: Color32::from_rgb(255, 255, 255),  // White
            
            inspector_bg: Color32::from_rgb(0, 0, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 255, 255), // White
        }
    }
}

impl ColorProfile {
    /// Returns a list of built-in color presets
    pub fn built_in() -> Vec<Self> {
        // Profiles are defined in presets.rs
        crate::presets::built_in_colors()
    }

    /// Try to find a built-in prfile by name
    pub fn find_by_name(name: &str) -> Option<Self> {
        Self::built_in().into_iter().find(|p| p.name == name)
    }
}

/// A Link to a color profile
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum ColorRef {
    Preset(String),         // Name of built-in preset
    Custom(ColorProfile),   // User-defined custom profile
}

// =====================================================================================
// Visual Profile (Windowing, Bars, and Visualization Colors)
// =====================================================================================

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct VisualProfile {
    pub name: String,

    // === Visual Structure ===
    pub visual_mode: VisualMode,
    pub num_bars: usize,
    pub bar_gap_px: u32,
    pub bar_opacity: f32,
    pub segment_height_px: f32,
    pub segment_gap_px: f32,
    pub inverted_spectrum: bool,
    pub fill_peaks: bool,
    pub show_peaks: bool,

    // Font Selection
    pub overlay_font: ThemeFont,

    // === Dynamics ===
    pub sensitivity: f32,
    pub attack_time_ms: f32,
    pub release_time_ms: f32,
    pub peak_hold_time_ms: f32,
    pub peak_release_time_ms: f32,
    pub use_peak_aggregation: bool,

    // === Color Link ===
    pub color_link: ColorRef,

    // MAY REMOVE THIS LATER?
    pub background: Option<Color32>,
}

impl Default for VisualProfile {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            visual_mode: VisualMode::SolidBars,
            num_bars: 150,
            bar_gap_px: 2,
            bar_opacity: 1.0,
            segment_height_px: 4.0,
            segment_gap_px: 2.0,
            inverted_spectrum: false,
            fill_peaks: false,
            show_peaks: true,
            overlay_font: ThemeFont::Standard,

            sensitivity: 1.0,
            attack_time_ms: 20.0,
            release_time_ms: 200.0,
            peak_hold_time_ms: 1000.0,
            peak_release_time_ms: 1500.0,
            use_peak_aggregation: true,

            color_link: ColorRef::Preset("Default".to_string()),

            background: None,
        }
    }
}

impl VisualProfile {
    /// Built-in Visual Profiles
    pub fn built_in() -> Vec<Self> {
        crate::presets::built_in_visuals()
    }
}

// ====================================================================================
// Main State & Config 
// ====================================================================================

// ==== Main Shared State ====
pub struct SharedState{
    /// Current visualization datda (bars, peaks)
    pub visualization: VisualizationData,

    /// Performance metrics (FFT timing, frame counts)
    pub performance : PerformanceStats,

    /// Application configuration (user settings)
    pub config: AppConfig,

    // === Audio Device State === 
    pub audio_devices: Vec<String>,

    /// Flag: GUI requested a device switch (handled by main thread)
    pub device_changed: bool,

    /// Flag: GUI requests a hardware scan (handled by main thread
    pub refresh_devices_requested: bool,

    // === Media Player State ===
    /// Curreently playing track info
    pub media_info: Option<crate::media::MediaTrackInfo>,
    /// When the track info was last updated
    pub last_media_update: Option<Instant>,

    // === User Presets ===
    /// Loaded from JSON file at startup
    pub user_color_presets: Vec<ColorProfile>,
    pub user_visual_presets: Vec<VisualProfile>,

}

impl SharedState {
    pub fn new() -> Self {
        let config = AppConfig::load();

        let user_color_presets = AppConfig::load_user_color_presets();
        let user_visual_presets = AppConfig::load_user_visual_presets();
        tracing::info!("[State] Loaded {} user color presets", user_color_presets.len());
        tracing::info!("[State] Loaded {} user visual presets", user_visual_presets.len());

        Self {
            visualization: VisualizationData::new(config.profile.num_bars),
            performance: PerformanceStats::default(),
            config,
            audio_devices: Vec::new(),
            device_changed: false,
            refresh_devices_requested: false,
            media_info: None,
            last_media_update: None,
            user_color_presets,
            user_visual_presets,
        }
    }
}
// === Data Structures ====

#[derive(Clone)]
pub struct VisualizationData {
    /// Bar heights in dB ( typically -80 to +40 range)
    pub bars: Vec<f32>,

    /// Peak indicator heights in dB
    pub peaks: Vec<f32>,

    /// Raw Audio wavefor for oscilloscope mode 
    // We keep a small buffer for drawing
    pub waveform: Vec<f32>,

    /// When this data was last updated
    pub timestamp: Instant,
}

impl VisualizationData {
    pub fn new(num_bars: usize) -> Self {
        Self {
            bars: vec![SILENCE_DB; num_bars],
            peaks: vec![SILENCE_DB; num_bars],
            waveform: vec![0.0; 2048],
            timestamp: Instant::now(),
        }
    }
}

/// Performance statistics (updated by both threads, yo)
#[derive(Clone, Default)]
pub struct PerformanceStats {
    pub frame_count: u64,
    pub fft_ave_time: Duration,
    pub fft_min_time: Duration,
    pub fft_max_time: Duration,
    pub gui_fps: f32,
    pub fft_info: FFTInfo,
}


// ==== Configuration ====

#[derive(Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub profile: VisualProfile,

    // === System Settings ===
    
    /// Saved Window dimensions [width, height]
    pub window_size:  [f32; 2],

    /// Saved Window position on screen [x, y]
    pub window_position: Option<[f32; 2]>,
   
    pub always_on_top: bool,

    ///  "Ghost Mode": Window is click-through until focused with alt-tab
    pub window_locked: bool,

    /// Shop window title bar and borders
    pub window_decorations: bool,

    pub show_stats: bool,

    pub inspector_enabled: bool,    

    /// Name of selected input device (default: "Default")
    pub selected_device: String,

    /// The lowest dB value to display (the "floor")
    pub noise_floor_db: f32,

    // === Media Settings ===
    pub media_display_mode: MediaDisplayMode,
    pub media_fade_duration_sec: f32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            profile: VisualProfile::default(),
            window_size: [800.0, 400.0],
            window_position: None,
            always_on_top: false,
            window_locked: false,
            window_decorations: false,
            inspector_enabled: true,
            show_stats: false,
            selected_device: "Default".to_string(),
            noise_floor_db: -60.0,
            media_display_mode: MediaDisplayMode::FadeOnUpdate,
            media_fade_duration_sec: 5.0,
        }
    }
}

impl AppConfig {
    /// Returns the standard OS config path, e.g.:
    /// Windows: C:\Users\Username\AppData\Roaming\BeSpec
    /// MacOS: /Users/Username/Library/Application Support/BeSpec
    /// Linux: /home/username/.config/BeSpec
    fn get_config_path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("","","BeSpec") {
            let config_dir = proj_dirs.config_dir();

            // Ensure directory exists
            if let Err(e) = fs::create_dir_all(config_dir) {
                tracing::error!("[Config] Error creating config directory: {}", e);
            }

            return config_dir.join("config.json");
        }
        
        // Fallback
        PathBuf::from("BeSpec_config.json")
    }

    pub fn load() -> Self {
        let path = Self::get_config_path();

        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(contents) => match serde_json::from_str(&contents) {
                    Ok(config) => {
                        tracing::info!("[Config] Loading config from {:?}", path);
                        return config;
                    },
                    Err(e) => tracing::error!("[Config] Parse eror: {}", e),
                },
                Err(e) => tracing::error!("[Config] Read error: {}", e),
            }
        }
        tracing::info!("[Config] Using defaults (new config will be saved to {:?})", path);
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::get_config_path();
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = fs::write(&path, json) {
                    eprint!("[Config] Failed to save to {:?}: {}", path, e);
                } else {
                    tracing::info!("[Config] Saved config to {:?}", path);
                }
            },
            Err(e) => tracing::error!("[Config] Failed to serialize config: {}", e),
        }
    }

       
    pub fn resolve_colors(&self, user_presets: &[ColorProfile]) -> ColorProfile {
        match &self.profile.color_link {
            ColorRef::Custom(colors) => {
                let mut c = colors.clone();
                //Apply optional background override
                if let Some(bg) = self.profile.background {c.background = bg; }
                c
            },
            ColorRef::Preset(name) => {
                // 1. Try finding User presets first
                if let Some(p)  = user_presets.iter().find(|p| &p.name == name) {
                    let mut c = p.clone();
                    if let Some (bg) = self.profile.background {c.background = bg; }
                    return c;
                }

                // 2. Fallback into built-in presets
                let mut c = ColorProfile::find_by_name(name).unwrap_or_default();
                if let Some (bg) = self.profile.background {c.background = bg; }
                c
            }   
        }
    }

    pub fn load_user_color_presets() -> Vec<ColorProfile> {
        let mut profiles = Vec::new();

        // Path: ../BeSpec/presets/colors/
        if let Some(proj_dirs) = ProjectDirs::from("","","BeSpec") {
            let preset_dir = proj_dirs.data_dir().join("presets").join("colors");

            if preset_dir.exists() {
                if let Ok(entries) = fs::read_dir(&preset_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().map_or(false, |ext| ext == "json") {
                            if let Ok(content) = fs::read_to_string(&path) {
                                match serde_json::from_str::<ColorProfile>(&content) {
                                    Ok(profile) => {
                                        if profiles.iter().any(|p: &ColorProfile| p.name == profile.name) {
                                            tracing::warn!("[Presets] Duplicate color profile name '{}' in file {:?}. Skipping.", profile.name, path);
                                        } else {
                                            tracing::info!("[Config] Loaded user color preset: {}", profile.name);
                                            profiles.push(profile);
                                        }
                                    },
                                    Err(e) => {
                                        tracing::error!("[Config] Failed to parse color preset {:?}: {}", path, e);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        profiles
    }

    pub fn save_user_color_preset(profile: &ColorProfile) -> std::io::Result<()> {
        if let Some(proj_dirs) = ProjectDirs::from("","","BeSpec") {
            let preset_dir = proj_dirs.data_dir().join("presets").join("colors");
            fs::create_dir_all(&preset_dir)?;

            let filename = format!("{}.json",Self::sanitize_filename(&profile.name));
            
            let json = serde_json::to_string_pretty(profile)?;
            fs::write(preset_dir.join(filename), json)?;
        }
        Ok(())
    }

    pub fn delete_user_color_preset(name: &str) -> std::io::Result<()> {
        if let Some(proj_dirs) = ProjectDirs::from("","","BeSpec") {
            let preset_dir = proj_dirs.data_dir().join("presets").join("colors");
            let filename = format!("{}.json", Self::sanitize_filename(name));
            let path = preset_dir.join(filename);
            if path.exists() {
                fs::remove_file(path)?;
                tracing::info!("[Presets] Deleted color preset: {}", name);
            }
        }
        Ok(())
    }

    pub fn load_user_visual_presets() -> Vec<VisualProfile> {
        let mut profiles = Vec::new();
        if let Some(proj_dirs) = ProjectDirs::from("","","BeSpec") {
            let preset_dir = proj_dirs.data_dir().join("presets").join("visuals");
            if preset_dir.exists() {
                if let Ok(entries) = fs::read_dir(preset_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().map_or(false, |ext| ext == "json") {
                            if let Ok(content) = fs::read_to_string(&path) {
                                match serde_json::from_str::<VisualProfile>(&content) {
                                    Ok(profile) => {
                                        // FIX: Check for duplicates before adding
                                        if profiles.iter().any(|p: &VisualProfile| p.name == profile.name) {
                                            tracing::warn!("[Presets] Duplicate visual profile name '{}' in file {:?}. Skipping.", profile.name, path);
                                        } else {
                                            profiles.push(profile);
                                        }
                                    },
                                    Err(e) => tracing::warn!("[Presets] Failed to parse {:?}: {}", path, e),
                                }
                            }
                        }
                    }
                }
            }
        }
        profiles
    }

    pub fn save_user_visual_preset(profile: &VisualProfile) -> std::io::Result<()> {
        if let Some(proj_dirs) = ProjectDirs::from("","","BeSpec") {
            let preset_dir = proj_dirs.data_dir().join("presets").join("visuals");
            fs::create_dir_all(&preset_dir)?;

            let filename = format!("{}.json",Self::sanitize_filename(&profile.name));
            
            let json = serde_json::to_string_pretty(profile)?;
            fs::write(preset_dir.join(filename), json)?;
        }
        Ok(())
    }

    pub fn delete_user_visual_preset(name: &str) -> std::io::Result<()> {
        if let Some(proj_dirs) = ProjectDirs::from("","","BeSpec") {
            let preset_dir = proj_dirs.data_dir().join("presets").join("visuals");
            let filename = format!("{}.json", Self::sanitize_filename(name));
            let path = preset_dir.join(filename);
            if path.exists() {
                fs::remove_file(path)?;
                tracing::info!("[Presets] Deleted visual preset: {}", name);
            }
        }
        Ok(())
    }

    // Helper: Sanitize Filename to avoid duplicates / illegal chars
    fn sanitize_filename(name: &str) -> String {
        name.trim()
            .replace(" ", "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "")
            .to_lowercase()
    } 
}




/// Simple RGBA Color (compatible with egui)
/// 
/// We define our own to avoid depending on egui in SharedState
/// (can convert to egui::Color32 in GUI Code)
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Color32{
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color32 {

    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self {r, g, b, a: 255}
    }

    #[allow(dead_code)]
    pub const WHITE: Self = Self::from_rgb(255, 255, 255);
    #[allow(dead_code)]
    pub const BLACK: Self = Self::from_rgb(0, 0, 0);
    #[allow(dead_code)]
    pub const RED: Self = Self::from_rgb(255, 0, 0);

    /// Multiply color by opacity (for transparency)
    #[allow(dead_code)]
    pub fn with_opacity(self, opacity: f32) -> Self {
        Self {
            r: self.r,
            g: self.g,
            b: self.b,
            a: (self.a as f32 * opacity.clamp(0.0, 1.0)) as u8,
        }
    }
}

// === Tests ====
#[cfg(test)]
mod tests {
    use super::*;

    // --- 1. Serialization Tests ---
    // Critical: Ensures your structs don't crash serde when saving/loading
    #[test]
    fn test_visual_profile_serialization() {
        let original = VisualProfile {
            name: "Test Profile".to_string(),
            visual_mode: VisualMode::LineSpectrum,
            num_bars: 128,
            // ... explicit non-default values to ensure they persist
            sensitivity: 2.5,
            ..Default::default()
        };

        let serialized = serde_json::to_string(&original).expect("Failed to serialize");
        let deserialized: VisualProfile = serde_json::from_str(&serialized).expect("Failed to deserialize");

        assert_eq!(original, deserialized);
        assert_eq!(deserialized.sensitivity, 2.5);
    }

    // --- 2. Logic Tests (Color Resolution) ---
    // Critical: Ensures the "cascading" logic of presets works
    #[test]
    fn test_resolve_colors_priority() {
        
        // A. Setup a user preset that CLASHES with a built-in name
        let mut user_presets = Vec::new();
        let user_neon = ColorProfile {
            name: "Neon Tokyo".to_string(), // Same name as built-in
            low: Color32::RED, // User version is RED
            ..ColorProfile::default()
        };
        user_presets.push(user_neon);

        // B. Request that name
        let mut profile = VisualProfile::default();
        profile.color_link = ColorRef::Preset("Neon Tokyo".to_string());
        
        // C. Mock the resolve call (we construct a config just for this test)
        let mut test_config = AppConfig::default();
        test_config.profile = profile;

        // D. Verify USER preset overrides BUILT-IN
        let resolved = test_config.resolve_colors(&user_presets);
        assert_eq!(resolved.low, Color32::RED, "User preset should override built-in preset with same name");
    }

    #[test]
    fn test_background_override_logic() {
        let mut config = AppConfig::default();
        
        // 1. Set a base preset (Neon Tokyo has a dark background by default)
        config.profile.color_link = ColorRef::Preset("Neon Tokyo".to_string());
        
        // 2. Set an explicit override
        let override_color = Color32::from_rgb(100, 100, 100);
        config.profile.background = Some(override_color);

        // 3. Resolve
        let resolved = config.resolve_colors(&[]);

        // 4. Assert the background changed, but other colors (like 'low') remained Neon Tokyo's
        assert_eq!(resolved.background, override_color, "Background override failed");
        assert_ne!(resolved.low, Color32::BLACK, "Preset colors should still be present");
    }

    // --- 3. Filename Sanitization ---
    // Critical: Prevents file system errors or overwrites
    #[test]
    fn test_filename_sanitization() {
        // We need to access the private helper. 
        // Rust unit tests in the same file CAN access private methods.
        assert_eq!(AppConfig::sanitize_filename("Cool Preset"), "cool_preset");
        assert_eq!(AppConfig::sanitize_filename("My/Preset!"), "mypreset");
        assert_eq!(AppConfig::sanitize_filename("  Trim Me  "), "trim_me");
        assert_eq!(AppConfig::sanitize_filename("O'Reilly"), "oreilly");
    }
}