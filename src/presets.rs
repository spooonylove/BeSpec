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
    ]
}

/// Returns all built-in Visual Profiles
pub fn built_in_visuals() -> Vec<VisualProfile> {
    vec![
        VisualProfile::default(), // Classic

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
            overlay_font: ThemeFont::Standard,
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
        }
    ]
}


/*
==== OLD PRESETS - KEEP FOR REFERENCE ====

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
            
            */