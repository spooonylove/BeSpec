use std::time::{Duration, Instant};
use crate::fft_config::FFTInfo;
use serde::{Serialize, Deserialize};
use std::fs;
use std::path::PathBuf;
use directories::ProjectDirs;

use tracing::{info, error};

pub const SILENCE_DB: f32 = -140.0;

/// Main Shared state container -- wrapped in Arc<Mutx<>> for thread safety
/// 
///  This struct is shared between:
///  - FFT thread (writes visualization data, reads config)
///  - GUI thread (reads visualization data, writes config)
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

}

impl SharedState {

    
    /// Create new shated state with default values
    pub fn new() -> Self {
        let config = AppConfig::load();
        
        Self {
            visualization: VisualizationData::new(config.num_bars),
            performance: PerformanceStats::default(),
            config,
            audio_devices: Vec::new(),
            device_changed: false,
            refresh_devices_requested: false,
        
        }
    }

}

impl Default for SharedState {
    fn default() -> Self {
        Self::new()
    }
}

/// Current visualization data (updated by FFT thread)
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
    /// Total audio frames processed
    pub frame_count: u64,

    /// Average FFT processing time
    pub fft_ave_time: Duration,

    /// Min FFT processing time
    pub fft_min_time: Duration,

    /// Max FFT processing time
    pub fft_max_time: Duration,

    /// Current GUI frame rate (updated by GUI)
    pub gui_fps: f32,

    /// Ya know.. the stats.
    pub fft_info: FFTInfo,
}

/// Visualization Mode
/// 
/// Set by user, GUI render loop uses this to choose rendering method
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum VisualMode {
    SolidBars,
    SegmentedBars,
    LineSpectrum,
    Oscilloscope,
}

/// Application configuration (users settings)
/// 
/// GUI writes these values, FFT thread reads them
#[derive(Clone, Serialize, Deserialize)]
pub struct AppConfig {
    // === Visual Settings ===

    /// Visualization Mode
    pub visual_mode: VisualMode,

    /// Number of frequency bars to display (16-512)
    pub num_bars: usize,

    /// Gap between bars in pixels (0-10)
    pub bar_gap_px: u32,

    /// Opacity of bars (0.0 = transparent, 1.0 = opaque)
    pub bar_opacity: f32,

    /// Opacity of background (0.0 = transparent, 1.0 = opaque)
    pub background_opacity: f32,

    /// Show peak hold indicators
    pub show_peaks: bool,

    /// Show performance statistics overlay
    pub show_stats: bool,

    /// Opacity of stats text (0.0 = transparent, 1.0 = opaque)
    pub stats_opacity: f32,

    /// Invert the spectrum (bars grow from top to bottom)
    pub inverted_spectrum: bool,

    /// Segment height (for segmented mode)
    pub segment_height_px: f32,    

    /// Gap betwixt bar segments (for segmented mode)
    pub segment_gap_px: f32,

    /// Fill Peaks true (solid) or false (floating peak)
    pub fill_peaks: bool,

    // === Inspector Settings ===
    /// Enable the mouse-over inspector tool
    pub inspector_enabled: bool,
    /// Opacity of the inspector overlay
    pub inspector_opacity: f32,

    // === Window Settings ===
    /// Keep window above all others
    pub always_on_top: bool,

    /// Allow clicking through windo (pass events to windows below)
    pub click_through: bool,

    /// Shop window title bar and borders
    pub window_decorations: bool,

    /// Saved Window dimensions [width, height
    pub window_size:  [f32; 2],

    /// Saved Window position on screen [x, y]
    pub window_position: Option<[f32; 2]>,

    // === Audio Settings ===
    /// Name of selected input device (default: "Default")
    pub selected_device: String,

    /// Sensitivity multiplier (0.1 - 10.0)
    pub sensitivity: f32,

    /// The lowest dB value to display (the "floor")
    pub noise_floor_db: f32,
    /// How fast bars rise (milliseconds)
    pub attack_time_ms: f32,

    /// How fast bars fall (milliseconds)
    pub release_time_ms: f32,

    /// Duration of peak hold (milliseconds)
    pub peak_hold_time_ms: f32,

    /// How fast peak falls (milliseconds)
    pub peak_release_time_ms: f32,

    /// Use peak aggregation (true) or average (false) for bar grouping
    pub use_peak_aggregation: bool,

    // === Color Settings === 
    pub color_scheme: ColorScheme,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            visual_mode: VisualMode::SolidBars,
            num_bars: 150,
            bar_gap_px: 2,
            bar_opacity: 1.0,
            background_opacity: 1.0,
            show_peaks: true,
            show_stats: false,
            stats_opacity: 0.3,
            inverted_spectrum: false,
            segment_height_px: 4.0,
            segment_gap_px: 2.0,
            fill_peaks: false,

            // Inspector Settings
            inspector_enabled: true,
            inspector_opacity: 0.9,

            // Window Settings
            always_on_top: false,
            click_through: false,
            window_decorations: false,
            window_size: [800.0, 400.0],
            window_position: None,

            // Audio Settings
            selected_device: "Default".to_string(),
            sensitivity: 1.0,
            noise_floor_db: -60.0,
            attack_time_ms: 20.0,
            release_time_ms: 200.0,
            peak_hold_time_ms: 1000.0,
            peak_release_time_ms: 1500.0,
            use_peak_aggregation: true,

            // Color Settings 
            color_scheme: ColorScheme::default(),
        }
    }
}

impl AppConfig {
    /// Returns the standard OS config path, e.g.:
    /// Windows: C:\Users\Username\AppData\Roaming\BeAnal
    /// MacOS: /Users/Username/Library/Application Support/BeAnal
    /// Linux: /home/username/.config/BeAnal
    fn get_config_path() -> PathBuf {
        if let Some(proj_dirs) = ProjectDirs::from("","","beanal") {
            let config_dir = proj_dirs.config_dir();

            // Ensure directory exists
            if let Err(e) = fs::create_dir_all(config_dir) {
                tracing::error!("[Config] Error creating config directory: {}", e);
            }

            return config_dir.join("config.json");
        }
        
        // Fallback
        PathBuf::from("beanal_config.json")
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
    
    
    /// Check if this config requires rebuilding the FFT processor
    pub fn needs_fft_rebuild(&self, other: &AppConfig) -> bool {
        self.num_bars != other.num_bars
    }


    /// Apply a color preset by name
    pub fn apply_preset(&mut self, preset_name: &str) {
        if let Some(preset) = ColorPreset::find(preset_name) {
            self.color_scheme = ColorScheme::Preset {
                name: preset.name, 
                low: preset.low,
                high: preset.high,
                peak: preset.peak,
            };
        }
    }

    /// Get current preset name or scheme name
    pub fn scheme_name(&self) -> String {
        match &self.color_scheme {
            ColorScheme::Preset { name, ..} => name.clone(),
            ColorScheme::Custom { .. } => "Custom".to_string(),
            ColorScheme::Rainbow => "Rainbow".to_string(),
        }
 
 
    }

    /// Get the colors from current scheme (low, high, peak)
    pub fn get_colors(&self) -> (Color32, Color32, Color32) {
        match &self.color_scheme {
            ColorScheme::Preset { low, high, peak, .. } => (*low, *high, *peak),
            ColorScheme::Custom { low, high, peak } => (*low, *high, *peak),
            ColorScheme::Rainbow => {
                // Rainboe doesnt used fixed colors, but returns default for UI display
                (Color32::RED, Color32::BLUE, Color32::WHITE)
            }
            
        }
    }

    /// Set Custom Colors (switches to Custom mode)
    pub fn set_custom_colors(&mut self, low: Color32, high: Color32, peak: Color32) {
        self.color_scheme = ColorScheme::Custom { low, high, peak };
    }
}

/// Color scheme options for Visualization
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum ColorScheme {
    /// Named Presets (includes names and colors together)
    Preset {
        name: String,
        low: Color32,
        high: Color32,
        peak: Color32,
    },

    /// Custom Colors  (user defined, not a preset)
    Custom {
        low: Color32,
        high: Color32,
        peak: Color32,
    },

    /// Rainbow effect across frequenct spectrum
    Rainbow,
}

/// Named color preset with name and colors
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorPreset {
    pub name: String,
    pub low: Color32,
    pub high: Color32,
    pub peak: Color32,
}

impl ColorPreset {
    #[allow(dead_code)]
    pub fn new(name: &str, low: Color32, high: Color32, peak: Color32) -> Self {
        Self {
            name: name.to_string(),
            low,
            high,
            peak,
        }
    }

    /// Get all built-in color presets
    pub fn all_presets() -> Vec<ColorPreset> {
        vec![
            ColorPreset {
                name: "Classic Winamp".to_string(),
                low: Color32::from_rgb(50, 205, 50),    // LimeGreen
                high: Color32::from_rgb(255, 255, 0),   // Yellow
                peak: Color32::from_rgb(255, 0, 0),     // Red
            },
            ColorPreset {
                name: "Ocean Blue".to_string(),
                low: Color32::from_rgb(30, 144, 255),   // DodgerBlue
                high: Color32::from_rgb(0, 255, 255),   // Cyan
                peak: Color32::from_rgb(255, 255, 255), // White
            },
            ColorPreset {
                name: "Sunset".to_string(),
                low: Color32::from_rgb(255, 69, 0),     // OrangeRed
                high: Color32::from_rgb(255, 255, 0),   // Yellow
                peak: Color32::from_rgb(255, 255, 255), // White
            },
            ColorPreset {
                name: "Synthwave".to_string(),
                low: Color32::from_rgb(255, 0, 255),    // Magenta
                high: Color32::from_rgb(0, 255, 255),   // Cyan
                peak: Color32::from_rgb(255, 255, 0),   // Yellow
            },
            ColorPreset {
                name: "Spy Black".to_string(),
                low: Color32::from_rgb(0, 0, 0),        // Black
                high: Color32::from_rgb(47, 79, 79),    // DarkSlateGray
                peak: Color32::from_rgb(220, 20, 60),   // Crimson
            },
            ColorPreset {
                name: "Forest Canopy".to_string(),
                low: Color32::from_rgb(0, 100, 0),      // DarkGreen
                high: Color32::from_rgb(0, 255, 0),     // Lime
                peak: Color32::from_rgb(255, 255, 0),   // Yellow
            },
            ColorPreset {
                name: "Molten Core".to_string(),
                low: Color32::from_rgb(139, 0, 0),      // DarkRed
                high: Color32::from_rgb(255, 165, 0),   // Orange
                peak: Color32::from_rgb(255, 255, 255), // White
            },
            ColorPreset {
                name: "Arctic Night".to_string(),
                low: Color32::from_rgb(75, 0, 130),     // Indigo
                high: Color32::from_rgb(173, 216, 230), // LightBlue
                peak: Color32::from_rgb(255, 255, 255), // White
            },
            ColorPreset {
                name: "Matrix".to_string(),
                low: Color32::from_rgb(0, 0, 0),        // Black
                high: Color32::from_rgb(0, 255, 0),     // Lime
                peak: Color32::from_rgb(245, 245, 245), // WhiteSmoke
            },
            ColorPreset {
                name: "Bubblegum".to_string(),
                low: Color32::from_rgb(255, 20, 147),   // DeepPink
                high: Color32::from_rgb(0, 255, 255),   // Aqua
                peak: Color32::from_rgb(255, 255, 0),   // Yellow
            },
            ColorPreset {
                name: "Monochrome".to_string(),
                low: Color32::from_rgb(105, 105, 105),  // DimGray
                high: Color32::from_rgb(211, 211, 211), // LightGray
                peak: Color32::from_rgb(255, 255, 255), // White
            },
            ColorPreset {
                name: "Vintage VU".to_string(),
                low: Color32::from_rgb(184, 134, 11),   // DarkGoldenrod
                high: Color32::from_rgb(255, 215, 0),   // Gold
                peak: Color32::from_rgb(205, 92, 92),   // IndianRed
            },
            ColorPreset {
                name: "Deep Space".to_string(),
                low: Color32::from_rgb(0, 0, 0),        // Black
                high: Color32::from_rgb(148, 0, 211),   // DarkViolet
                peak: Color32::from_rgb(0, 255, 255),   // Cyan
            },
            ColorPreset {
                name: "8-Bit Blueberry".to_string(),
                low: Color32::from_rgb(0, 0, 128),      // Navy
                high: Color32::from_rgb(65, 105, 225),  // RoyalBlue
                peak: Color32::from_rgb(255, 255, 255), // White
            },
            ColorPreset {
                name: "Desert Heat".to_string(),
                low: Color32::from_rgb(128, 0, 0),      // Maroon
                high: Color32::from_rgb(255, 69, 0),    // OrangeRed
                peak: Color32::from_rgb(240, 230, 140), // Khaki
            },
            ColorPreset {
                name: "Super Mario Bros.".to_string(),
                low: Color32::from_rgb(0, 0, 205),      // Medium Blue
                high: Color32::from_rgb(220, 20, 60),   // Crimson
                peak: Color32::from_rgb(255, 215, 0),   // Gold
            },
            ColorPreset {
                name: "Halo".to_string(),
                low: Color32::from_rgb(85, 107, 47),    // Olive Drab
                high: Color32::from_rgb(218, 165, 32),  // Goldenrod
                peak: Color32::from_rgb(0, 191, 255),   // Deep Sky Blue (Cortana)
            },
            ColorPreset {
                name: "Fallout".to_string(),
                low: Color32::from_rgb(75, 0, 130),     // Indigo
                high: Color32::from_rgb(0, 255, 255),   // Cyan
                peak: Color32::from_rgb(240, 248, 255), // Alice Blue
            },
            ColorPreset {
                name: "Sith Lord".to_string(),
                low: Color32::from_rgb(20, 20, 20),     // Near Black
                high: Color32::from_rgb(220, 20, 60),   // Crimson
                peak: Color32::from_rgb(255, 255, 255), // White
            },
            ColorPreset {
                name: "Neon Genesis Evangelion".to_string(),
                low: Color32::from_rgb(106, 13, 173),   // Purple
                high: Color32::from_rgb(57, 255, 20),   // Neon Green
                peak: Color32::from_rgb(255, 140, 0),   // Dark Orange
            },
            ColorPreset {
                name: "Neon Tokyo".to_string(),
                low: Color32::from_rgb(255, 0, 127),    // Hot Pink
                high: Color32::from_rgb(0, 255, 255),   // Cyan
                peak: Color32::from_rgb(255, 255, 0),   // Yellow
            },
            ColorPreset {
                name: "Lava Lamp".to_string(),
                low: Color32::from_rgb(128, 0, 128),    // Purple
                high: Color32::from_rgb(255, 140, 0),   // Dark Orange
                peak: Color32::from_rgb(255, 255, 100), // Bright Yellow
            },
            ColorPreset {
                name: "Northern Lights".to_string(),
                low: Color32::from_rgb(0, 100, 0),      // Dark Green
                high: Color32::from_rgb(0, 255, 127),   // Spring Green
                peak: Color32::from_rgb(138, 43, 226),  // Blue Violet
            },
            ColorPreset {
                name: "Cyberpunk".to_string(),
                low: Color32::from_rgb(255, 0, 255),    // Magenta
                high: Color32::from_rgb(0, 255, 255),   // Cyan
                peak: Color32::from_rgb(255, 255, 0),   // Yellow
            },
            ColorPreset {
                name: "Radioactive".to_string(),
                low: Color32::from_rgb(50, 50, 0),      // Dark Yellow
                high: Color32::from_rgb(173, 255, 47),  // Green Yellow
                peak: Color32::from_rgb(255, 0, 0),     // Red
            },
            ColorPreset {
                name: "Ice Fire".to_string(),
                low: Color32::from_rgb(0, 191, 255),    // Deep Sky Blue
                high: Color32::from_rgb(255, 165, 0),   // Orange
                peak: Color32::from_rgb(255, 0, 0),     // Red
            },
            ColorPreset {
                name: "Retrowave".to_string(),
                low: Color32::from_rgb(255, 0, 128),    // Pink
                high: Color32::from_rgb(128, 0, 255),   // Purple
                peak: Color32::from_rgb(0, 255, 255),   // Cyan
            },
            ColorPreset {
                name: "Blood Moon".to_string(),
                low: Color32::from_rgb(25, 0, 0),       // Very Dark Red
                high: Color32::from_rgb(139, 0, 0),     // Dark Red
                peak: Color32::from_rgb(255, 69, 0),    // Orange Red
            },
            ColorPreset {
                name: "Mint Condition".to_string(),
                low: Color32::from_rgb(0, 100, 100),    // Dark Cyan
                high: Color32::from_rgb(127, 255, 212), // Aquamarine
                peak: Color32::from_rgb(255, 255, 255), // White
            },
            ColorPreset {
                name: "Golden Hour".to_string(),
                low: Color32::from_rgb(255, 140, 0),    // Dark Orange
                high: Color32::from_rgb(255, 215, 0),   // Gold
                peak: Color32::from_rgb(255, 250, 205), // Lemon Chiffon
            },
            ColorPreset {
                name: "Tequila Sunrise".to_string(),
                low: Color32::from_rgb(178, 34, 34),    // Firebrick
                high: Color32::from_rgb(255, 165, 0),   // Orange
                peak: Color32::from_rgb(255, 255, 0),   // Yellow
            },
            ColorPreset {
                name: "Espresso Martini".to_string(),
                low: Color32::from_rgb(28, 20, 13),     // Very Dark Brown
                high: Color32::from_rgb(160, 82, 45),   // Sienna
                peak: Color32::from_rgb(255, 248, 220), // Cornsilk
            },
            ColorPreset {
                name: "Cotton Candy".to_string(),
                low: Color32::from_rgb(255, 105, 180),  // Hot Pink
                high: Color32::from_rgb(135, 206, 250), // Light Sky Blue
                peak: Color32::from_rgb(255, 255, 255), // White
            },


        ]
    }

    /// Find preset by name
    pub fn find(name: &str) -> Option<ColorPreset> {
        Self::all_presets().into_iter().find(|p| p.name == name)
    }

    /// get preset names for UI dropdown
    pub fn preset_names() -> Vec<String> {
        Self::all_presets().into_iter().map(|p| p.name).collect()
    }
}
 impl Default for ColorScheme {
    fn default() -> Self {
        // Start withe Classic Winamp as default
        ColorScheme::Preset {
            name: "Classic Winamp".to_string(),
            low: Color32::from_rgb(50, 205, 50),
            high: Color32::from_rgb(255, 255, 0),
            peak: Color32::from_rgb(255, 0, 0),
        }   
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

    pub const WHITE: Self = Self::from_rgb(255, 255, 255);
    pub const BLACK: Self = Self::from_rgb(0, 0, 0);
    pub const RED: Self = Self::from_rgb(255, 0, 0);

    #[allow(dead_code)]
    pub const GREEN: Self = Self::from_rgb(0, 255, 0);
    pub const BLUE: Self = Self::from_rgb(0, 0, 255);

    /// Linear interpolation between two colors
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: (self.r as f32 + (other.r as f32 - self.r as f32) * t) as u8,
            g: (self.g as f32 + (other.g as f32 - self.g as f32) * t) as u8,
            b: (self.b as f32 + (other.b as f32 - self.b as f32) * t) as u8,
            a: (self.a as f32 + (other.a as f32 - self.a as f32) * t) as u8,
            
        }
    }

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

    // === Tests for Complex Logic ===
    #[test]
    fn test_needs_fft_rebuild() {
        let mut config1 = AppConfig::default();
        let config2 =  AppConfig::default();

        // Same config - no rebuild needed
        assert!(!config1.needs_fft_rebuild(&config2));

        // change bar count -- needs rebuild
        config1.num_bars = 256;
        assert!(config1.needs_fft_rebuild(&config2));

        // Change sensitivity - NO rebuild needed 
        config1.num_bars = 150;
        config1.sensitivity = 5.0;
        assert!(!config1.needs_fft_rebuild(&config2));
        
        // Change attack time - No Rebuild Required!
        config1.attack_time_ms = 20.0;
        assert!(!config1.needs_fft_rebuild(&config2));

    }

    // === Test for invariants ===

    #[test]
    fn test_color_lerp_boundaries() {
        let black = Color32::BLACK;
        let white = Color32::WHITE;

        // Test 0% (should be first color)
        let result = black.lerp(white, 0.0);
        assert_eq!(result, black);

        // Test 100% (should be second color)
        let result = black.lerp(white, 1.0);
        assert_eq!(result, white);

        // Testin clamping below 0
        let result = black.lerp(white, -0.1);
        assert_eq!(result, black);

        // Test clamping above 1
        let result = black.lerp(white, 1.1);
        assert_eq!(result, white);
    
        // test midpoint (should be gray)
        let gray  = black.lerp(white, 0.5);
        assert_eq!(gray.r, 127);
        assert_eq!(gray.g, 127);
        assert_eq!(gray.b, 127);
    }

    // === Tests for State Transitions

    #[test]
    fn test_preset_to_custom_transition() {
        let mut config = AppConfig::default();

        // start with preset
        assert_eq!(config.scheme_name(), "Classic Winamp");

        //Switch to custom colors
        config.set_custom_colors(
            Color32::from_rgb(100, 0, 0),
            Color32::from_rgb(200, 0, 0),
            Color32::WHITE,
        );

        // Scheme mode should be in Custom
        assert_eq!(config.scheme_name(), "Custom");

        // Colors should match what we set
        let (low, high, peak) = config.get_colors();
        assert_eq!(low.r, 100);
        assert_eq!(high.r, 200);
        assert_eq!(peak, Color32::WHITE);
    }

    #[test]
    fn test_custom_to_preset_transition() {
        let mut config = AppConfig::default();

        // start with custom
        config. set_custom_colors(
            Color32::BLACK,
            Color32::WHITE,
            Color32::RED,
        );
        assert_eq!(config.scheme_name(), "Custom");

        // Switch to preset
        config.apply_preset("Synthwave");
        assert_eq!(config.scheme_name(), "Synthwave");

        // Colors should match SynthWave Preset
        let(low, high, peak) = config.get_colors();
        assert_eq!(low, Color32::from_rgb(255, 0, 255));    // Magenta
        assert_eq!(high, Color32::from_rgb(0, 255, 255));   // Cyan
        assert_eq!(peak, Color32::from_rgb(255, 255, 0));   // Yellow
    }

    #[test]
    fn test_rainbow_mode () {
        let mut config = AppConfig::default();

        // switch to  with Rainbow
        config.color_scheme = ColorScheme::Rainbow;
        assert_eq!(config.scheme_name(), "Rainbow");

        //get_colors() should return default colors (rainbox does use fixed colors)
        let (low, high, peak) = config.get_colors();
        assert_eq!(low, Color32::RED);
        assert_eq!(high, Color32::BLUE);
        assert_eq!(peak, Color32::WHITE);
    
    }

    // === Test for Data Integrity ===

    #[test]
    fn test_preset_data_integrity() {
        // Verifyt a few key presets have correct colors
        let classic = ColorPreset::find("Classic Winamp").unwrap();
        assert_eq!(classic.low, Color32::from_rgb(50, 205, 50));     //LimeGreen
        assert_eq!(classic.high, Color32::from_rgb(255, 255, 0));    // Yellow
        assert_eq!(classic.peak, Color32::from_rgb(255, 0, 0));      // Red

        let synthwave = ColorPreset::find("Synthwave").unwrap();
        assert_eq!(synthwave.low, Color32::from_rgb(255, 0, 255));   //Magenta
        assert_eq!(synthwave.high, Color32::from_rgb(0, 255, 255));  // Cyan
        assert_eq!(synthwave.peak, Color32::from_rgb(255, 255, 0));  // Yellow
    }

    #[test]
    fn test_all_presets_accessible() {
        let presets = ColorPreset::all_presets();
        
        // Should have at least 25 presets
        assert!(presets.len() >= 25);

        // Each preset should be findable by name
        for preset in &presets {
            let found = ColorPreset::find(&preset.name);
            assert!(found.is_some(), "Preset '{}' not findable", preset.name);
        }

        // No duplicate names
        let names: Vec<String> = presets.iter().map(|p| p.name.clone()).collect();
        let mut sorted_names = names.clone();
        sorted_names.sort();
        sorted_names.dedup();
        assert_eq!(names.len(), sorted_names.len(), "Duplicate preset names found");
    }
}

