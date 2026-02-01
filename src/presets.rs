use crate::shared_state::{Color32, ColorProfile, ColorRef, ThemeFont, VisualMode, VisualProfile};

/// Returns all built-in Color Profiles
pub fn built_in_colors() -> Vec<ColorProfile> {
    vec![
        ColorProfile::default(), // Classic Winamp, perhaps?

        ColorProfile {
            name: "Neon Tokyo".to_string(),
            low: Color32::from_rgb(255, 0, 127),    // Hot Pink
            high: Color32::from_rgb(0, 255, 255),   // Cyan
            peak: Color32::from_rgb(255, 255, 0),   // Yellow
            background: Color32::from_rgb(5, 5, 10), // Deep Void
            text: Color32::from_rgb(0, 255, 255),    // Cyan Text
            inspector_bg: Color32::from_rgb(0, 0, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(0, 255, 255),
        },

        ColorProfile {
            name: "Blueprint (Light)".to_string(),
            low: Color32::from_rgb(255, 255, 255),
            high: Color32::from_rgb(200, 200, 255),
            peak: Color32::from_rgb(255, 50, 50),
            background: Color32::from_rgb(20, 40, 100), // Blueprint Blue
            text: Color32::from_rgb(255, 255, 255),
            inspector_bg: Color32::from_rgb(0, 0, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 50, 50),
        },

        ColorProfile {
            name: "Ghost Mode".to_string(),
            low: Color32::from_rgb(255, 255, 255).with_opacity(0.5),
            high: Color32::from_rgb(255, 255, 255),
            peak: Color32::from_rgb(255, 0, 0),
            background: Color32::from_rgb(0, 0, 0).with_opacity(0.1), // 10% Opacity
            text: Color32::from_rgb(200, 200, 200),
            inspector_bg: Color32::from_rgb(0, 0, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 0, 0),
        },

        ColorProfile {
            name: "Deep Ocean".to_string(),
            low: Color32::from_rgb(30, 144, 255),   // Dodger Blue
            high: Color32::from_rgb(0, 255, 255),   // Aqua
            peak: Color32::from_rgb(255, 255, 255), // White
            background: Color32::from_rgb(5, 10, 30), // Navy
            text: Color32::from_rgb(200, 240, 255),
            inspector_bg: Color32::from_rgb(0, 0, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(0, 255, 255),
        },

        ColorProfile {
            name: "Cyberpunk City".to_string(),
            low: Color32::from_rgb(255, 0, 255),    // Magenta
            high: Color32::from_rgb(0, 255, 255),   // Cyan
            peak: Color32::from_rgb(255, 255, 0),   // Yellow
            background: Color32::from_rgb(10, 5, 20), // Dark Purple tint
            text: Color32::from_rgb(255, 0, 255),
            inspector_bg: Color32::from_rgb(0, 0, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(0, 255, 255),
        },

        // === Restored Legacy Presets ===
        
        ColorProfile {
            name: "Ocean Blue".to_string(),
            low: Color32::from_rgb(30, 144, 255),
            high: Color32::from_rgb(0, 255, 255),
            peak: Color32::from_rgb(255, 255, 255),
            background: Color32::from_rgb(5, 10, 40), // Deep Sea
            text: Color32::from_rgb(200, 240, 255),
            inspector_bg: Color32::from_rgb(0, 0, 30).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(0, 255, 255),
        },
        ColorProfile {
            name: "Sunset".to_string(),
            low: Color32::from_rgb(255, 69, 0),
            high: Color32::from_rgb(255, 255, 0),
            peak: Color32::from_rgb(255, 255, 255),
            background: Color32::from_rgb(30, 10, 20), // Twilight
            text: Color32::from_rgb(255, 200, 150),
            inspector_bg: Color32::from_rgb(20, 5, 10).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 215, 0),
        },
        ColorProfile {
            name: "Synthwave".to_string(),
            low: Color32::from_rgb(255, 0, 255),
            high: Color32::from_rgb(0, 255, 255),
            peak: Color32::from_rgb(255, 255, 0),
            background: Color32::from_rgb(15, 0, 25), // Dark Grid Purple
            text: Color32::from_rgb(255, 100, 200),
            inspector_bg: Color32::from_rgb(20, 0, 30).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(0, 255, 255),
        },
        ColorProfile {
            name: "Spy Black".to_string(),
            low: Color32::from_rgb(0, 0, 0),
            high: Color32::from_rgb(47, 79, 79),
            peak: Color32::from_rgb(220, 20, 60),
            background: Color32::from_rgb(5, 5, 5), // Almost Pitch Black
            text: Color32::from_rgb(200, 200, 200), // Silver
            inspector_bg: Color32::from_rgb(0, 0, 0).with_opacity(0.95),
            inspector_fg: Color32::from_rgb(220, 20, 60),
        },
        ColorProfile {
            name: "Forest Canopy".to_string(),
            low: Color32::from_rgb(0, 100, 0),
            high: Color32::from_rgb(0, 255, 0),
            peak: Color32::from_rgb(255, 255, 0),
            background: Color32::from_rgb(5, 20, 5), // Deep Forest
            text: Color32::from_rgb(150, 255, 150),
            inspector_bg: Color32::from_rgb(0, 15, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(0, 255, 0),
        },
        ColorProfile {
            name: "Molten Core".to_string(),
            low: Color32::from_rgb(139, 0, 0),
            high: Color32::from_rgb(255, 165, 0),
            peak: Color32::from_rgb(255, 255, 255),
            background: Color32::from_rgb(25, 5, 0), // Magma Rock
            text: Color32::from_rgb(255, 200, 150),
            inspector_bg: Color32::from_rgb(20, 0, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 165, 0),
        },
        ColorProfile {
            name: "Arctic Night".to_string(),
            low: Color32::from_rgb(75, 0, 130),
            high: Color32::from_rgb(173, 216, 230),
            peak: Color32::from_rgb(255, 255, 255),
            background: Color32::from_rgb(5, 5, 25), // Midnight
            text: Color32::from_rgb(220, 240, 255),
            inspector_bg: Color32::from_rgb(10, 10, 40).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(0, 255, 255),
        },
        ColorProfile {
            name: "Matrix".to_string(),
            low: Color32::from_rgb(0, 0, 0),
            high: Color32::from_rgb(0, 255, 0),
            peak: Color32::from_rgb(245, 245, 245),
            background: Color32::from_rgb(0, 10, 0), // Dark Code
            text: Color32::from_rgb(0, 255, 0),
            inspector_bg: Color32::from_rgb(0, 0, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(0, 255, 0),
        },
        ColorProfile {
            name: "Bubblegum".to_string(),
            low: Color32::from_rgb(255, 20, 147),
            high: Color32::from_rgb(0, 255, 255),
            peak: Color32::from_rgb(255, 255, 0),
            background: Color32::from_rgb(40, 20, 40), // Dark Plum
            text: Color32::from_rgb(255, 200, 255),
            inspector_bg: Color32::from_rgb(50, 10, 30).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(0, 255, 255),
        },
        ColorProfile {
            name: "Monochrome".to_string(),
            low: Color32::from_rgb(105, 105, 105),
            high: Color32::from_rgb(211, 211, 211),
            peak: Color32::from_rgb(255, 255, 255),
            background: Color32::from_rgb(20, 20, 20), // Dark Gray
            text: Color32::from_rgb(220, 220, 220),
            inspector_bg: Color32::from_rgb(0, 0, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 255, 255),
        },
        ColorProfile {
            name: "Vintage VU".to_string(),
            low: Color32::from_rgb(184, 134, 11),
            high: Color32::from_rgb(255, 215, 0),
            peak: Color32::from_rgb(205, 92, 92),
            background: Color32::from_rgb(35, 25, 15), // Wood/Bakelite
            text: Color32::from_rgb(240, 230, 200), // Aged Paper
            inspector_bg: Color32::from_rgb(20, 10, 5).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 215, 0),
        },
         ColorProfile {
            name: "BeOS Desktop".to_string(),
            low: Color32::from_rgb(230, 166, 0),
            high: Color32::from_rgb(255, 242, 153),
            peak: Color32::from_rgb(255, 147, 27),
            background: Color32::from_rgb(51, 102, 152),
            text: Color32::from_rgb(220, 220, 220), 
            inspector_bg: Color32::from_rgb(133, 133, 133).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 255, 255),
        },
        ColorProfile {
            name: "Deep Space".to_string(),
            low: Color32::from_rgb(0, 0, 0),
            high: Color32::from_rgb(148, 0, 211),
            peak: Color32::from_rgb(0, 255, 255),
            background: Color32::from_rgb(0, 0, 0), // Void
            text: Color32::from_rgb(255, 255, 255), // Stars
            inspector_bg: Color32::from_rgb(10, 0, 20).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(148, 0, 211),
        },
        ColorProfile {
            name: "8-Bit Blueberry".to_string(),
            low: Color32::from_rgb(0, 0, 128),
            high: Color32::from_rgb(65, 105, 225),
            peak: Color32::from_rgb(255, 255, 255),
            background: Color32::from_rgb(0, 0, 40), // Dark Blue
            text: Color32::from_rgb(255, 255, 255),
            inspector_bg: Color32::from_rgb(0, 0, 60).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(65, 105, 225),
        },
        ColorProfile {
            name: "Desert Heat".to_string(),
            low: Color32::from_rgb(128, 0, 0),
            high: Color32::from_rgb(255, 69, 0),
            peak: Color32::from_rgb(240, 230, 140),
            background: Color32::from_rgb(40, 15, 5), // Scorched Earth
            text: Color32::from_rgb(255, 255, 200),
            inspector_bg: Color32::from_rgb(30, 10, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 69, 0),
        },
        ColorProfile {
            name: "Super Mario Bros.".to_string(),
            low: Color32::from_rgb(0, 0, 205),
            high: Color32::from_rgb(220, 20, 60),
            peak: Color32::from_rgb(255, 215, 0),
            background: Color32::from_rgb(0, 0, 50), // Underground Blue
            text: Color32::from_rgb(255, 215, 0), // Coin Gold
            inspector_bg: Color32::from_rgb(50, 0, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 255, 255),
        },
        ColorProfile {
            name: "Halo".to_string(),
            low: Color32::from_rgb(85, 107, 47),
            high: Color32::from_rgb(218, 165, 32),
            peak: Color32::from_rgb(0, 191, 255),
            background: Color32::from_rgb(20, 30, 20), // Armor Green
            text: Color32::from_rgb(0, 200, 255), // Cortana Blue
            inspector_bg: Color32::from_rgb(15, 20, 10).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(218, 165, 32),
        },
        ColorProfile {
            name: "Fallout".to_string(),
            low: Color32::from_rgb(75, 0, 130),
            high: Color32::from_rgb(0, 255, 255),
            peak: Color32::from_rgb(240, 248, 255),
            background: Color32::from_rgb(0, 20, 0), // Pip-Boy Dark
            text: Color32::from_rgb(0, 255, 0), // Phosphor Green
            inspector_bg: Color32::from_rgb(0, 30, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(0, 255, 255),
        },
        ColorProfile {
            name: "Sith Lord".to_string(),
            low: Color32::from_rgb(20, 20, 20),
            high: Color32::from_rgb(220, 20, 60),
            peak: Color32::from_rgb(255, 255, 255),
            background: Color32::from_rgb(10, 5, 5), // Dark Side
            text: Color32::from_rgb(255, 50, 50),
            inspector_bg: Color32::from_rgb(0, 0, 0).with_opacity(0.95),
            inspector_fg: Color32::from_rgb(255, 0, 0),
        },
        ColorProfile {
            name: "Neon Genesis Evangelion".to_string(),
            low: Color32::from_rgb(106, 13, 173),
            high: Color32::from_rgb(57, 255, 20),
            peak: Color32::from_rgb(255, 140, 0),
            background: Color32::from_rgb(20, 10, 30), // Eva-01 Dark
            text: Color32::from_rgb(255, 140, 0), // HUD Orange
            inspector_bg: Color32::from_rgb(30, 10, 40).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(57, 255, 20),
        },
        ColorProfile {
            name: "Lava Lamp".to_string(),
            low: Color32::from_rgb(128, 0, 128),
            high: Color32::from_rgb(255, 140, 0),
            peak: Color32::from_rgb(255, 255, 100),
            background: Color32::from_rgb(20, 0, 10), // Dark Magma
            text: Color32::from_rgb(255, 255, 200),
            inspector_bg: Color32::from_rgb(20, 0, 20).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 140, 0),
        },
        ColorProfile {
            name: "Northern Lights".to_string(),
            low: Color32::from_rgb(0, 100, 0),
            high: Color32::from_rgb(0, 255, 127),
            peak: Color32::from_rgb(138, 43, 226),
            background: Color32::from_rgb(5, 10, 25), // Night Sky
            text: Color32::from_rgb(100, 255, 200),
            inspector_bg: Color32::from_rgb(0, 10, 20).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(138, 43, 226),
        },
        ColorProfile {
            name: "Radioactive".to_string(),
            low: Color32::from_rgb(50, 50, 0),
            high: Color32::from_rgb(173, 255, 47),
            peak: Color32::from_rgb(255, 0, 0),
            background: Color32::from_rgb(20, 20, 0), // Hazard Dark
            text: Color32::from_rgb(255, 255, 0),
            inspector_bg: Color32::from_rgb(30, 30, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 0, 0),
        },
        ColorProfile {
            name: "Ice Fire".to_string(),
            low: Color32::from_rgb(0, 191, 255),
            high: Color32::from_rgb(255, 165, 0),
            peak: Color32::from_rgb(255, 0, 0),
            background: Color32::from_rgb(20, 0, 40), // Dark Violet
            text: Color32::from_rgb(255, 255, 255),
            inspector_bg: Color32::from_rgb(0, 0, 30).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 165, 0),
        },
        ColorProfile {
            name: "Retrowave".to_string(),
            low: Color32::from_rgb(255, 0, 128),
            high: Color32::from_rgb(128, 0, 255),
            peak: Color32::from_rgb(0, 255, 255),
            background: Color32::from_rgb(15, 0, 25), // Grid Black
            text: Color32::from_rgb(0, 255, 255),
            inspector_bg: Color32::from_rgb(20, 0, 40).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 0, 128),
        },
        ColorProfile {
            name: "Blood Moon".to_string(),
            low: Color32::from_rgb(25, 0, 0),
            high: Color32::from_rgb(139, 0, 0),
            peak: Color32::from_rgb(255, 69, 0),
            background: Color32::from_rgb(10, 0, 0), // Night Black
            text: Color32::from_rgb(255, 100, 100),
            inspector_bg: Color32::from_rgb(20, 0, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 69, 0),
        },
        ColorProfile {
            name: "Mint Condition".to_string(),
            low: Color32::from_rgb(0, 100, 100),
            high: Color32::from_rgb(127, 255, 212),
            peak: Color32::from_rgb(255, 255, 255),
            background: Color32::from_rgb(0, 30, 30), // Dark Teal
            text: Color32::from_rgb(240, 255, 250),
            inspector_bg: Color32::from_rgb(0, 40, 40).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 255, 255),
        },
        ColorProfile {
            name: "Golden Hour".to_string(),
            low: Color32::from_rgb(255, 140, 0),
            high: Color32::from_rgb(255, 215, 0),
            peak: Color32::from_rgb(255, 250, 205),
            background: Color32::from_rgb(40, 20, 0), // Sunset Brown
            text: Color32::from_rgb(255, 215, 0),
            inspector_bg: Color32::from_rgb(30, 15, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 250, 205),
        },
        ColorProfile {
            name: "Tequila Sunrise".to_string(),
            low: Color32::from_rgb(178, 34, 34),
            high: Color32::from_rgb(255, 165, 0),
            peak: Color32::from_rgb(255, 255, 0),
            background: Color32::from_rgb(30, 10, 10), // Deep Red
            text: Color32::from_rgb(255, 255, 200),
            inspector_bg: Color32::from_rgb(40, 10, 10).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 165, 0),
        },
        ColorProfile {
            name: "Espresso Martini".to_string(),
            low: Color32::from_rgb(28, 20, 13),
            high: Color32::from_rgb(160, 82, 45),
            peak: Color32::from_rgb(255, 248, 220),
            background: Color32::from_rgb(15, 10, 10), // Coffee Black
            text: Color32::from_rgb(210, 180, 140), // Crema
            inspector_bg: Color32::from_rgb(20, 15, 10).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 255, 255),
        },
        ColorProfile {
            name: "Cotton Candy".to_string(),
            low: Color32::from_rgb(255, 105, 180),
            high: Color32::from_rgb(135, 206, 250),
            peak: Color32::from_rgb(255, 255, 255),
            background: Color32::from_rgb(20, 30, 50), // Dark Pastel Blue
            text: Color32::from_rgb(255, 192, 203), // Pink
            inspector_bg: Color32::from_rgb(40, 20, 30).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(0, 255, 255),
        },
        // --- Winamp Classic ---
        ColorProfile {
            name: "Classic Skin".to_string(),
            low: Color32::from_rgb(0, 200, 0),     // Green
            high: Color32::from_rgb(220, 220, 0),  // Yellow
            peak: Color32::from_rgb(220, 0, 0),    // Red
            background: Color32::from_rgb(10, 10, 10),
            text: Color32::from_rgb(0, 255, 0),    // Bitmap font green
            inspector_bg: Color32::from_rgb(20, 20, 20).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(0, 255, 0),
        },

        // --- CRT Phosphor (P1 Green) ---
        ColorProfile {
            name: "Phosphor P1".to_string(),
            low: Color32::from_rgb(0, 50, 0),      // Dim trace
            high: Color32::from_rgb(50, 255, 50),  // Bright trace
            peak: Color32::from_rgb(200, 255, 200),// Overdrive
            background: Color32::from_rgb(0, 5, 0),// Dark glass
            text: Color32::from_rgb(50, 255, 50),
            inspector_bg: Color32::from_rgb(0, 20, 0).with_opacity(0.8),
            inspector_fg: Color32::from_rgb(50, 255, 50),
        },

        // --- VFD Amber (Marantz/Pioneer) ---
        ColorProfile {
            name: "VFD Amber".to_string(),
            low: Color32::from_rgb(180, 80, 0),    // Dim Orange
            high: Color32::from_rgb(255, 160, 0),  // Amber
            peak: Color32::from_rgb(255, 220, 100),// Bright Yellow
            background: Color32::from_rgb(15, 5, 0),
            text: Color32::from_rgb(255, 160, 0),
            inspector_bg: Color32::from_rgb(20, 10, 0).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(255, 160, 0),
        },

        // --- VFD Blue (Sony/Panasonic) ---
        ColorProfile {
            name: "VFD Blue".to_string(),
            low: Color32::from_rgb(0, 100, 150),
            high: Color32::from_rgb(0, 200, 255),
            peak: Color32::from_rgb(200, 240, 255),
            background: Color32::from_rgb(0, 5, 15),
            text: Color32::from_rgb(0, 200, 255),
            inspector_bg: Color32::from_rgb(0, 10, 20).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(0, 255, 255),
        },

        // --- Gameboy (Dot Matrix) ---
        ColorProfile {
            name: "Gameboy".to_string(),
            low: Color32::from_rgb(48, 98, 48),    // Dark Olive
            high: Color32::from_rgb(139, 172, 15), // LCD Green
            peak: Color32::from_rgb(155, 188, 15), // Brightest
            background: Color32::from_rgb(15, 56, 15), // Darkest (Off)
            text: Color32::from_rgb(15, 56, 15),   // Text is usually dark on GB
            inspector_bg: Color32::from_rgb(139, 172, 15).with_opacity(0.9),
            inspector_fg: Color32::from_rgb(15, 56, 15),
        },
    ]
}

/// Returns all built-in Visual Profiles
pub fn built_in_visuals() -> Vec<VisualProfile> {
    vec![
        VisualProfile::default(), // Classic

        // --- 1. The "Winamp" Classic ---
        // 19 bars, specific gap, simple colors.
        VisualProfile {
            name: "Llama Whipper".to_string(),
            visual_mode: VisualMode::SolidBars,
            num_bars: 19, // Authentic Winamp bar count
            bar_gap_px: 1,
            overlay_font: ThemeFont::Mini,
            color_link: ColorRef::Preset("Classic Skin".to_string()), // (See below)
            show_peaks: true,
            peak_hold_time_ms: 600.0,
            peak_release_time_ms: 200.0,
            ..VisualProfile::default()
        },

        VisualProfile {
            name: "Beos (Haiku!)".to_string(),
            visual_mode: VisualMode::SolidBars,
            num_bars: 80, // Affects resolution even in scope mode sometimes
            overlay_font: ThemeFont::Small,
            color_link: ColorRef::Preset("BeOS Desktop".to_string()),
            sensitivity: 2.0,
            beos_enabled: true,
            ..VisualProfile::default()
        },

        // --- 2. High-Fidelity Analysis ---
        // Maximizes density for checking mixes.
        VisualProfile {
            name: "Spectrogram 512".to_string(),
            visual_mode: VisualMode::SolidBars,
            num_bars: 512, // Requires a wide window!
            bar_gap_px: 0, // No gaps for maximum data density
            overlay_font: ThemeFont::Small,
            color_link: ColorRef::Preset("Monochrome".to_string()), 
            show_peaks: false, // Peaks are distracting in analysis
            sensitivity: 1.5,
            ..VisualProfile::default()
        },

        // --- 3. Retro 8-Bit ---
        // Very chunky, slow update for that NES feel.
        VisualProfile {
            name: "8-Bit Arcade".to_string(),
            visual_mode: VisualMode::SolidBars,
            num_bars: 10,
            bar_gap_px: 4,
            overlay_font: ThemeFont::Monospace,
            color_link: ColorRef::Preset("Gameboy".to_string()), // (See below)
            attack_time_ms: 0.0, // Instant movement
            release_time_ms: 0.0, // Instant drop (no smoothing)
            show_peaks: false,
            ..VisualProfile::default()
        },

        // --- 4. Analog Oscilloscope (CRT) ---
        // Simulates a lab bench tool.
        VisualProfile {
            name: "Lab Bench CRT".to_string(),
            visual_mode: VisualMode::Oscilloscope,
            num_bars: 256, // High resolution for smooth lines
            overlay_font: ThemeFont::Monospace,
            color_link: ColorRef::Preset("Phosphor P1".to_string()), // (See below)
            sensitivity: 2.5, // Scopes need high gain
            background: Some(Color32::from_rgb(0, 15, 0)), // Slight green tint background
            ..VisualProfile::default()
        },

        // --- 5. 90s Car Stereo (Segmented) ---
        // Dancing lights style.
        VisualProfile {
            name: "Highway Night".to_string(),
            visual_mode: VisualMode::SegmentedBars,
            num_bars: 24,
            segment_height_px: 3.0,
            segment_gap_px: 1.0,
            overlay_font: ThemeFont::Large, // Big text for "Driving"
            color_link: ColorRef::Preset("VFD Amber".to_string()), // (See below)
            show_peaks: true,
            fill_peaks: true, // Connects the bar to the peak
            ..VisualProfile::default()
        },

        // --- 6. Sony Minidisc Deck ---
        // Tight, high-tech segments.
        VisualProfile {
            name: "MD Deck".to_string(),
            visual_mode: VisualMode::SegmentedBars,
            num_bars: 14,
            segment_height_px: 2.0,
            segment_gap_px: 1.0,
            color_link: ColorRef::Preset("VFD Blue".to_string()), // (See below)
            beos_enabled: false,
            ..VisualProfile::default()
        },
        VisualProfile {
            name: "Retro Dashboard".to_string(),
            visual_mode: VisualMode::SegmentedBars,
            num_bars: 64, 
            segment_height_px: 6.0,
            segment_gap_px: 2.0,
            overlay_font: ThemeFont::Monospace,
            color_link: ColorRef::Preset("Neon Tokyo".to_string()),
            attack_time_ms: 10.0,
            release_time_ms: 120.0,
            ..VisualProfile::default()
        },

        VisualProfile {
            name: "Chill Wave".to_string(),
            visual_mode: VisualMode::LineSpectrum,
            num_bars: 256,
            overlay_font: ThemeFont::Medium,
            color_link: ColorRef::Preset("Blueprint (Light)".to_string()),
            attack_time_ms: 80.0,
            release_time_ms: 300.0,
            ..VisualProfile::default()
        },

        VisualProfile {
            name: "Ghost HUD".to_string(),
            visual_mode: VisualMode::LineSpectrum,
            overlay_font: ThemeFont::Monospace,
            color_link: ColorRef::Preset("Ghost Mode".to_string()),
            show_peaks: false,
            background: Some(Color32::from_rgb(0,0,0).with_opacity(0.1)), // Explicit override
            ..VisualProfile::default()
        },

        VisualProfile {
            name: "Engineering".to_string(),
            visual_mode: VisualMode::Oscilloscope,
            num_bars: 256, // Affects resolution even in scope mode sometimes
            overlay_font: ThemeFont::Monospace,
            color_link: ColorRef::Preset("Blueprint (Light)".to_string()),
            sensitivity: 2.0,
            ..VisualProfile::default()
        },
    ]
}
