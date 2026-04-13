use egui::{Color32, FontId, FontFamily};
use crate::shared_state::{Color32 as SharedColor, ThemeFont};
use crate::gui::StateColor32;

// === BeOS / Haiku Design Tokens ====

// 1. Tab Gradients (Warm & Buttery, not White)
// Top: Creamy Yellow (Toned down from the harsh white-yellow)
pub const BEOS_TAB_GRADIENT_TOP: Color32 = Color32::from_rgb(255, 240, 120); 
// Bot: The Classic R5 Gold
pub const BEOS_TAB_GRADIENT_BOT: Color32 = Color32::from_rgb(255, 203, 0);  

// 2. Tab Bevels
pub const BEOS_TAB_HIGHLIGHT: Color32 = Color32::from_rgb(255, 255, 255); // White edge
pub const BEOS_TAB_SHADOW: Color32 = Color32::from_rgb(200, 160, 0);      // Darker Gold shadow

// 3. Window Frame
pub const BEOS_FRAME_LIGHT: Color32 = Color32::from_rgb(255, 255, 255);
pub const BEOS_FRAME_MID: Color32 = Color32::from_rgb(216, 216, 216);
pub const BEOS_FRAME_DARK: Color32 = Color32::from_rgb(150, 150, 150);
pub const BEOS_FRAME_SHADOW: Color32 = Color32::from_rgb(100, 100, 100);

// 4. Button Colors (Now Fully Yellow-Tinted!)
pub const BEOS_BUTTON_BORDER: Color32 = Color32::from_rgb(179, 143, 0); // Dark Gold Border

// Close Button (Subtle Yellow Gradient)
pub const BEOS_CLOSE_TOP: Color32 = Color32::from_rgb(255, 248, 225); 
pub const BEOS_CLOSE_BOT: Color32 = Color32::from_rgb(255, 203, 0);

// Zoom Button: Background (Large/Max State) - Darker/Recessed Yellow
pub const BEOS_ZOOM_BACK_TOP: Color32 = Color32::from_rgb(255, 231, 141);
pub const BEOS_ZOOM_BACK_BOT: Color32 = Color32::from_rgb(255, 217, 70);

// Zoom Button: Foreground (Small/Normal State) - Bright Pop Yellow
pub const BEOS_ZOOM_FRONT_TOP: Color32 = Color32::from_rgb(255, 237, 169);
pub const BEOS_ZOOM_FRONT_BOT: Color32 = Color32::from_rgb(255, 203, 0);

// 5. Metrics
pub const BEOS_TAB_HEIGHT: f32 = 24.0;
pub const BEOS_BORDER_WIDTH: f32 = 4.0;

// === Global UI Constants ===
//pub const PANEL_WIDTH: f32 = 250.0;
//pub const ANIMATION_SPEED: f32 = 0.1;

/// Convert our internal StateColor32 (Straight Alpha) to egui::Color32 (Premultiplied)
pub fn to_egui_color(color: SharedColor) -> egui::Color32 {
    // CORRECT: 'unmultiplied' forces Egui to multiply Alpha for us.
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

/// Convert egui::Color32 (Premultiplied) back to StateColor32 (Straight Alpha)
pub fn from_egui_color(c: egui::Color32) -> StateColor32 {
    let alpha = c.a() as f32 / 255.0;

    // Safety check for divide-by-zero (fully transparent)
    if alpha <= 0.001 {
        return StateColor32 { r: 0, g: 0, b: 0, a: 0 };
    }

    // CORRECT: We UN-MULTIPLY the rgb values to restore "Straight" color.
    // We add 0.5 to round to nearest integer (matches f32::round behavior).
    StateColor32 {
        r: ((c.r() as f32 / alpha) + 0.5).min(255.0) as u8,
        g: ((c.g() as f32 / alpha) + 0.5).min(255.0) as u8,
        b: ((c.b() as f32 / alpha) + 0.5).min(255.0) as u8,
        a: c.a(),
    }
}

// == Helper Functions ==

pub fn db_to_px(db: f32, noise_floor: f32, max_height: f32) -> f32 {
    let range = (0.0 - noise_floor).max(1.0);
    let normalized = ((db - noise_floor) / range).clamp(0.0, 1.0);
    normalized * max_height
}

pub fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    egui::Color32::from_rgba_premultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
        (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t) as u8,
    )
}

/// Retro VU meter coloring: 3 discrete color zones instead of a smooth gradient.
///
/// Mimics classic hardware spectrum analyzers with distinct color bands:
/// - 0–70%:  `low` color  (normal operating level)
/// - 70–90%: `high` color (warning / elevated level)
/// - 90–100%: `peak` color (danger / clipping zone)
pub fn retro_color(low: egui::Color32, high: egui::Color32, peak: egui::Color32, t: f32) -> egui::Color32 {
    if t < 0.7 { low }
    else if t < 0.9 { high }
    else { peak }
}

/// Choose the appropriate bar color based on the profile's `vu_coloring` setting.
///
/// - Retro mode: 3 discrete color zones via [`retro_color`].
/// - Gradient mode: smooth linear interpolation via [`lerp_color`].
pub fn bar_color(
    low: egui::Color32,
    high: egui::Color32,
    peak: egui::Color32,
    t: f32,
    vu_coloring: crate::shared_state::VuColoring,
) -> egui::Color32 {
    match vu_coloring {
        crate::shared_state::VuColoring::Retro => retro_color(low, high, peak, t),
        crate::shared_state::VuColoring::Gradient => lerp_color(low, high, t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retro_color_low_zone() {
        let low = Color32::from_rgb(0, 255, 0);
        let high = Color32::from_rgb(255, 255, 0);
        let peak = Color32::from_rgb(255, 0, 0);

        assert_eq!(retro_color(low, high, peak, 0.0), low);
        assert_eq!(retro_color(low, high, peak, 0.35), low);
        assert_eq!(retro_color(low, high, peak, 0.69), low);
    }

    #[test]
    fn test_retro_color_high_zone() {
        let low = Color32::from_rgb(0, 255, 0);
        let high = Color32::from_rgb(255, 255, 0);
        let peak = Color32::from_rgb(255, 0, 0);

        assert_eq!(retro_color(low, high, peak, 0.7), high);
        assert_eq!(retro_color(low, high, peak, 0.8), high);
        assert_eq!(retro_color(low, high, peak, 0.89), high);
    }

    #[test]
    fn test_retro_color_peak_zone() {
        let low = Color32::from_rgb(0, 255, 0);
        let high = Color32::from_rgb(255, 255, 0);
        let peak = Color32::from_rgb(255, 0, 0);

        assert_eq!(retro_color(low, high, peak, 0.9), peak);
        assert_eq!(retro_color(low, high, peak, 0.95), peak);
        assert_eq!(retro_color(low, high, peak, 1.0), peak);
    }

    #[test]
    fn test_bar_color_retro_dispatches_to_retro() {
        use crate::shared_state::VuColoring;
        let low = Color32::from_rgb(0, 255, 0);
        let high = Color32::from_rgb(255, 255, 0);
        let peak = Color32::from_rgb(255, 0, 0);

        assert_eq!(bar_color(low, high, peak, 0.5, VuColoring::Retro), low);
    }

    #[test]
    fn test_bar_color_gradient_dispatches_to_lerp() {
        use crate::shared_state::VuColoring;
        let low = Color32::from_rgb(0, 0, 0);
        let high = Color32::from_rgb(255, 255, 255);
        let peak = Color32::from_rgb(255, 0, 0);

        let result = bar_color(low, high, peak, 0.5, VuColoring::Gradient);
        assert_ne!(result, low);
        assert_ne!(result, high);
    }

    #[test]
    fn test_lerp_color_endpoints() {
        let a = Color32::from_rgb(0, 0, 0);
        let b = Color32::from_rgb(255, 255, 255);

        assert_eq!(lerp_color(a, b, 0.0).r(), 0);
        assert_eq!(lerp_color(a, b, 1.0).r(), 255);
    }
}

pub fn to_egui_font(font_variant: &ThemeFont) -> FontId {
    match font_variant {
        ThemeFont::Mini => FontId::new(9.0, FontFamily::Proportional),
        ThemeFont::Small => FontId::new(11.0, FontFamily::Proportional),
        ThemeFont::Medium => FontId::new(14.0, FontFamily::Proportional),
        ThemeFont::Large => FontId::new(18.0, FontFamily::Proportional),
        ThemeFont::Monospace => FontId::new(12.0, FontFamily::Monospace),
    }
}