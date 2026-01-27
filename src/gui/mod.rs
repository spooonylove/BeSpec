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

use crate::fft_config::FIXED_FFT_SIZE;
use crate::media::{PlatformMedia, MediaController};
use crate::shared_state::{Color32 as StateColor32, ColorProfile, MediaDisplayMode, SharedState, VisualMode, VisualProfile};
use crate::shared_state::ColorRef;

#[derive(PartialEq, Debug)]
enum SettingsTab {
    Visual, 
    Audio,
    Colors,
    Window,
    Performance,
}

#[derive(PartialEq)]
enum SaveTarget {
    None,
    Visual,
    Color,
}

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
        let (bg_color_egui, window_locked, background_alpha) = if let Ok(state) = self.shared_state.lock() {
            let colors = state.config.resolve_colors(&state.user_color_presets);
            let bg = to_egui_color(colors.background);
            let base_alpha = bg.a() as f32 / 255.0;
            
            // Apply flash
            let final_alpha = (base_alpha + (flash_strength * 0.2)).min(1.0);
            
            // Reconstruct with new alpha
            let final_bg = egui::Color32::from_rgba_premultiplied(
                bg.r(), bg.g(), bg.b(), (final_alpha * 255.0) as u8
            );
            
            (final_bg, state.config.window_locked, final_alpha)
        } else {
            (egui::Color32::BLACK, false, 1.0) 
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
        
        let custom_frame = egui::Frame::central_panel(&ctx.style()).fill(bg_color_egui).inner_margin(1.0);
        egui::CentralPanel::default().frame(custom_frame).show(ctx, |ui| {
                // === Layout & Interaction Setup ===
                // Setup the basic window rects
                let window_rect  = ui.available_rect_before_wrap();
                let viz_rect = window_rect.shrink(1.0);



                // Handle Dragging
                //self.handle_window_drag(ctx, ui, window_rect);
                crate::gui::widgets::handle_window_interaction(ui, ctx, window_rect,&mut self.settings_open);
                
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

                //Scope management for State Lock!!!
                {

                    // Visualization (requres Read-only Lock)
                    let state = self.shared_state.lock().unwrap(); //lock once!


                    
                    let viz_data = &state.visualization;

                    let perf = &state.performance;
                    let media_info = state.media_info.as_ref();
                    let colors = state.config.resolve_colors(&state.user_color_presets);

                    // === Render Visualization ===
                    crate::gui::visualizers::draw_main_visualizer(
                        ui.painter(),
                        viz_rect,
                        viz_data,
                        &state.config,
                        &colors,
                        perf,
                        ui.input(|i| i.pointer.hover_pos()),
                    );

                    // Sonar Ping Effect
                    if flash_strength > 0.0 {
                        self.draw_sonar_ping(ui, ui.max_rect().shrink(5.0), flash_strength, &colors);
                    }
                    
                    // Media Overlay
                    if self.media_opacity > 0.01 {
                        if let Some(info) = media_info{
                            crate::gui::visualizers::draw_media_overlay(
                                ui,
                                viz_rect,
                                Some(info),
                                state.config.media_display_mode,
                                &state.config.profile.overlay_font,
                                self.media_opacity,
                                &colors,
                                self.album_art_texture.as_ref(),
                            );
                        }
                    }
                }


                // Render the windows controls (resize grips, lock button, context menu)
                self.draw_window_controls(ctx, ui, is_focused, window_rect);

            });
        
        
        
        
        //  === SETTINGS WINDOW (Separate Viewport) ===
        if self.settings_open {
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

                        self.render_settings_window(ui);
                    });
                }
            );
        }
    }
}

impl SpectrumApp {

    /// Handle window movement and context menu (The "floor" logic)
    fn handle_window_drag(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, rect: egui::Rect) {
        // 1. Handle Window Movement (Dragging the background)
        // CHANGE: Switch from click_and_drag() to click().
        //
        // EXPLANATION:
        // - click_and_drag() makes egui overly sensitive to mouse movement. If you right-click
        //   and move 1 pixel, it counts as a "drag" and suppresses the "click" event, preventing
        //   the context menu from opening.
        // - click() fixes the context menu.
        // - Window Dragging still works because we trigger StartDrag manually via
        //   pointer.button_pressed() below, which doesn't depend on egui's high-level drag state.
        let interaction = ui.interact(rect, ui.id().with("window_drag"), 
            egui::Sense::click());

        // Dragging moves the window
        // Use button_pressed() for instant, single-fire trigger
        if interaction.hovered() && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        
        // Double-clicking toggles maximize
        if interaction.double_clicked() {
            let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
        }

        // Right-Click opens the Settings Menu
        interaction.context_menu(|ui| {
   
            if ui.button("⚙ Settings").clicked() {
                self.settings_open = true;

                // Force the settings window to the front
                ctx.send_viewport_cmd_to(
                    egui::ViewportId::from_hash_of("settings_viewport"),
                    egui::ViewportCommand::Focus,
                );

                ui.close_menu();
            }

            ui.separator();

            if ui.button("❌ Exit").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

    }


    /// Draw invisible resize handles, handle window moverment, and context menu
    fn draw_window_controls(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, is_focused: bool, rect: egui::Rect) {

        // Resize Grip (Bottom Right Corner)
        self.draw_resize_grip(ctx, ui, &rect);

        // Lock Button (Bottom Left Corner)
        self.draw_lock_button(ui, rect, is_focused);
    }
    
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



    /// Helper to draw vector buttons (ISO 60417 standard geometry)
    fn render_transport_controls(&self, ui: &mut egui::Ui, is_playing: bool, opacity: f32, base_color: egui::Color32) {
        let btn_size = egui::vec2(28.0, 28.0); 
        let color = base_color.linear_multiply(opacity);
        // background highlight on hover
        let hover_bg = base_color.linear_multiply(0.15 * opacity);

        // Use Right-to-Left to anchor to the right side
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            // === 3. NEXT (ISO 60417-5862) ===
            // Drawn FIRST so it appears on the Far Right
            let (rect, resp) = ui.allocate_exact_size(btn_size, egui::Sense::click());
            if resp.hovered() { ui.painter().rect_filled(rect.expand(2.0), 4.0, hover_bg); }
            if resp.clicked() { self.media_controller.try_next(); }

            if ui.is_rect_visible(rect) {
                let painter = ui.painter();
                let c = rect.center();
                let w = 12.0;
                let h = 12.0;
                let bar_w = 2.0;

                // Right Bar
                let bar_rect = egui::Rect::from_min_size(
                    egui::pos2(c.x + (w / 2.0) - bar_w, c.y - (h / 2.0)), 
                    egui::vec2(bar_w, h)
                );
                painter.rect_filled(bar_rect, 0.5, color);

                // Right Triangle
                let tip = egui::pos2(c.x + (w / 2.0) - bar_w - 1.0, c.y);
                let base_x = c.x - (w / 2.0);

                painter.add(egui::Shape::convex_polygon(
                    vec![
                        tip,
                        egui::pos2(base_x, c.y - (h / 2.0)),
                        egui::pos2(base_x, c.y + (h / 2.0)),
                    ],
                    color,
                    egui::Stroke::NONE
                ));
            }

            // === 2. PLAY / PAUSE (ISO 60417-5857 / 5858) ===
            // Drawn SECOND so it appears to the LEFT of Next
            let (rect, resp) = ui.allocate_exact_size(btn_size, egui::Sense::click());
            if resp.hovered() { ui.painter().rect_filled(rect.expand(2.0), 4.0, hover_bg);}
            if resp.clicked() { self.media_controller.try_play_pause(); }

            if ui.is_rect_visible(rect) {
                let painter= ui.painter();
                let c = rect.center();
                let h = 14.0; 

                if is_playing {
                    // PAUSE 
                    let bar_w = 4.0;
                    let gap = 3.0;

                    painter.rect_filled(
                        egui::Rect::from_min_size(egui::pos2(c.x - gap/2.0 - bar_w, c.y - h/2.0), egui::vec2(bar_w, h)),
                        1.0, color
                    );
                    painter.rect_filled(
                        egui::Rect::from_min_size(egui::pos2(c.x + gap/2.0, c.y - h/2.0), egui::vec2(bar_w, h)), 
                        1.0, color
                    );
                } else {
                    // PLAY
                    let optical_offset = 1.5; 
                    let tri_h = 14.0;
                    let tri_w = 12.0;

                    let tip = egui::pos2(c.x + (tri_w / 2.0) + optical_offset, c.y);
                    let base_x = c.x - (tri_w / 2.0) + optical_offset;

                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            tip,
                            egui::pos2(base_x, c.y - (tri_h / 2.0)),
                            egui::pos2(base_x, c.y + (tri_h / 2.0)),
                        ],
                        color,
                        egui::Stroke::NONE
                    ));
                }
            }

            // === 1. PREVIOUS (ISO 60417-5861) ===
            // Drawn LAST so it appears to the LEFT of Play
            let (rect, resp) = ui.allocate_exact_size(btn_size, egui::Sense::click());
            if resp.hovered() { ui.painter().rect_filled(rect.expand(2.0), 4.0, hover_bg);}
            if resp.clicked() { self.media_controller.try_prev(); }

            if ui.is_rect_visible(rect) {
                let painter = ui.painter();
                let c = rect.center();
                let w = 12.0;
                let h = 12.0;
                let bar_w = 2.0;

                // left bar
                let bar_rect = egui::Rect::from_min_size(
                    egui::pos2(c.x - (w / 2.0), c.y - (h / 2.0)),
                    egui::vec2(bar_w, h)
                );
                painter.rect_filled(bar_rect, 0.5, color);

                // left triangle
                let tip = egui::pos2(c.x  - (w / 2.0) + bar_w + 1.0, c.y);
                let base_x = c.x + (w / 2.0);

                painter.add(egui::Shape::convex_polygon(
                    vec![
                        tip,
                        egui::pos2(base_x, c.y - (h / 2.0)),
                        egui::pos2(base_x, c.y + (h / 2.0)),
                    ],
                    color,
                    egui::Stroke::NONE
                ));
            }
        });
    }

    // === OVERLAYS ===

    fn draw_resize_grip(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, rect: &egui::Rect) {
        // 1. Check Inverted State
        let is_inverted = if let Ok(state) = self.shared_state.lock() {
            state.config.profile.inverted_spectrum
        } else {
            false
        };

        let corner_size = 20.0;
        
        // 2. Calculate Origin (Top-Right if inverted, Bottom-Right if normal)
        let grip_origin = if is_inverted {
            egui::pos2(rect.right() - corner_size, rect.top())
        } else {
            egui::pos2(rect.right() - corner_size, rect.bottom() - corner_size)
        };

        let grip_rect = egui::Rect::from_min_size(
            grip_origin,
            egui::Vec2::splat(corner_size)
        );

        let response = ui.interact(grip_rect, ui.id().with("resize_grip"), egui::Sense::drag());

        // 3. Set Cursor & Direction based on mode
        let (cursor, direction) = if is_inverted {
            (egui::CursorIcon::ResizeNorthEast, egui::ResizeDirection::NorthEast)
        } else {
            (egui::CursorIcon::ResizeSouthEast, egui::ResizeDirection::SouthEast)
        };

        if response.hovered() {
            ctx.set_cursor_icon(cursor);
        }

        // Use button_pressed() for instant resize start
        if response.hovered() && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }

        // 4. Draw the Grip Lines
        if ui.is_rect_visible(grip_rect) {
            let painter = ui.painter();
            let stroke = egui::Stroke::new(2.0, egui::Color32::from_white_alpha(50));
            
            for i in 0..4 {
                let offset = i as f32 * 4.0;

                // Calcluate line start/end points based on corner
                let (p1, p2) = if is_inverted {
                    // Top-Right Corner Geometry
                    (
                        egui::pos2(rect.right() - 4.0 - offset, rect.top() + 4.0),
                        egui::pos2(rect.right() - 4.0, rect.top() + 4.0 + offset),
                    )
                } else {
                    // Bottom-Right Corner Geometry
                    (
                        egui::pos2(rect.right() - 4.0 - offset, rect.bottom() - 4.0),
                        egui::pos2(rect.right() - 4.0, rect.bottom() - 4.0 - offset),
                    )
                };
                painter.line_segment([p1, p2], stroke);
            }
        }
    }

    fn draw_lock_button(&mut self, ui: &mut egui::Ui, rect: egui::Rect, is_focused: bool) {
        // we need mutable access to toggle the state
        let mut state = match self.shared_state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };

        // Use resolved background alpha
        let colors = state.config.resolve_colors(&state.user_color_presets);
        let bg_alpha = colors.background.a as f32 / 255.0;

        // only show if background is transparent
        if bg_alpha >= 0.05 { return;}

        // 1. Set up geometry and state
        let is_locked = state.config.window_locked;
        let is_inverted = state.config.profile.inverted_spectrum;
        let size = 24.0;
        let padding = 8.0;

        // 2. Calculate Y position based on mode
        let y_pos = if is_inverted {
            rect.top() + padding
        } else {
            rect.bottom() - size - padding
        };

        // 3. Create Rect (Top-left or Bottom-left)
        let lock_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + padding, y_pos),
            egui::Vec2::splat(size)
        );

        // Handle Click
        let response = ui.interact(lock_rect, ui.id().with("lock_btn"), 
            egui::Sense::click());
        if response.clicked() {
            state.config.window_locked = !state.config.window_locked;
            self.last_media_interaction = Some(Instant::now());  // wake up on click

        }

        if response.hovered() {
            let text = if is_locked {
                // OS-Agnostic Instructions
                "GHOST MODE ACTIVE\n\n\
                 1. Window is click-through (ignore mouse).\n\
                 2. Switch focus to another app to engage.\n\
                 3. Switch focus back here to unlock."
            } else {
                "ENTER GHOST MODE\n\n\
                 Click to make window click-through.\n\
                 (Must be transparent first)"
            };
            response.clone().on_hover_text(text);
            
            // Wake up on hover
            self.last_media_interaction = Some(Instant::now()); 
        }

        // 4. Calculate Opacity
        let mut opacity = 1.0;

        if is_locked {
            let cooldown = 3.0; // Seconds to wait before fading
            let fade_duration = 1.0;
            let resting_opacity = 0.1; // Dim state

            if let Some(last_interact) = self.last_media_interaction{
                let elapsed = last_interact.elapsed().as_secs_f32();
                let t = ((elapsed - cooldown) / fade_duration).clamp(0.0, 1.0);
                opacity = 1.0 - (t * (1.0 - resting_opacity));

                if t < 1.0{
                    ui.ctx().request_repaint();
                }
            } else {
                // If we've never interacted, default to dim ghost mode
                opacity = resting_opacity;
            }

        }

        // 5. Draw  
        let painter = ui.painter();

        // Color Logic:
        // -- Locked and Focused : Bright Red (wake up!)
        // -- Locked and Unfocused : Dim Red (ghost mode)
        // -- Unlocked : White/ Grey (passive)
        let base_color = if is_locked {
            if is_focused { egui::Color32::from_rgb(255,100,100) }
            else { egui::Color32::from_rgb(200,50,50) }
        } else {
            if response.hovered() { egui::Color32::WHITE } else { egui::Color32::from_white_alpha(50) }
        };

        let color = base_color.linear_multiply(opacity);

        // Draw Body (Main square)
        let body_h = size * 0.6;
        let body_rect = egui::Rect::from_min_max(
            egui::pos2(lock_rect.left(), lock_rect.bottom() - body_h),
            lock_rect.right_bottom()
        );
        painter.rect_filled(body_rect, 4.0, color);

        // Draw Shackle (the Loop)
        let shackle_w = size * 0.6;
        let shackle_h = size * 0.5;

        // If unlocked, shift the schakle up/right to look "open"
        let (shackle_x_off, shackle_y_off) = if is_locked { (0.0, 0.0)} else { (-4.0, -4.0)};

        let shackle_rect = egui::Rect::from_center_size(
            egui::pos2(
                lock_rect.center().x + shackle_x_off,
                body_rect.top() - (shackle_h/2.0) + 4.0 + shackle_y_off
            ), 
            egui::vec2(shackle_w, shackle_h)
        );

        //Draw the arch
        painter.rect_stroke(
            shackle_rect,
            egui::Rounding { nw: 10.0, ne: 10.0, sw: 0.0, se: 0.0},
            egui::Stroke::new(3.0, color)
        );

        // Keyhole detail
        painter.circle_filled(body_rect.center(), 2.5, egui::Color32::BLACK);
    }

    fn draw_sonar_ping(&self, ui: &mut egui::Ui, rect: egui::Rect, strength: f32, colors: &ColorProfile) {
        // 1. Setup
        let base_color = to_egui_color(colors.high);
        
        let rounding = 12.0;

        // 2. Calculate Animation State based on 'strength' (1.0 -> 0.0)
        
        // Alpha: Directly use strength (starts bright, fades to 0)
        // We square it so it stays bright a bit longer then drops off
        let global_alpha = strength.powi(2);
        
        // Expansion: Invert strength so we start at 0 expansion and grow outward
        // Grows up to 12px outward
        let progress = 1.0 - strength; 
        let expansion = 2.0 + (10.0 * progress); 

        let painter = ui.painter();

        // 3. Draw Multi-Pass Glow
        
        // Pass 1: The "Haze" (Wide, Outer, Very Transparent)
        painter.rect_stroke(
            rect.expand(expansion + 4.0), 
            rounding,
            egui::Stroke::new(6.0, base_color.linear_multiply(0.10 * global_alpha))
        );

        // Pass 2: The "Glow" (Medium, Middle, Medium Transparent)
        painter.rect_stroke(
            rect.expand(expansion + 2.0), 
            rounding,
            egui::Stroke::new(3.0, base_color.linear_multiply(0.3 * global_alpha))
        );

        // Pass 3: The "Filament" (Thin, Inner, Bright)
        painter.rect_stroke(
            rect.expand(expansion), 
            rounding,
            egui::Stroke::new(1.0, base_color.linear_multiply(0.8 * global_alpha))
        );
    }   

    fn render_preview_spectrum(&self, ui: &mut egui::Ui, current_colors: &ColorProfile, bar_opacity: f32) {
        ui.label("Preview:");
        let height = 60.0;
        let (response, painter) = ui.allocate_painter(
            egui::vec2 (ui.available_width(), height),
            egui::Sense::hover()
        );
        let rect = response.rect;

        // Draw Background
        let bg_color = to_egui_color(current_colors.background);
        painter.rect_filled(rect, 4.0, bg_color);

        // Mock Data Pattern (bass heavy, dip in mids, sparkle in highs)
        let mock_levels = [
            0.10, 0.40, 0.75, 0.95, 0.90, 0.85, 0.70, // Bass
            0.55, 0.40, 0.30, 0.25,                   // Mids
            0.40, 0.60, 0.50, 0.35,                   // High Mids
            0.25, 0.15, 0.25, 0.40, 0.30, 0.20, 0.15, 0.10, 0.08, 0.04, 0.01 // Highs
        ];

        let low = to_egui_color(current_colors.low).linear_multiply(bar_opacity);
        let high = to_egui_color(current_colors.high).linear_multiply(bar_opacity);
        let peak = to_egui_color(current_colors.peak).linear_multiply(bar_opacity);

        let bar_width = rect.width() / mock_levels.len() as f32;
        let gap = 2.0;

        for (i, &level) in mock_levels.iter().enumerate() {
            let x = rect.left() + (i as f32 * bar_width) + gap/2.0;
            let w = (bar_width - gap).max(1.0);
            let h = level * rect.height();

            // Gradient
            let bar_color = lerp_color(low, high, level);

            // Draw Bar (Bottom-up standard for preview)
            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(x, rect.bottom() - h), 
                egui::vec2(w, h)
            );
            
            // Simple gradient mesh for preview
            use egui::epaint::{Mesh, Vertex};
            let mut mesh = Mesh::default();
            mesh.vertices.push(Vertex { pos: bar_rect.left_bottom(), uv: egui::Pos2::ZERO, color: low });
            mesh.vertices.push(Vertex { pos: bar_rect.right_bottom(), uv: egui::Pos2::ZERO, color: low });
            mesh.vertices.push(Vertex { pos: bar_rect.right_top(), uv: egui::Pos2::ZERO, color: bar_color });
            mesh.vertices.push(Vertex { pos: bar_rect.left_top(), uv: egui::Pos2::ZERO, color: bar_color });
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(egui::Shape::mesh(mesh));

            // Peak
            if level > 0.05 {
                let peak_y = bar_rect.top() - 4.0;
                let peak_rect = egui::Rect::from_min_size(egui::pos2(x, peak_y), egui::vec2(w, 2.0));
                painter.rect_filled(peak_rect, 0.0, peak);
            }
        }
    }

    /// Render settings window content
    fn render_settings_window(&mut self, ui: &mut egui::Ui) {
        let shared_state_ref = self.shared_state.clone();
        let mut state = match shared_state_ref.lock() { Ok(s) => s, Err(_) => return, };

        // Tabs
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            let colors = state.config.resolve_colors(&state.user_color_presets);
            let highlight = to_egui_color(colors.high);
            ui_tab_button(ui, " 🎨 Visual ", SettingsTab::Visual, &mut self.active_tab, highlight);
            ui_tab_button(ui, " 🔊 Audio ", SettingsTab::Audio, &mut self.active_tab, highlight);
            ui_tab_button(ui, " 🌈 Colors ", SettingsTab::Colors, &mut self.active_tab, highlight);
            ui_tab_button(ui, " 🪟 Window ", SettingsTab::Window, &mut self.active_tab, highlight);
            ui_tab_button(ui, " 📊 Stats ", SettingsTab::Performance, &mut self.active_tab, highlight);
        });
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            match self.active_tab {
                SettingsTab::Visual => self.settings_tab_visual(ui, &mut state),
                SettingsTab::Audio => self.settings_tab_audio(ui, &mut state),
                SettingsTab::Colors => self.settings_tab_colors(ui, &mut state),
                SettingsTab::Window => self.settings_tab_window(ui, &mut state),
                SettingsTab::Performance => self.settings_tab_performance(ui, &mut state),
            }
        });
    }

    fn settings_tab_visual(&mut self, ui: &mut egui::Ui, state: &mut SharedState) {
        let grid_spacing = egui::vec2(40.0, 12.0); 
            ui.horizontal(|ui| {
                ui.label("Visual Profile:");
                egui::ComboBox::from_id_salt("viz_profile_combo")
                    .selected_text(&state.config.profile.name)
                    .show_ui(ui, |ui| {
                        // --- User Visual Presets ---
                        let user_visuals = state.user_visual_presets.clone();
                        if !user_visuals.is_empty() {
                            let _ = ui.selectable_label(false, egui::RichText::new("--- User Presets ---").strong());
                            for vp in &user_visuals {
                                ui.horizontal(|ui| {
                                    if ui.selectable_label(state.config.profile.name == vp.name, &vp.name).clicked() {
                                        state.config.profile = vp.clone();
                                    }
                                    // Delete button
                                    if ui.small_button("🗑").clicked() {
                                        let _ = crate::shared_state::AppConfig::delete_user_visual_preset(&vp.name);
                                        // Remove from memory immediately
                                        state.user_visual_presets.retain(|p| p.name != vp.name);
                                    }
                                });
                            }
                            ui.separator();
                        }
                        // --- Built-in Visual Presets ---
                        let _ = ui.selectable_label(false, egui::RichText::new("--- Built-in ---").strong());
                        for vp in VisualProfile::built_in() {
                            if ui.selectable_label(state.config.profile.name == vp.name, &vp.name).clicked() {
                                state.config.profile = vp;
                            }
                        }
                    });
                if ui.button("💾").on_hover_text("Save Profile").clicked() { 
                    self.save_target = SaveTarget::Visual;
                    self.new_preset_name = state.config.profile.name.clone(); // Pre-fill
                }
            });

            // -- Save Popup --
            if self.save_target == SaveTarget::Visual {
                ui_save_popup(ui, &mut self.new_preset_name, |name| {
                    state.config.profile.name = name.clone();
                    if let Err(e) = crate::shared_state::AppConfig::save_user_visual_preset(&state.config.profile) {
                        eprintln!("Error saving visual preset: {}", e);
                    } else {
                        if let Some(existing) = state.user_visual_presets.iter_mut().find(|p| p.name == name) {
                            *existing = state.config.profile.clone();
                        } else {
                            state.user_visual_presets.push(state.config.profile.clone());
                        }
                    }
                }, &mut self.save_target);
            }

            ui.separator();

            // --- Visual Controls ---
            ui.group(|ui| {
                egui::Grid::new("visual_grid").num_columns(2).spacing(grid_spacing).show(ui, |ui| {
                    ui.label("Mode");
                    egui::ComboBox::from_id_salt("viz_mode")
                        .selected_text(format!("{:?}", state.config.profile.visual_mode))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.config.profile.visual_mode, VisualMode::SolidBars, "Solid Bars");
                            ui.selectable_value(&mut state.config.profile.visual_mode, VisualMode::SegmentedBars, "Segmented (LED)");
                            ui.selectable_value(&mut state.config.profile.visual_mode, VisualMode::LineSpectrum, "Line Spectrum");
                            ui.selectable_value(&mut state.config.profile.visual_mode, VisualMode::Oscilloscope, "Oscilloscope");
                        });
                    ui.end_row();
                    
                    // Specific Controls
                    if state.config.profile.visual_mode != VisualMode::Oscilloscope {
                        ui.label("Bar Count");
                        ui.add(egui::Slider::new(&mut state.config.profile.num_bars, 10..=512)
                            .step_by(1.0).drag_value_speed(1.0).smart_aim(false));
                        ui.end_row();

                        ui.label("Bar Gap");
                        ui.add(egui::Slider::new(&mut state.config.profile.bar_gap_px, 0..=10).suffix(" px"));
                        ui.end_row();
                    }
                    
                    ui.label("Bar Opacity");
                    ui.add(egui::Slider::new(&mut state.config.profile.bar_opacity, 0.0..=1.0));
                    ui.end_row();

                    // NEW: Background Opacity Slider Logic
                    ui.label("Background Opacity");
                    // FIX: Resolve immutable colors first, don't hold lock long if possible, 
                    // but here we are modifying state in UI so we need the lock anyway.
                    // The error was that we borrowed `state.config` (immutable via resolve_colors) 
                    // and then tried to mutate `state.config.profile.background`.
                    // FIX: Clone the color needed, don't hold the borrow from resolve_colors
                    let current_bg = state.config.resolve_colors(&state.user_color_presets).background;
                    
                    // Calculate current alpha (0.0 - 1.0)
                    let mut alpha = current_bg.a as f32 / 255.0;
                    
                    ui.horizontal(|ui|{
                        if ui.add(egui::Slider::new(&mut alpha, 0.0..=1.0).show_value(true)).changed() {
                            // Override: Keep active RGB, but enforce new Alpha
                            let new_bg = crate::shared_state::Color32 {
                                r: current_bg.r,
                                g: current_bg.g,
                                b: current_bg.b,
                                a: (alpha * 255.0) as u8
                            };
                            state.config.profile.background = Some(new_bg);
                        }
                        
                        // Show Reset button if override is active
                        if state.config.profile.background.is_some() {
                            if ui.button("↺").on_hover_text("Reset to Preset Default").clicked() {
                                state.config.profile.background = None;
                            }
                        }
                    });
                    ui.end_row();

                    if state.config.profile.visual_mode == VisualMode::SegmentedBars {
                        ui.label("Segment Height");
                        ui.add(egui::Slider::new(&mut state.config.profile.segment_height_px, 1.0..=20.0).suffix(" px"));
                        ui.end_row();

                        ui.label("Segment Gap");
                        ui.add(egui::Slider::new(&mut state.config.profile.segment_gap_px, 0.0..=10.0).suffix(" px"));
                        ui.end_row();
                    }

                    if state.config.profile.visual_mode != VisualMode::Oscilloscope {
                        ui.label("Peak Indicators");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut state.config.profile.show_peaks, "Show");
                            if state.config.profile.show_peaks && state.config.profile.visual_mode == VisualMode::SegmentedBars {
                                ui.checkbox(&mut state.config.profile.fill_peaks, "Fill to Peak");
                            }
                        });
                        ui.end_row();
                    }

                    ui.label("Font Style");
                    egui::ComboBox::from_id_salt("font_combo")
                        .selected_text(format!("{:?}", state.config.profile.overlay_font))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.config.profile.overlay_font, crate::shared_state::ThemeFont::Medium, "Standard");
                            ui.selectable_value(&mut state.config.profile.overlay_font, crate::shared_state::ThemeFont::Monospace, "Retro (Mono)");
                        });
                    ui.end_row();
                });
            });

            ui.add_space(10.0);
            ui.group(|ui| {
                ui.label("Aggregation Mode:");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut state.config.profile.use_peak_aggregation, true, "Peak (Dramatic)");
                    ui.radio_value(&mut state.config.profile.use_peak_aggregation, false, "Average (Smooth)");
                });

                ui.add_space(5.0);
                ui.label("Orientation:");
                ui.checkbox(&mut state.config.profile.inverted_spectrum, "Inverted (Top-Down)");
            });
    }

    fn settings_tab_audio(&mut self, ui: &mut egui::Ui, state: &mut SharedState) {
        let grid_spacing = egui::vec2(40.0, 12.0);
        ui.heading("Audio Configuration");
        ui.add_space(5.0);

        ui.group(|ui| {
            egui::Grid::new("audio_grid")
                .num_columns(2)
                .spacing(grid_spacing)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("FFT Window Size");
                    ui.label(format!("{} samples (fixed)", FIXED_FFT_SIZE));
                    ui.end_row();

                    ui.label("Sensitivity");
                    ui.add(egui::Slider::new(&mut state.config.profile.sensitivity, 0.01..=100.0)
                        .logarithmic(true)
                        .custom_formatter(|v, _| format!("{:+.1} dB", 20.0 * v.log10()))
                    );
                    ui.end_row();

                    ui.label("Noise Floor");
                    ui.add(egui::Slider::new(&mut state.config.noise_floor_db, -120.0..=-20.0).suffix(" dB"));
                    ui.end_row();
                });
        });

        ui.add_space(10.0);
        ui.heading("Response Timing");
        ui.group(|ui| {
            egui::Grid::new("timing_grid")
                .num_columns(2)
                .spacing(grid_spacing)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Bar Attack (Rise)");
                    ui.add(egui::Slider::new(&mut state.config.profile.attack_time_ms, 1.0..=500.0).suffix(" ms"));
                    ui.end_row();

                    ui.label("Bar Release (Fall)");
                    ui.add(egui::Slider::new(&mut state.config.profile.release_time_ms, 1.0..=2000.0).suffix(" ms"));
                    ui.end_row();

                    if state.config.profile.show_peaks {
                        ui.label("Peak Hold Time");
                        ui.add(egui::Slider::new(&mut state.config.profile.peak_hold_time_ms, 0.0..=2000.0).suffix(" ms"));
                        ui.end_row();

                        ui.label("Peak Fall Speed");
                        ui.add(egui::Slider::new(&mut state.config.profile.peak_release_time_ms, 10.0..=2000.0).suffix(" ms"));
                        ui.end_row();
                    }
                });
        });

        ui.add_space(10.0);
        ui.heading("Input Source");
        ui.add_space(5.0);

        ui.group(|ui| {
            egui::Grid::new("audio_source_grid")
                .num_columns(2)
                .spacing(grid_spacing)
                .show(ui, |ui| {
                    ui.label("Device");
                    
                    ui.horizontal(|ui| {
                        // Clone data to satisfy borrow checker (state is already locked)
                        let (current_sel, devices) = {
                            (state.config.selected_device.clone(), state.audio_devices.clone())
                        };

                        // Device Selector
                        egui::ComboBox::from_id_salt("audio_device_combo")
                            .selected_text(&current_sel)
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                
                                // 1. Default Option
                                if ui.selectable_label(current_sel == "Default", "Default System Device").clicked() {
                                    tracing::info!("[GUI] User selected device: Default");
                                    state.config.selected_device = "Default".to_string();
                                    state.device_changed = true;
                                }
                                
                                ui.separator();

                                // 2. Enumerated Hardware Devices
                                for name in devices {
                                    let is_selected = current_sel == name;
                                    if ui.selectable_label(is_selected, &name).clicked() {
                                        tracing::info!("[GUI] User selected device: '{}'", name);
                                        state.config.selected_device = name;
                                        state.device_changed = true;
                                    }
                                }
                            });

                        // Refresh Button
                        if ui.button("🔄").on_hover_text("Refresh Device List").clicked() {
                            tracing::info!("[GUI] User requested device list refresh");
                            state.refresh_devices_requested = true;
                        }
                    });
                    ui.end_row();
                });
        });
    }

    fn settings_tab_colors(&mut self, ui: &mut egui::Ui, state: &mut SharedState) {
         let grid_spacing = egui::vec2(40.0, 12.0); 
         let mut current_colors = state.config.resolve_colors(&state.user_color_presets);
         let initial_colors = current_colors.clone();
         let bar_opacity = state.config.profile.bar_opacity;

         // -- Preset Loader --
         ui.horizontal(|ui| {
            ui.label("Preset:");
            let combo_text = match &state.config.profile.color_link {
                ColorRef::Preset(name) => name.clone(),
                ColorRef::Custom(_) => "Custom (Unsaved)".to_string(),
            };
            egui::ComboBox::from_id_salt("color_preset_combo").selected_text(combo_text).show_ui(ui, |ui| {
                    let user_presets = state.user_color_presets.clone();
                    if !user_presets.is_empty() {
                        let _ = ui.selectable_label(false, egui::RichText::new("--- User Presets ---").strong());
                        for p in &user_presets {
                            ui.horizontal(|ui| {
                                if ui.selectable_label(false, &p.name).clicked() {
                                    state.config.profile.color_link = ColorRef::Preset(p.name.clone());
                                    state.config.profile.background = None;
                                }
                                if ui.small_button("🗑").clicked() {
                                    let _ = crate::shared_state::AppConfig::delete_user_color_preset(&p.name);
                                    state.user_color_presets.retain(|x| x.name != p.name);
                                }
                            });
                        }
                        ui.separator();
                    }
                    let _ = ui.selectable_label(false, egui::RichText::new("--- Built-in ---").strong());
                    for cp in ColorProfile::built_in() {
                        if ui.selectable_label(false, &cp.name).clicked() {
                            state.config.profile.color_link = ColorRef::Preset(cp.name);
                            state.config.profile.background = None;
                        }
                    }
                });
            if ui.button("💾").on_hover_text("Save as User Preset").clicked() {
                    self.save_target = SaveTarget::Color;
                    self.new_preset_name.clear(); // Colors usually saved as new name
            }
         });

         // -- Save Popup --
         if self.save_target == SaveTarget::Color {
            ui_save_popup(ui, &mut self.new_preset_name, |name| {
                let mut new_profile = current_colors.clone();
                new_profile.name = name.clone();
                if let Err(e) = crate::shared_state::AppConfig::save_user_color_preset(&new_profile) {
                    tracing::error!("Failed to save preset: {}", e);
                } else {
                    if let Some(existing) = state.user_color_presets.iter_mut().find(|p| p.name == name) {
                        *existing = new_profile.clone();
                    } else {
                        state.user_color_presets.push(new_profile.clone());
                    }
                    state.config.profile.color_link = ColorRef::Preset(new_profile.name);
                    state.config.profile.background = None;
                }
            }, &mut self.save_target);
         }
         ui.separator();

         // -- Editors --
         let mut egui_low = to_egui_color(current_colors.low);
         let mut egui_high = to_egui_color(current_colors.high);
         let mut egui_peak = to_egui_color(current_colors.peak);
         let mut egui_bg = to_egui_color(current_colors.background);
         let mut egui_text = to_egui_color(current_colors.text);
         let mut egui_insp_bg = to_egui_color(current_colors.inspector_bg);
         let mut egui_insp_fg = to_egui_color(current_colors.inspector_fg);

         ui.group(|ui| {
            egui::Grid::new("color_grid").num_columns(2).spacing(grid_spacing).show(ui, |ui| {
                ui.label("Low"); ui.color_edit_button_srgba(&mut egui_low); ui.end_row();
                ui.label("High"); ui.color_edit_button_srgba(&mut egui_high); ui.end_row();
                ui.label("Peak"); ui.color_edit_button_srgba(&mut egui_peak); ui.end_row();
                ui.label("Background"); ui.color_edit_button_srgba(&mut egui_bg); ui.end_row();
                ui.label("Overlay Text"); ui.color_edit_button_srgba(&mut egui_text); ui.end_row();
                ui.label("Inspector Box"); ui.color_edit_button_srgba(&mut egui_insp_bg); ui.end_row();
                ui.label("Inspector Text/Line"); ui.color_edit_button_srgba(&mut egui_insp_fg); ui.end_row();
            });
         });
         
         ui.add_space(10.0);
         self.render_preview_spectrum(ui, &current_colors, bar_opacity);

         current_colors.low = from_egui_color(egui_low);
         current_colors.high = from_egui_color(egui_high);
         current_colors.peak = from_egui_color(egui_peak);
         current_colors.background = from_egui_color(egui_bg);
         current_colors.text = from_egui_color(egui_text);
         current_colors.inspector_bg = from_egui_color(egui_insp_bg);
         current_colors.inspector_fg = from_egui_color(egui_insp_fg);

         if current_colors != initial_colors {
            state.config.profile.color_link = ColorRef::Custom(current_colors);
            state.config.profile.background = None; 
         }
    }

    fn settings_tab_window(&mut self, ui: &mut egui::Ui, state: &mut SharedState) {
        let grid_spacing = egui::vec2(40.0, 12.0);

        ui.heading("Window Behavior");
        ui.add_space(5.0);
        
        ui.group(|ui| {
            egui::Grid::new("window_grid")
                .num_columns(2)
                .spacing(grid_spacing)
                .show(ui, |ui| {
                    ui.label("Main Window");
                    if ui.checkbox(&mut state.config.always_on_top, "Always on Top").changed() {
                        let level = if state.config.always_on_top {
                            egui::WindowLevel::AlwaysOnTop
                        } else {
                            egui::WindowLevel::Normal
                        };
                        ui.ctx().send_viewport_cmd_to(
                            egui::ViewportId::ROOT,
                            egui::ViewportCommand::WindowLevel(level)
                        );
                    }
                    ui.end_row();

                    ui.label("Ghost Mode 👻");
                    ui.horizontal(|ui| {
                        ui.label("Enable via Lock Icon 🔒");
                        ui.add(egui::Label::new("❓").sense(egui::Sense::hover()))
                            .on_hover_text(
                                "How to use Ghost Mode:\n\
                                1. Click the Lock icon (bottom-left) to enable click-through.\n\
                                2. The window will ignore mouse clicks so you can work through it.\n\
                                3. To UNLOCK: Alt-Tab (switch focus) back to this window.\n\
                                The lock will reactivate temporarily."
                            );
                    });
                    ui.end_row();

                    ui.label("Decorations");
                    if ui.checkbox(&mut state.config.window_decorations, "Show Title Bar").changed() {
                        let show = state.config.window_decorations;
                        ui.ctx().send_viewport_cmd_to(
                            egui::ViewportId::ROOT,
                            egui::ViewportCommand::Decorations(show));
                    }
                    ui.end_row();

                    ui.label("Inspector Tool");
                    ui.checkbox(&mut state.config.inspector_enabled, "Enabled").on_hover_text("Show frequency and dB on mouse hover");
                    ui.end_row();

                    // Media Settings
                    ui.label("Now Playing");
                    egui::ComboBox::from_id_salt("media_mode")
                        .selected_text(format!("{:?}", state.config.media_display_mode))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut state.config.media_display_mode, MediaDisplayMode::FadeOnUpdate, "Fade On Update");
                            ui.selectable_value(&mut state.config.media_display_mode, MediaDisplayMode::AlwaysOn, "Always On");
                            ui.selectable_value(&mut state.config.media_display_mode, MediaDisplayMode::Off, "Off");
                        });
                    ui.end_row();
                });
        });
    }

    fn settings_tab_performance(&mut self, ui: &mut egui::Ui, state: &mut SharedState) {

        
        ui.group(|ui| {
            ui.heading("Performance Monitoring");
            ui.checkbox(&mut state.config.show_stats, "Show Performance Overlay");
            ui.small("Displays FPS, FFT latency, and processing times.");
            
            ui.add_space(10.0);
            ui.heading("Diagnostics");
            
            let info = &state.performance.fft_info;
            egui::Grid::new("perf_grid")
                .num_columns(2)
                .spacing([20.0, 10.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Sample Rate");
                    ui.label(format!("{} Hz", info.sample_rate));
                    ui.end_row();

                    ui.label("FFT Size");
                    ui.label(format!("{} samples (fixed)", info.fft_size));
                    ui.end_row();

                    ui.label("Frequency Resolution");
                    ui.label(format!("{:.2} Hz / bin", info.frequency_resolution));
                    ui.end_row();

                    ui.label("Theoretical Latency");
                    ui.label(format!("{:.2} ms", info.latency_ms));
                    ui.end_row();

                    ui.label("GUI Frame Rate");
                    ui.label(format!("{:.1} FPS", state.performance.gui_fps));
                    ui.end_row();
                });
        });
    }
}


// === Helper Functions ===

    /// A custom "Pill" style tab button with animations and theme integration
fn ui_tab_button(
    ui: &mut egui::Ui,
    label: &str,
    tab: SettingsTab,
    active_tab: &mut SettingsTab,
    highlight_color: egui::Color32,
) {
    let is_selected = *active_tab == tab;

    // Text color: Black/White if selected, default grey if not
    let text_color = if is_selected {
        egui::Color32::BLACK 
    } else {
        ui.visuals().text_color()
    };
    
    // Draw the button
    let response = ui.add(
        egui::Button::new(egui::RichText::new(label).size(14.0).color(text_color))
            .fill(if is_selected {highlight_color} else {egui::Color32::TRANSPARENT})
            .frame(is_selected)     // only paint the background if selected
            .rounding(12.0)         // Rounding = 1/2 the hieght for pill shape
            .min_size(egui::vec2(80.0, 28.0)) // Wide clickable area
    );
    if response.clicked() {
        *active_tab = tab;
    }

    // Subltle hover effect for inactive tabs
    if response.hovered() && !is_selected {
        ui.painter().rect_filled(
            response.rect,
            12.0,
            ui.visuals().widgets.hovered.bg_fill.linear_multiply(0.2)
        );
    }
}

fn ui_save_popup(
    ui: &mut egui::Ui,
    name_buffer: &mut String,
    mut on_save: impl FnMut(String),
    target_flag: &mut SaveTarget,
) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.text_edit_singleline(name_buffer);
            if ui.button("Confirm").clicked() && !name_buffer.is_empty() {
                on_save(name_buffer.clone());
                *target_flag = SaveTarget::None;
            }
            if ui.button("Cancel").clicked() {
                *target_flag = SaveTarget::None;
            }
        });
    });
}



fn from_egui_color(c: egui::Color32) -> StateColor32 {
    StateColor32 { r: c.r(), g: c.g(), b: c.b(), a: c.a() }
}



// =============== Tests ==================
#[cfg(test)]
mod tests {
    use super::*;

    // --- 1. Math Helper Tests ---
    
    // Test the decibel to pixel mapping
    #[test]
    fn test_db_to_px_scaling() {
        let app = SpectrumApp::new(
            Arc::new(Mutex::new(SharedState::new())), 
            crossbeam_channel::unbounded().1, 
            Arc::new(PlatformMedia::new()) // Dummy media
        );

        let max_h = 100.0;
        let floor = -60.0; // The noise floor

        // Case A: Signal is at noise floor (should be 0 height)
        let h_silence = theme::db_to_px(-60.0, floor, max_h);
        assert_eq!(h_silence, 0.0);

        // Case B: Signal is below noise floor (should be clamped to 0)
        let h_deep_silence = theme::db_to_px(-100.0, floor, max_h);
        assert_eq!(h_deep_silence, 0.0);

        // Case C: Signal is at 0dB (should be max height)
        let h_max = theme::db_to_px(0.0, floor, max_h);
        assert_eq!(h_max, 100.0);

        // Case D: Signal is clipped > 0dB (should be clamped to max)
        let h_clip = theme::db_to_px(10.0, floor, max_h);
        assert_eq!(h_clip, 100.0);
    }

    // Test the Color Interpolation (Lerp)
    #[test]
    fn test_lerp_color_logic() {
        let c1 = egui::Color32::from_rgb(0, 0, 0);       // Black
        let c2 = egui::Color32::from_rgb(100, 200, 255); // Blue-ish

        // 0.0 -> Start Color
        assert_eq!(lerp_color(c1, c2, 0.0), c1);

        // 1.0 -> End Color
        assert_eq!(lerp_color(c1, c2, 1.0), c2);

        // 0.5 -> Midpoint
        let mid = lerp_color(c1, c2, 0.5);
        assert_eq!(mid.r(), 50);
        assert_eq!(mid.g(), 100);
        assert_eq!(mid.b(), 127); // 255/2 = 127.5 -> 127
    }

    // --- 2. Helper Logic Tests ---
    
    #[test]
    fn test_ui_tab_button_logic() {
        // Just verifying the enum logic works as expected
        let mut active = SettingsTab::Visual;
        
        // Simulating a click logic (abstractly)
        let clicked_tab = SettingsTab::Audio;
        if clicked_tab != active {
            active = clicked_tab;
        }
        
        assert_eq!(active, SettingsTab::Audio);
    }
}