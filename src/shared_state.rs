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

// === Media Options ===
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum MediaDisplayMode {
    FadeOnUpdate,   // Fade for N seconds the fade
    AlwaysOn,       // Always visible
    Off,            // Hidden
}

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

}

impl SharedState {
    pub fn new() -> Self {
        let config = AppConfig::load();
        Self {
            visualization: VisualizationData::new(config.num_bars),
            performance: PerformanceStats::default(),
            config,
            audio_devices: Vec::new(),
            device_changed: false,
            refresh_devices_requested: false,
            media_info: None,
            last_media_update: None,
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

    ///  "Ghost Mode": Window is click-through until focused with alt-tab
    pub window_locked: bool,

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

    // === Media Settings ===
    pub media_display_mode: MediaDisplayMode,
    pub media_fade_duration_sec: f32,

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
            inspector_opacity: 0.3,

            // Window Settings
            always_on_top: false,
            window_locked: false,
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

            // Media Settings
            media_display_mode: MediaDisplayMode::FadeOnUpdate,
            media_fade_duration_sec: 5.0,

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
        if let Some(proj_dirs) = ProjectDirs::from("","","BeAnal") {
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
        }
    }

    /// Get the colors from current scheme (low, high, peak)
    pub fn get_colors(&self) -> (Color32, Color32, Color32) {
        match &self.color_scheme {
            ColorScheme::Preset { low, high, peak, .. } => (*low, *high, *peak),
            ColorScheme::Custom { low, high, peak } => (*low, *high, *peak),
            }
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


    // 1. Test Configuration Defaults
    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        
        // Assert Critical Defaults are sensible
        assert_eq!(config.num_bars, 150);
        assert_eq!(config.visual_mode, VisualMode::SolidBars);
        assert!(config.show_peaks);
        assert_eq!(config.segment_height_px, 4.0); // verify new fields exist and have defaults

        // Assert Default Color Scheme
        match config.color_scheme {
            ColorScheme::Preset { name, .. } => assert_eq!(name, "Classic Winamp"),
            _ => panic!("Default should be Classic Winamp"),
        }
    }

    // 2. Test Data Structure Initialization
    #[test]
    fn test_visualization_buffer_init() {
        let viz = VisualizationData::new(100);
        
        assert_eq!(viz.bars.len(), 100);
        assert_eq!(viz.peaks.len(), 100);
        assert_eq!(viz.waveform.len(), 2048); // Fixed buffer size
        
        // Ensure initialized to silence
        assert_eq!(viz.bars[0], SILENCE_DB);
        assert_eq!(viz.peaks[0], SILENCE_DB);
        assert_eq!(viz.waveform[0], 0.0);
    }

    // 3. Test Color Scheme Logic & Transitions
    #[test]
    fn test_scheme_transitions() {
        let mut config = AppConfig::default();

        // A. Starts as Preset
        assert_eq!(config.scheme_name(), "Classic Winamp");
        let (low1, _, _) = config.get_colors();

        // B. Switch to a different Preset
        config.apply_preset("Cyberpunk");
        assert_eq!(config.scheme_name(), "Cyberpunk");
        let (low2, _, _) = config.get_colors();
        assert_ne!(low1, low2); // Colors should change

        // C. Switch to Custom
        config.color_scheme = ColorScheme::Custom { 
            low: Color32::WHITE, 
            high: Color32::BLACK, 
            peak: Color32::RED 
        };
        assert_eq!(config.scheme_name(), "Custom");
        
        let (c_low, c_high, c_peak) = config.get_colors();
        assert_eq!(c_low, Color32::WHITE);
        assert_eq!(c_high, Color32::BLACK);
        assert_eq!(c_peak, Color32::RED);

        // D. Switch back to Preset
        config.apply_preset("Classic Winamp");
        assert_eq!(config.scheme_name(), "Classic Winamp");
    }

    // 4. Test Preset Integrity
    #[test]
    fn test_presets_exist() {
        let names = ColorPreset::preset_names();
        assert!(names.contains(&"Classic Winamp".to_string()));
        assert!(names.contains(&"Synthwave".to_string()));
        assert!(!names.contains(&"Non Existent Preset 12345".to_string()));

        // Ensure find() works
        let found = ColorPreset::find("Ocean Blue");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Ocean Blue");
    }

    // 5. Test Color Math
    #[test]
    fn test_color_opacity() {
        let white = Color32::WHITE; // 255, 255, 255, 255

        // Half opacity
        let semi = white.with_opacity(0.5);
        assert_eq!(semi.a, 127); // 255 * 0.5 = 127.5 -> 127

        // Clamp validation
        let over = white.with_opacity(2.0);
        assert_eq!(over.a, 255);

        let under = white.with_opacity(-1.0);
        assert_eq!(under.a, 0);
    }
    
}

