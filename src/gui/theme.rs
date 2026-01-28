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

/// Converts EGUI colors to our internal Color32 type
pub fn from_egui_color(c: egui::Color32) -> StateColor32 {
    StateColor32 { r: c.r(), g: c.g(), b: c.b(), a: c.a() }
}