use egui::{Color32, FontId, FontFamily};
use crate::shared_state::{Color32 as SharedColor, ThemeFont};

// === BeOS / Haiku Design Tokens ====
pub const BEOS_YELLOW: Color32 = Color32::from_rgb(255, 203, 0);
pub const BEOS_FRAME_LIGHT: Color32 = Color32::from_rgb(255, 255,255);
pub const BEOS_FRAME_MID: Color32 = Color32::from_rgb(216, 216, 216);
pub const BEOS_FRAME_DARK: Color32 = Color32::from_rgb(150, 150, 150);

pub const BEOS_TAB_HEIGHT: f32 = 20.0;
pub const BEOS_BORDER_WIDTH: f32 = 2.0;

// === Global UI Constants ===
pub const PANEL_WIDTH: f32 = 250.0;
pub const ANIMATION_SPEED: f32 = 0.1;

/// Convert our Color32 to egui::Color32
pub fn to_egui_color(color: SharedColor) -> egui::Color32 {
    egui::Color32::from_rgba_premultiplied(color.r, color.g, color.b, color.a)
}

// == Helper Functions ==

pub fn db_to_px(db: f32, noise_floor: f32, max_height: f32) -> f32 {
    let range = (0.0 - noise_floor).max(1.0);
    let normalized = ((db - noise_floor) / range).clamp(0.0, 1.0);
    normalized * max_height
}

/// Linear interpolation between two egui colors
pub fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    egui::Color32::from_rgba_premultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
        (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t) as u8,
    )
}

/// Converts our internal ThemeFont setting into a usable egui FontID
pub fn to_egui_font(font_variant: &ThemeFont) -> FontId {
    match font_variant {
        ThemeFont::Mini => FontId::new(9.0, FontFamily::Proportional),
        ThemeFont::Small => FontId::new(11.0, FontFamily::Proportional),
        ThemeFont::Medium => FontId::new(14.0, FontFamily::Proportional),
        ThemeFont::Large => FontId::new(18.0, FontFamily::Proportional),
        ThemeFont::Monospace => FontId::new(12.0, FontFamily::Monospace),
    }
}