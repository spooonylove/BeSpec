// src/gui/mod.rs
pub mod theme;
pub mod visualizers;
pub mod decorations;
pub mod widgets;

use crate::gui::theme::*;
use crate::gui::visualizers as viz; // Alias for cleaner calls

use crossbeam_channel::Receiver;
use eframe:: egui;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::media::{PlatformMedia};
use crate::shared_state::{Color32 as StateColor32, SharedState};

use crate::gui::widgets::{SaveTarget, SettingsTab};



// Main Application GUI - handles rendering and user interaction
pub struct SpectrumApp {
    /// shared state between FFT and GUI threads
    shared_state: Arc<Mutex<SharedState>>,

    /// Receiver for media updates (local to GUI thread)
    media_rx: Receiver<crate::media::MediaTrackInfo>,

    /// Controller access
    media_controller: Arc<PlatformMedia>,

    /// cached album art texture
    album_art_texture: Option<egui::TextureHandle>,
    
    /// Opacity for entire media overlay
    media_opacity: f32,

    /// Last time user hovered the media overlay or window
    last_media_interaction: Option<Instant>,

    /// Settings window state
    settings_open: bool,

    /// Current active settings tab
    active_tab: SettingsTab,

    /// Performance tracking
    last_frame_time :  Instant, 
    frame_times: Vec<f32>,

    /// Track window size to only log changes
    last_window_size: Option<egui::Vec2>,
    last_window_pos: Option<egui::Pos2>,
    last_passthrough_state: bool,

    // Sonar Ping State
    was_focused: bool,
    flash_start: Option<Instant>,

    // User Preset UI State
    save_target: SaveTarget,
    new_preset_name: String,
}

impl SpectrumApp {
    pub fn new(
        shared_state: Arc<Mutex<SharedState>>,
        media_rx: Receiver<crate::media::MediaTrackInfo>,
        media_controller: Arc<PlatformMedia>,
    ) -> Self {
        Self {
            shared_state,
            media_rx,
            media_controller,
            media_opacity: 0.0,
            last_media_interaction: None,
            album_art_texture: None,
            settings_open: false,
            active_tab: SettingsTab::Visual,
            last_frame_time: Instant::now(),
            frame_times: Vec::with_capacity(60),
            last_window_size: None,
            last_window_pos: None,
            last_passthrough_state: false,
            was_focused: true,
            flash_start: Some(Instant::now()),
            save_target: SaveTarget::None,
            new_preset_name: String::new(),
        }
    }
}

impl eframe::App for SpectrumApp {

    // This is called by eframe periodicatlly and/or on exit
    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        // On exit, save the current config to disk
        if let Ok(state) = self.shared_state.lock() {
            state.config.save();
        }
    }
    
    /// Tell eframe to clear the window with total transparency
    /// this alllows the OS background to show through when our CentralPanel
    /// is also transparent.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Return RGBA array directly [Red, Green, Blue, Alpha]
        [0.0, 0.0, 0.0, 0.0] 
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        
        let minimize_key = self.shared_state.lock().unwrap().config.minimize_key;
        let shortcut = egui::KeyboardShortcut::new(egui::Modifiers::CTRL, minimize_key);

        if ctx.input_mut(|i| i.consume_shortcut(&shortcut)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }

        // --- Poll for Media Updates ---
        let mut new_track = None;
        while let Ok(info) = self.media_rx.try_recv() {
            new_track = Some(info);
        }

        if let Some(track) = new_track {
            if let Ok(mut state) = self.shared_state.lock() {
                state.media_info = Some(track.clone());
                state.last_media_update = Some(Instant::now());
            }

            // Process album art
            if let Some(bytes) = &track.album_art {
                if let Ok(image) = image::load_from_memory(bytes) {
                    let size = [image.width() as _, image.height() as _];
                    let image_buffer = image.into_rgba8();
                    let pixels = image_buffer.as_flat_samples();
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        size,
                        pixels.as_slice(),
                    );

                    // load into GPU
                    self.album_art_texture  = Some(ctx.load_texture(
                        "album_art", 
                        color_image,
                        egui::TextureOptions::LINEAR,
                    ));
                } else {
                    self.album_art_texture = None;
                }
            } else {
                self.album_art_texture = None;
            }
        }
        

        // --- Main Window Position tracking ---
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            let current_pos = rect.min;

            if self.last_window_pos != Some(current_pos) {
                // Determine if we should log (don't log first detection to avoid spam on startup)
                if self.last_window_pos.is_some() {
                    tracing::debug!("[GUI] Main Window Moved: x: {:.0}, y: {:.0}", current_pos.x, current_pos.y);
                }
                
                self.last_window_pos = Some(current_pos);

                // Save to config
                if let Ok(mut state) = self.shared_state.lock() {
                    state.config.window_position = Some([current_pos.x, current_pos.y]);
                }
            }
        }

        // --- Window Size tracking (Keep separate to avoid growth bug) ---
         if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
             let size = rect.size();
             let size_changed = self.last_window_size.map_or(true, |ls| (ls - size).length() > 1.0);
             if size_changed {
                 if let Ok(mut state) = self.shared_state.lock() {
                     state.config.window_size = [size.x, size.y];
                 }
                 self.last_window_size = Some(size);
             }
         }
        
        // === Performance Stats (FPS) ===
        // Calculate FPS
        let now = Instant::now();
        let frame_time = now.duration_since(self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;
        
        // Rolling buffer of frame times. push a new one in, pop the oldest.
        self.frame_times.push(frame_time);
        if self.frame_times.len() > 60 {
            self.frame_times.remove(0);
        }

        let avg_frame_time = self.frame_times.iter().sum::<f32>() / self.frame_times.len() as f32;
        let fps = 1.0 / avg_frame_time;

        // Update the FPS in shared state
        if let Ok(mut state) = self.shared_state.lock() {
            state.performance.gui_fps = fps;
         }

        // Request continuous repainting for smooth animation
        ctx.request_repaint();

        // === Main Window ===

        // === Sonar Ping ===
        let is_focused = ctx.input(|i| i.focused);
        if is_focused && !self.was_focused {
            self.flash_start = Some(Instant::now());
            self.last_media_interaction = Some(Instant::now());
        }
        self.was_focused = is_focused;
        
        let mut flash_strength = 0.0;
        if let Some(start) = self.flash_start {
            let elapsed = start.elapsed().as_secs_f32();
            if elapsed < 2.0 {
                flash_strength = (1.0 - (elapsed / 2.0)).powi(3);
                ctx.request_repaint();
            } else {
                self.flash_start = None;
            }
        }

        // Use Profile Background Color
        let (window_fill, content_fill, window_locked, background_alpha) = if let Ok(state) = self.shared_state.lock() {
            let colors = state.config.resolve_colors(&state.user_color_presets);
            let bg = to_egui_color(colors.background);
            let base_alpha = bg.a() as f32 / 255.0;
            
            // Apply flash
            let final_alpha = (base_alpha + (flash_strength * 0.2)).min(1.0);
            
            // Reconstruct the user's desired background color
            let user_bg_color = egui::Color32::from_rgba_premultiplied(
                bg.r(), bg.g(), bg.b(), (final_alpha * 255.0) as u8
            );

            // LOGIC CHANGE FOR BEOS MODE:
            // If BeOS mode is active, the "Window" (CentralPanel) must be TRANSPARENT 
            // so the area around the tab is clear. We will paint the 'user_bg_color' 
            // manually inside the decorations module.
            if state.config.profile.beos_enabled {
                (egui::Color32::TRANSPARENT, user_bg_color, state.config.window_locked, final_alpha)
            } else {
                (user_bg_color, user_bg_color, state.config.window_locked, final_alpha)
            }
        } else {
            (egui::Color32::BLACK, egui::Color32::BLACK, false, 1.0) 
        };

        // === 3. Ghost Mode Logic === (Focus-to-Wake) ===
        // Determines if the window should ignore mouse events (click-through).
        // We only enable passthrough if ALL conditions are met:
        // 1. window_locked: User enabled "Ghost Mode".
        // 2. is_transparent: Background is invisible (avoid confusion of clicking through solid pixels).
        // 3. !is_focused: The window is NOT currently active.
        //    CRITICAL: This allows "Alt-Tab to Wake". If the user Alt-Tabs to this window,
        //    it gains focus, passthrough turns OFF, and the user can click the unlock button.
        let is_transparent = background_alpha <= 0.05; // Threshold for "invisible"
        let should_passthrough = window_locked && is_transparent && !is_focused;

        // Only send command if state changed (prevents spamming the OS Window manager)
        if should_passthrough != self.last_passthrough_state {
            let status = if should_passthrough { "GHOST MODE" } else { "INTERACTIVE" };
            tracing::info!("[GUI] Window State: {}", status);

            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(should_passthrough));
            self.last_passthrough_state = should_passthrough;
        }

        // === 4. Render Window ===
        // This is the main draw call for the application window.
        // We use a CentralPanel which fills the entire OS window.
        //
        // Rendering Order (Painter's Algorithm - items drawn later appear on top):
        // 1. Background (via custom_frame): Clears window with theme color/transparency.
        // 2. Visualizer: Spectrum bars or oscilloscope (bottom layer).
        // 3. Sonar Ping: Visual flash effect when window gains focus.
        // 4. Media Overlay: "Now Playing" info (drawn in a floating Area, so technically separate Z-layer).
        // 5. Window Controls: Resize grips, lock button, and drag logic (top interaction layer).
        
        let custom_frame = egui::Frame::central_panel(&ctx.style())
            .fill(window_fill)
            .inner_margin(0.0);

        egui::CentralPanel::default().frame(custom_frame).show(ctx, |ui| {
                // === Layout & Interaction Setup ===
                // Setup the basic window rects
                let window_rect  = ui.available_rect_before_wrap();
                
                // If Enabled, draw BeOS Decorations & calculate layout
                // We need to lock the state mutably briefly to update dragging offsets
                let chrome_layout = {
                    let mut state= self.shared_state.lock().unwrap();
                    let layout = crate::gui::decorations::draw_beos_window_frame(
                        ui,
                        ctx,
                        window_rect,
                        &mut state.config,
                        content_fill
                    );
                    layout
                };
                
                let viz_rect =  chrome_layout.content_rect;
                
                // Handle BeOS Tab Right-Click (Toggle Settings)
                // Since the tab captures mouse events, we check its specific response 
                // to see if the user right-clicked it.
                if let Some(tab_resp) = &chrome_layout.tab_response {
                    if tab_resp.clicked_by(egui::PointerButton::Secondary) {
                        self.settings_open = !self.settings_open;
                    }
                }

                // Handle Dragging
                if !chrome_layout.is_collapsed {
                    widgets::handle_window_interaction(ui, ctx, viz_rect, &mut self.settings_open);
                }
                
                // === Orchestration Setup: Calculate Opacity
                // Briefly lock to get the config/timepstamps for logic
                {
                    // 1. Clone the ARC
                    let state_arc = self.shared_state.clone();

                    // 2. Lock the Clone. Now 'self' isn't borrowed
                    let state = state_arc.lock().unwrap();

                    // 3. Safely call a mutable method on self                    
                    self.calculate_media_opacity(ui, &state);
                } // Lock drops here, self.media_opacity is now updated for this frame

                //=== Update Notification Setup ===
                let mut dismissed_click = false;
                let mut update_url_copy: Option<String> = None;
                let mut show_banner = false;

                // Quick check! (small scope lock)
                if let Ok(state) = self.shared_state.lock(){
                    if state.update_url.is_some() && !state.update_dismissed {
                        show_banner = true;
                        update_url_copy = state.update_url.clone();
                    }
                }

                //Scope management for State Lock!!!
                {
                    // Visualization (requres Read-only Lock)
                    let state = self.shared_state.lock().unwrap(); //lock once!
                    let mut final_viz_rect = viz_rect;

                    // ======= Update Notification Banner =========
                    if show_banner{

                        // Define the banner area (top 25px of the content area)
                        let banner_height = 28.0;
                        let split = final_viz_rect.split_top_bottom_at_y(final_viz_rect.top() + banner_height);
                        let banner_rect = split.0;
                        final_viz_rect = split.1;

                        // Draw the Banner
                        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(banner_rect), |ui| {
                            // "Info Blue" background
                            ui.painter().rect_filled(banner_rect, 0.0, egui::Color32::from_rgb(0, 90, 160));
                            // Add a subtle bottom border for contrast
                            ui.painter().line_segment(
                                [banner_rect.left_bottom(), banner_rect.right_bottom()], 
                                egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 120, 200))
                            );

                            ui.horizontal_centered(|ui|{
                                ui.spacing_mut().item_spacing.x = 10.0;
                                ui.label(egui::RichText::new("🚀 New version available!").color(egui::Color32::WHITE).strong());

                                if ui.button("Download").clicked() {
                                    if let Some(url) = &update_url_copy {
                                        let _ = open::that(url);
                                    }
                                }

                                //Push to right
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui|{
                                    ui.add_space(8.0);
                                    // the X button
                                    if ui.add(egui::Button::new(egui::RichText::new(" 🗙 ").color(egui::Color32::WHITE).strong()).frame(false))
                                        .on_hover_text("Dismiss")
                                        .clicked()
                                    {
                                            dismissed_click = true;
                                    }
                                
                                });
                            });
                        });
                    }
                    //=============================================
                    
                    let viz_data = &state.visualization;

                    let perf = &state.performance;
                    let media_info = state.media_info.as_ref();
                    let colors = state.config.resolve_colors(&state.user_color_presets);

                    // === Render Visualization ===
                    viz::draw_main_visualizer(
                        ui.painter(),
                        final_viz_rect,
                        viz_data,
                        &state.config,
                        &colors,
                        perf,
                        ui.input(|i| i.pointer.hover_pos()),
                    );

                    // Sonar Ping Effect
                    if flash_strength > 0.0 {
                      

                        viz::draw_sonar_ping(ui, final_viz_rect.shrink(5.0), flash_strength, &colors);
                    }
                    
                    // Media Overlay
                    if self.media_opacity > 0.01 {
                        if let Some(info) = media_info{
                            viz::draw_media_overlay(
                                ui,
                                final_viz_rect,
                                Some(info),
                                state.config.media_display_mode,
                                &state.config.profile.overlay_font,
                                self.media_opacity,
                                &colors,
                                self.album_art_texture.as_ref(),
                                self.media_controller.as_ref(),
                            );
                        }
                    }
                }//State Lock Drops Here!

                // We manage the dismissal click out of the state lock block above, due to limited access
                // to the shared state. Hence, double bools!
                if dismissed_click {
                    if let Ok(mut state) = self.shared_state.lock() {
                        state.update_dismissed = true;
                    }
                }

                // === WINDOW CONTROLS ====
                // 1. Resize Grip (Needs Context + Window Rect)
                // We check the inverted state first (read-only lock)
                let is_inverted = if let Ok(s) = self.shared_state.lock() {
                    s.config.profile.inverted_spectrum
                } else {
                    false
                };
                widgets::draw_resize_grip(ui, ctx, window_rect, is_inverted);

                // 2. Lock Button (needs mutable State Access)
                // We pass the Arc<Mutex> so the widget can lock it internally when clicked
                widgets::draw_lock_button(ui, window_rect, &self.shared_state, &mut self.last_media_interaction, is_focused);

            });
        
        
        
        
        //  === SETTINGS WINDOW (Separate Viewport) ===
        if self.settings_open {
            let mut state = self.shared_state.lock().unwrap();

            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("settings_viewport"),
                egui::ViewportBuilder::default()
                    .with_title("BeSpec Settings")
                    .with_inner_size([450.0, 500.0])
                    .with_resizable(false)
                    .with_maximize_button(false),
                |ctx, _class| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        // Handle closing the viewport  via the OS "X" button
                        if ctx.input(|i| i.viewport().close_requested()) {
                            self.settings_open = false;
                        }

                        crate::gui::widgets::show_settings_window(
                            ui,
                            &mut state,
                            &mut self.active_tab,
                            &mut self.save_target,
                            &mut self.new_preset_name
                        );
                    });
                }
            );
        }
    }
}

impl SpectrumApp {

    /// Logic to determine if the media overlay should be visible
    /// Updates 'last_media_interaction' if the user hovers the mouse
    fn calculate_media_opacity(&mut self, ui: &egui::Ui, state: &SharedState) {
        let mode = state.config.media_display_mode;

        // 1. Target determiniation (Should we be visible?)
        let should_be_visible = match mode {
            crate::shared_state::MediaDisplayMode::Off => false,
            crate::shared_state::MediaDisplayMode::AlwaysOn => true,
            crate::shared_state::MediaDisplayMode::FadeOnUpdate => {
                let now = Instant::now();
                let hold_time = state.config.media_fade_duration_sec;
                let mut active = false;

                // A. Check Track Update Activity
                if let Some(last_update) = state.last_media_update {
                    if now.duration_since(last_update).as_secs_f32() < hold_time{
                        active = true;
                    }
                }

                // B. Check Historic Interaction (Hovering previously)
                if let Some(last_interact) = self.last_media_interaction {
                    if now.duration_since(last_interact).as_secs_f32() < hold_time {
                        active = true;
                    }
                }

                // C. Check Current Hover (Global Window check)
                // if user moves mouse, keep it alive
                if ui.input(|i| i.pointer.hover_pos().is_some()) {
                    self. last_media_interaction = Some(now);
                    active = true;
                }

                active
            }
        };

        // 2. Animation (Lerp towards target)
        let target = if should_be_visible { 1.0 } else { 0.0 };
        let dt = ui.input(|i| i.stable_dt).min(0.1);

        // Fast fade in (6.0), slow fade out (1.0)
        let speed = if target >  self.media_opacity { 6.0 } else { 1.0 };

        self.media_opacity += (target - self.media_opacity) *  speed * dt;
        self.media_opacity = self.media_opacity.clamp(0.0, 1.0);

        // Request repaint if we are animating
        if self.media_opacity > 0.001 &&  self. media_opacity < 0.999 {
            ui.ctx().request_repaint();
        }
    }

}