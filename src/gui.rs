use crossbeam_channel::Receiver;
use eframe:: egui;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::fft_config::FIXED_FFT_SIZE;
use crate::media::{PlatformMedia, MediaController};
use crate::shared_state::{Color32 as StateColor32, ColorProfile, MediaDisplayMode, SharedState, VisualMode, VisualProfile};
use crate::fft_processor::FFTProcessor;
use crate::shared_state::ColorRef;

#[derive(PartialEq)]
enum SettingsTab {
    Visual, 
    Audio,
    Colors,
    Window,
    Performance,
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
        [0.0, 0.0, 0.0, 0.0] // Fully transparent   
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
        }
        self.was_focused = is_focused;
        
        let mut flash_strength = 0.0;
        if let Some(start) = self.flash_start {
            let elapsed = start.elapsed().as_secs_f32();
            if elapsed < 0.8 {
                flash_strength = (1.0 - (elapsed / 0.8)).powi(3);
                ctx.request_repaint();
            } else {
                self.flash_start = None;
            }
        }

        // Use Profile Background Color
        let (bg_color_egui, window_locked, background_alpha) = if let Ok(state) = self.shared_state.lock() {
            let colors = state.config.resolve_colors();
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
                self.render_visualizer(ui);
                if flash_strength > 0.0 {
                    self.draw_sonar_ping(ui, ui.max_rect().shrink(5.0), flash_strength);
                }
                self.render_media_overlay(ui);
                self.window_controls(ctx, ui, is_focused);
            });
        
        //  === SETTINGS WINDOW (Separate Viewport) ===
        if self.settings_open {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("settings_viewport"),
                egui::ViewportBuilder::default()
                    .with_title("BeAnal Settings")
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

    /// Draw invisible resize handles, handle window moverment, and context menu
    fn window_controls(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, is_focused: bool) {
        // use max_rect to get the area of everyhting drawn so far (the whole window!)
        let rect = ui.max_rect();

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

        // 2. Resize Grip (Bottom Right Corner)
        self.draw_resize_grip(ctx, ui, &rect);

        // 3. Lock Button (Bottom Left Corner)
        self.draw_lock_button(ui, rect, is_focused);
    }

    /// Render the main spectrum visualizer
    fn render_visualizer(&mut self, ui: &mut egui::Ui) {
        // 1. Aquire Locks and Setup
        let state = match self.shared_state.lock() {
            Ok(state) => state,
            Err(_) => {
                ui.centered_and_justified(|ui| {
                    ui.label("⚠ Error: Cannot access audio data");
                });
                return;
            }
        };

        let config = &state.config;
        let profile = &config.profile;
        let colors = &config.resolve_colors();

        let viz_data = &state.visualization;
        let perf = &state.performance;

        // 2. Allocate Drawing Space
        let available_size = ui.available_size();
        let (_, rect) = ui.allocate_space(available_size);

        // We grab the painter directly to draw on top of the empty space
        let painter = ui.painter();

        // 3. Early exit if no data (unless in Oscope mode)
        let num_bars = viz_data.bars.len();
        if num_bars == 0 && profile.visual_mode != VisualMode::Oscilloscope {
            drop(state);
            ui.centered_and_justified(|ui| {
                ui.label("⏸ Waiting for audio...");
            });
            return;
        }

        // 4. Calculate Common Layout Helpers
        // Ensure we don't divide by zero even if bars are missing
        let bar_slot_width = rect.width() / num_bars.max(1) as f32;
        let bar_width = (bar_slot_width - profile.bar_gap_px as f32).max(1.0);

        // 5. Handle mouse interactions (for frequency modes)
        let hovered_bar_index = if config.inspector_enabled && profile.visual_mode != VisualMode::Oscilloscope {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                if rect.contains(pos) {
                    let relative_x = pos.x - rect.left();
                    let index = (relative_x / bar_slot_width).floor() as usize;
                    if index < num_bars {Some(index)} else { None }
                }else { None }
            }else { None }
        } else { None };

        // 6. Dispatch Drawing Strategy
        match profile.visual_mode {
            VisualMode::SolidBars => {
                self.draw_solid_bars(&painter, &rect, profile, &colors, viz_data, bar_width, bar_slot_width, hovered_bar_index, config.noise_floor_db);
            },
            VisualMode::SegmentedBars => {
                self.draw_segmented_bars(&painter, &rect, profile, &colors, viz_data, bar_width, bar_slot_width, hovered_bar_index, config.noise_floor_db);
            },
            VisualMode::LineSpectrum => {
                self.draw_line_spectrum(&painter, &rect, profile, &colors, viz_data, hovered_bar_index, config.noise_floor_db);
            },
            VisualMode::Oscilloscope => {
                self.draw_oscilloscope(&painter, &rect, profile, &colors, viz_data);
            },
        }
        
        // 7. Draw Overlays
        if let Some(index) = hovered_bar_index {
            self.draw_inspector_overlay(&painter, &rect, &colors, config.noise_floor_db, viz_data, perf, index, bar_slot_width);
        }

        if config.show_stats {
            self.draw_stats_overlay(&painter, &rect, &colors, perf);
        }
    }

    // ========== DRAWING HELPERS ==========

    fn render_media_overlay(&mut self, ui: &mut egui::Ui) {
        let state = self.shared_state.lock().unwrap();
        let config = &state.config;

        // 1. Handle "Off" case early
        if config.media_display_mode == MediaDisplayMode::Off {
            return;
        }

        let colors = config.resolve_colors();
        let base_text_color = to_egui_color(colors.text);

        // 2. Info check
        let info_opt = state.media_info.clone();

        // Font Selection
        let font_family = match config.profile.overlay_font {
            crate::shared_state::ThemeFont::Standard => egui::FontFamily::Proportional,
            crate::shared_state::ThemeFont::Monospace => egui::FontFamily::Monospace,
        };
        
        // 3. Layout Rect calculation
        // Calculate based on the full screen rect since we use an Area
        let rect = ui.ctx().screen_rect();
        let overlay_w = rect.width() * 0.5;
        let overlay_h = 100.0;
        let pos = egui::pos2(rect.right() - overlay_w - 20.0, rect.top() + 20.0);

        // 4. Determine Interaction / Active State & Target Opacity
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        let mut target_opacity = 0.0;

        // If info is missing but we are in AlwaysOn, we show placeholder at full opacity
        // If info is missing and Fade, we show nothing.
        let has_info = info_opt.is_some();

        match config.media_display_mode {
            MediaDisplayMode::AlwaysOn => target_opacity = 1.0,
            MediaDisplayMode::FadeOnUpdate => {
                if !has_info {
                    target_opacity = 0.0;
                } else {
                    let now = Instant::now();
                    let hold_time = 5.0; // Stay visible for 5s after event
                    let mut active = false;

                    // A. Check Track Update Activity
                    if let Some(last_update) = state.last_media_update {
                        if now.duration_since(last_update).as_secs_f32() < hold_time {
                            active = true;
                        }
                    }

                    // B. Check Mouse Hover Activity (Global Window)
                    if ui.input(|i| i.pointer.hover_pos().is_some()) {
                        self.last_media_interaction = Some(now);
                        active = true;
                    }

                    // C. Check Historic Interaction
                    if let Some(last_interact) = self.last_media_interaction {
                        if now.duration_since(last_interact).as_secs_f32() < hold_time {
                            active = true;
                        }
                    }

                    target_opacity = if active { 1.0 } else { 0.0 };
                }
            },
            MediaDisplayMode::Off => {},
        }

        // 5. Animate Opacity
        let speed = if target_opacity > self.media_opacity { 6.0 } else { 1.0 };
        self.media_opacity += (target_opacity - self.media_opacity) * speed * dt;
        self.media_opacity = self.media_opacity.clamp(0.0, 1.0);

        if self.media_opacity <= 0.01 {
            return; // Invisible
        }

        // Force repaint if animating
        if self.media_opacity > 0.01 && self.media_opacity < 0.99 {
            ui.ctx().request_repaint();
        }

        // 6. Draw Content in a Floating Area
        // We use an Area so it sits *above* the visualization layer (z-order)
        // and uses absolute positioning.
        egui::Area::new("media_overlay_area".into()) // Fixed: Convert string to Id
            .fixed_pos(pos)
            .pivot(egui::Align2::LEFT_TOP)
            .interactable(true) // Allow clicks on buttons
            .show(ui.ctx(), |ui| {
                // Manually restrict size
                ui.set_max_width(overlay_w);
                ui.set_max_height(overlay_h);

                // Use the builder pattern for new UI
                let builder = egui::UiBuilder::new();
                ui.allocate_new_ui(builder, |ui| {
                    // Force Right-to-Left layout
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        
                        if let Some(info) = info_opt {
                            // === CASE A: Track Info Present ===
                            
                            // Album Art
                            if let Some(texture) = &self.album_art_texture {
                                let tint = egui::Color32::WHITE.linear_multiply(self.media_opacity);
                                ui.add(
                                    egui::Image::new(texture)
                                        .max_height(50.0)
                                        .rounding(4.0)
                                        .tint(tint)
                                );
                                ui.add_space(10.0); 
                            }

                            // Text Stack
                            ui.vertical(|ui| {
                                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                    // Title
                                    ui.label(egui::RichText::new(&info.title)
                                        .font(egui::FontId::new(16.0, font_family.clone()))
                                        .strong()
                                        .color(base_text_color.linear_multiply(self.media_opacity))
                                    );

                                    // Artist
                                    ui.label(egui::RichText::new(format!("{} - {}", info.artist, info.album))
                                        .font(egui::FontId::new(11.0, font_family.clone()))
                                        .color(base_text_color.linear_multiply(0.8).linear_multiply(self.media_opacity))
                                    );

                                    ui.add_space(2.0);

                                    // Controls
                                    if cfg!(not(target_os = "macos")) {
                                        ui.add_space(4.0);
                                        self.render_transport_controls(ui, info.is_playing, self.media_opacity, base_text_color);
                                    } else {
                                        ui.label(egui::RichText::new(format!("via {}", info.source_app))
                                            .font(egui::FontId::new(10.0, font_family.clone()))
                                            .color(base_text_color.linear_multiply(0.5).linear_multiply(self.media_opacity))
                                        );
                                    }
                                });
                            });

                        } else if config.media_display_mode == MediaDisplayMode::AlwaysOn {
                            // === CASE B: No Info, but Always On ===
                            ui.vertical(|ui| {
                                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                    ui.label(egui::RichText::new("Waiting for media...")
                                        .font(egui::FontId::new(14.0, font_family.clone()))
                                        .color(egui::Color32::from_white_alpha(150).linear_multiply(self.media_opacity))
                                        .color(base_text_color.linear_multiply(0.6).linear_multiply(self.media_opacity))
                                    );
                                });
                            });
                        }
                    });
                });
            });
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

    /// Draw solid gradient bars
    fn draw_solid_bars(
        &self, 
        painter: &egui::Painter, 
        rect: &egui::Rect, 
        profile: &VisualProfile,
        colors: &ColorProfile, 
        data: &crate::shared_state::VisualizationData,
        bar_width: f32,
        slot_width: f32,
        hovered_index: Option<usize>,
        noise_floor: f32
    ) {
        let low = to_egui_color(colors.low).linear_multiply(profile.bar_opacity);
        let high = to_egui_color(colors.high).linear_multiply(profile.bar_opacity);
        let peak = to_egui_color(colors.peak).linear_multiply(profile.bar_opacity);

        for (i, &db) in data.bars.iter().enumerate() {
            let x = rect.left() + (i as f32 * slot_width);
            let bar_height = self.db_to_px(db, noise_floor, rect.height());
            // Safe clamp for gradient
            let norm_height = (bar_height / rect.height()).clamp(0.0, 1.0);

            // Gradient Base Color
            let mut bar_color = lerp_color(low, high, norm_height);
            if Some(i) == hovered_index {
                bar_color = lerp_color(bar_color, egui::Color32::WHITE, 0.5);
            }

            use egui::epaint::Vertex;
            let bar_rect;
            let mesh_base;
            let mesh_tip;

            if profile.inverted_spectrum {
                bar_rect = egui::Rect::from_min_max(
                    egui::pos2(x, rect.top()),
                    egui::pos2(x + bar_width, rect.top() + bar_height ),
                );
                mesh_base = low;
                mesh_tip = bar_color;
            } else {
                bar_rect = egui::Rect::from_min_max(
                    egui::pos2(x, rect.bottom() - bar_height),
                    egui::pos2(x + bar_width, rect.bottom()),
                );
                mesh_base = low;
                mesh_tip = bar_color;
            }

            // Draw Mesh
            let mut mesh = egui::Mesh::default();
            if profile.inverted_spectrum {
                mesh.vertices.push(Vertex {pos: bar_rect.left_top(), uv: egui::Pos2::ZERO, color: mesh_base});
                mesh.vertices.push(Vertex {pos: bar_rect.right_top(),uv: egui::Pos2::ZERO, color: mesh_base});
                mesh.vertices.push(Vertex {pos: bar_rect.right_bottom(), uv: egui::Pos2::ZERO, color: mesh_tip});
                mesh.vertices.push(Vertex {pos: bar_rect.left_bottom(), uv: egui::Pos2::ZERO, color: mesh_tip});
            } else {
                mesh.vertices.push(Vertex {pos: bar_rect.left_bottom(), uv: egui::Pos2::ZERO, color: mesh_base});
                mesh.vertices.push(Vertex {pos: bar_rect.right_bottom(),uv: egui::Pos2::ZERO, color: mesh_base});
                mesh.vertices.push(Vertex {pos: bar_rect.right_top(), uv: egui::Pos2::ZERO, color: mesh_tip});
                mesh.vertices.push(Vertex {pos: bar_rect.left_top(), uv: egui::Pos2::ZERO, color: mesh_tip});
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(egui::Shape::mesh(mesh));

            // Peaks
            if profile.show_peaks && i < data.peaks.len() {
                let peak_h = self.db_to_px(data.peaks[i], noise_floor, rect.height());
                
                let peak_rect = if profile.inverted_spectrum {
                    let y = rect.top() + peak_h;
                    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(bar_width, 2.0))
                } else {
                    let y = rect.bottom() - peak_h;
                    egui::Rect::from_min_size(egui::pos2(x, y - 2.0), egui::vec2(bar_width, 2.0))
                };
                painter.rect_filled(peak_rect, 0.0, peak);
            }
        }
    }
    
    /// Draw segmented bars helper function
    ///
    /// Renders the spectrum as a series of discrete blocks (LED style).
    /// Handles:
    /// - Gradient coloring based on height
    /// - Inverted/Standard orientation
    /// - Peak indicators
    /// - "Fill to Peak" warning mode
    fn draw_segmented_bars(
        &self, 
        painter: &egui::Painter, 
        rect: &egui::Rect,
        profile: &VisualProfile,
        colors: &ColorProfile, 
        data: &crate::shared_state::VisualizationData,
        bar_width: f32,
        slot_width: f32,
        hovered_index: Option<usize>,
        noise_floor: f32
    ) {
        // 1. Resolve Colors & Opacity
        let low = to_egui_color(colors.low).linear_multiply(profile.bar_opacity);
        let high = to_egui_color(colors.high).linear_multiply(profile.bar_opacity);
        let peak_color = to_egui_color(colors.peak).linear_multiply(profile.bar_opacity);

        // 2. Calculate Segment Geometry
        // Ensure we don't get stuck in infinite loops with 0 height
        let seg_h = profile.segment_height_px.max(1.0);
        let seg_gap = profile.segment_gap_px.max(0.0);
        let total_seg_h = seg_h + seg_gap;

        // 3. Render Each Bar
        for (i, &db) in data.bars.iter().enumerate() {
             let x = rect.left() + (i as f32 * slot_width);
             
             // Convert dB to pixel height
             let total_h = self.db_to_px(db, noise_floor, rect.height());
             
             // Determine how many segments fit in this height
             let num_segments = (total_h / total_seg_h).floor() as i32;
             
             // --- Draw Active Segments ---
             for s in 0..num_segments {
                 let segment_idx = s as f32;
                 let y_offset = segment_idx * total_seg_h;
                 
                 // Calculate gradient color based on vertical position
                 let norm_h = (y_offset / rect.height()).clamp(0.0, 1.0);
                 let color = lerp_color(low, high, norm_h);

                 // Calculate rect based on orientation
                 let seg_rect = if profile.inverted_spectrum {
                     // Top-Down
                     egui::Rect::from_min_size(
                         egui::pos2(x, rect.top() + y_offset),
                         egui::vec2(bar_width, seg_h)
                     )
                 } else {
                     // Bottom-Up
                     egui::Rect::from_min_size(
                         egui::pos2(x, rect.bottom() - y_offset - seg_h),
                         egui::vec2(bar_width, seg_h)
                     )
                 };
                 painter.rect_filled(seg_rect, 1.0, color);
             }

             // --- Draw Peak Indicators ---
             if profile.show_peaks && i < data.peaks.len() {
                 let peak_h = self.db_to_px(data.peaks[i], noise_floor, rect.height());
                 
                 // Snap peak to the nearest segment grid position
                 let peak_seg_idx = (peak_h / total_seg_h).floor();
                 let y_offset = peak_seg_idx * total_seg_h;
                 
                 let peak_rect = if profile.inverted_spectrum {
                     egui::Rect::from_min_size(egui::pos2(x, rect.top() + y_offset), egui::vec2(bar_width, seg_h))
                 } else {
                     egui::Rect::from_min_size(egui::pos2(x, rect.bottom() - y_offset - seg_h), egui::vec2(bar_width, seg_h))
                 };
                 
                 painter.rect_filled(peak_rect, 1.0, peak_color);

                 // --- Fill Gap to Peak (Warning Mode) ---
                 // If enabled, fills the empty space between the current bar level and the peak
                 // with a dim color. Useful for seeing dynamic range.
                 if profile.fill_peaks {
                     let gap_segments = (peak_seg_idx as i32) - num_segments;
                     if gap_segments > 0 {
                         let fill_color = peak_color.linear_multiply(0.3);
                         for g in 0..gap_segments {
                             // Offset from the top of the current bar
                             let gap_y = (num_segments + g) as f32 * total_seg_h;
                             let gap_rect = if profile.inverted_spectrum {
                                 egui::Rect::from_min_size(egui::pos2(x, rect.top() + gap_y), egui::vec2(bar_width, seg_h))
                             } else {
                                 egui::Rect::from_min_size(egui::pos2(x, rect.bottom() - gap_y - seg_h), egui::vec2(bar_width, seg_h))
                             };
                             painter.rect_filled(gap_rect, 1.0, fill_color);
                         }
                     }
                 }
             }
         }
    }


    fn draw_line_spectrum(&self, painter: &egui::Painter, rect: &egui::Rect, profile: &VisualProfile, colors: &ColorProfile, data: &crate::shared_state::VisualizationData, hovered_index: Option<usize>, noise_floor: f32) {
        if data.bars.is_empty() { return; }
        
        // Use Profile colors
        let high = to_egui_color(colors.high).linear_multiply(profile.bar_opacity);

        // Pre-calculate points 
        let points: Vec<egui::Pos2> = data.bars.iter().enumerate().map(|(i, &db)| {
            let x = rect.left() + (i as f32 / data.bars.len() as f32) * rect.width();
            let height = self.db_to_px(db, noise_floor, rect.height());
        
            let y = if profile.inverted_spectrum {
                rect.top() + height
            } else {
                rect.bottom() - height
            };

            egui::pos2(x, y)
        }).collect();

        // Draw Glow (thick transparent line) - Restored!
        let glow_c = high.linear_multiply(0.3);
        painter.add(egui::Shape::line(points.clone(), egui::Stroke::new(4.0, glow_c)));

        // Draw Core (thin bright line) - Restored!
        let core_c = high; 
        painter.add(egui::Shape::line(points.clone(), egui::Stroke::new(2.0, core_c)));

        // Optional: Fill below line. Maybe remove?
        /*/
        if points.len() > 2 {
            let mut fill_points = points.clone();
            fill_points.push(egui::pos2(rect.right(), if profile.inverted_spectrum { rect.top() } else { rect.bottom() }));
            fill_points.push(egui::pos2(rect.left(), if profile.inverted_spectrum { rect.top() } else { rect.bottom() }));
            
            let fill_color = to_egui_color(colors.low).linear_multiply(0.15 * profile.bar_opacity);
            painter.add(egui::Shape::convex_polygon(fill_points, fill_color, egui::Stroke::NONE));
        }
        */

        // Draw hover Indicator - Restored!
        if let Some(idx) = hovered_index {
            if let Some(point) = points.get(idx){
                // Bright white dot with colored glow
                painter.circle_filled(*point, 4.0, egui::Color32::WHITE);
                painter.circle_stroke(*point, 5.0, egui::Stroke::new(1.0, core_c));
            }
        }
    }
     
    fn draw_oscilloscope(
        &self, 
        painter: &egui::Painter, 
        rect: &egui::Rect,
        profile: &VisualProfile,
        colors: &ColorProfile,
        data: &crate::shared_state::VisualizationData,
    ) {
        if data.waveform.len() < 2 { return; }
    
        let center_y = rect.center().y;
        // Scale: Audio is +/- 1.0, we map that to +/- half height
        // Sensitivity scales the amplitude
        let scale = (rect.height() / 2.0 ) * profile.sensitivity;

        // Downsampling for performance if buffer is huge
        // Just drawing every Nth sample or average could work, but simple stride is fast
        let step_x = rect.width() / (data.waveform.len() as f32 - 1.0);

        let points: Vec<egui::Pos2> = data.waveform.iter().enumerate().map(|(i, &sample)| {
            let x = rect.left() + (i as f32 * step_x);
            let y = center_y - (sample.clamp(-1.0, 1.0) * scale);
            egui::pos2(x, y)
        }).collect();
        
        let high = to_egui_color(colors.high).linear_multiply(profile.bar_opacity);
        painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, high)));
    }
    
    // === OVERLAYS ===

    fn draw_resize_grip(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, rect: &egui::Rect) {
        let corner_size = 20.0;
        let grip_rect = egui::Rect::from_min_size(
            egui::pos2(rect.right() - corner_size, rect.bottom() - corner_size),
            egui::Vec2::splat(corner_size)
        );

        let response = ui.interact(grip_rect, ui.id().with("resize_grip"), egui::Sense::drag());

        if response.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeSouthEast);
        }

        // Use button_pressed() for instant resize start
        if response.hovered() && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(egui::ResizeDirection::SouthEast));
        }

        if ui.is_rect_visible(grip_rect) {
            let painter = ui.painter();
            let stroke = egui::Stroke::new(2.0, egui::Color32::from_white_alpha(50));
            
            for i in 0..4 {
                let offset = i as f32 * 4.0;
                painter.line_segment(
                    [
                        egui::pos2(rect.right() - 4.0 - offset, rect.bottom() - 4.0),
                        egui::pos2(rect.right() - 4.0, rect.bottom() - 4.0 - offset),
                    ],
                    stroke,
                );
            }
        }
    }

    fn draw_lock_button(&self, ui: &mut egui::Ui, rect: egui::Rect, is_focused: bool) {
        // we need mutable access to the toggle the state
        let mut state = match self.shared_state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };

        // Use resolved background alpha
        let colors = state.config.resolve_colors();
        let bg_alpha = colors.background.a as f32 / 255.0;

        // only show if background is transparent
        if bg_alpha >= 0.05 {
            return;
        }


        let is_locked = state.config.window_locked;
        let size = 24.0;
        let padding = 8.0;

        // Position: Bottom left with padding
        let lock_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + padding, rect.bottom() - size - padding),
            egui::Vec2::splat(size)
        );

        // Handle Click
        let response = ui.interact(lock_rect, ui.id().with("lock_btn"), 
            egui::Sense::click());
        if response.clicked() {
            state.config.window_locked = !state.config.window_locked;
        }

        if response.hovered() {
            let text = if is_locked {
                "GHOST MODE ACTIVE\n\n1. Window is click-through (ignore mouse).\n2. Alt-Tab away to engage.\n3. Alt-Tab back to unlock."
            } else {
                "ENTER GHOST MODE\n\nClick to make window click-through.\n(Must be transparent first)"
            };
            response.clone().on_hover_text(text);
        }

        // ---- Visuals ----
        let painter = ui.painter();

        // Color Logic:
        // -- Locked and Focused : Bright Red (wake up!)
        // -- Locked and Unfocused : Dim Red (ghost mode)
        // -- Unlocked : White/ Grey (passive)
        let color = if is_locked {
            if is_focused { egui::Color32::from_rgb(255,100,100) }
            else { egui::Color32::from_rgb(200,50,50) }
        } else {
            if response.hovered() { egui::Color32::WHITE } else { egui::Color32::from_white_alpha(100) }
        };

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

    fn draw_sonar_ping(&self, ui: &mut egui::Ui, rect: egui::Rect, strength: f32) {
        // 1. Setup
        // We grab the 'High' color from your theme for the glow
        let colors = self.shared_state.lock().unwrap().config.resolve_colors();
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

    fn draw_inspector_overlay(
        &self, 
        painter: &egui::Painter, 
        rect: &egui::Rect, 
        colors: &ColorProfile,
        noise_floor: f32,
        data: &crate::shared_state::VisualizationData,
        perf: &crate::shared_state::PerformanceStats,
        index: usize,
        slot_width: f32,
    ) {

        // Crosshair
        let center_x = rect.left() + (index as f32 * slot_width) + (slot_width / 2.0);
        painter.line_segment(
            [egui::pos2(center_x, rect.top()), egui::pos2(center_x, rect.bottom())],
            egui::Stroke::new(1.0, to_egui_color(colors.inspector_fg))
        );

        // Label Calculation
        let amp_db = data.bars[index];
        let freq_hz = FFTProcessor::calculate_bar_frequency(
            index, 
            data.bars.len(),
            perf.fft_info.sample_rate,
            perf.fft_info.fft_size
        );

        let freq_text = if freq_hz >= 1000.0 {
            format!("{:.1} kHz", freq_hz / 1000.0)
        } else {
            format!("{:.0} Hz", freq_hz)
        };
        let label = format!("{} | {:+.1} dB", freq_text, amp_db);

        // ToolTip
        let font_id = egui::FontId::proportional(14.0);
        let galley = painter.layout_no_wrap(label,  font_id, egui::Color32::WHITE);
        let padding = 6.0;
        let w =  galley.size().x + padding * 2.0;
        let h = galley.size().y + padding * 2.0;

        let mut pos = if let Some(mouse) = painter.ctx().input(|i| i.pointer.hover_pos()) {
            mouse + egui::vec2(15.0, 0.0)
        } else {
            rect.center()
        };

        // Screen bounds check
        if pos.x + w > rect.right() { pos.x -= w + 30.0; }
        pos.y = pos.y.clamp(rect.top(), rect.bottom() - h);

        let label_rect = egui::Rect::from_min_size(pos, egui::vec2(w, h));
        painter.rect_filled(label_rect, 4.0, to_egui_color(colors.inspector_bg));
        painter.rect_stroke(label_rect, 4.0, egui::Stroke::new(1.0, to_egui_color(colors.inspector_fg)));
        painter.galley(label_rect.min + egui::vec2(padding, padding), galley, egui::Color32::WHITE);
    }

    /// Render performance statistics overlay
    fn draw_stats_overlay(&self, painter: &egui::Painter, rect: &egui::Rect, colors: &ColorProfile, perf: &crate::shared_state::PerformanceStats){
        // Position in top-left (with padding)
        let pos = rect.left_top() + egui::vec2(10.0, 10.0);
        
        let text = format!(
            "FPS: {:.0}\nFFT: {:.1}ms\nMin/Max: {:.1}/{:.1}ms\nRes: {:.1}Hz",
            perf.gui_fps,
            perf.fft_ave_time.as_micros() as f32 / 1000.0,
            perf.fft_min_time.as_micros() as f32 / 1000.0,
            perf.fft_max_time.as_micros() as f32 / 1000.0,
            perf.fft_info.frequency_resolution
        );

        // Reuse Inspector colors for consistency
        let bg_color = to_egui_color(colors.inspector_bg);
        let text_color = to_egui_color(colors.inspector_fg);

        // Draw background box
        // We estimate size or let text layout determine it, but simple rect is safer for now
        let galley = painter.layout_no_wrap(
            text, 
            egui::FontId::proportional(12.0), 
            text_color
        );
        
        let pad = 6.0;
        let bg_rect = egui::Rect::from_min_size(pos, galley.size() + egui::vec2(pad*2.0, pad*2.0));
        
        painter.rect_filled(bg_rect, 4.0, bg_color);
        painter.galley(pos + egui::vec2(pad, pad), galley, egui::Color32::TRANSPARENT); // Text color is baked into galley
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
        let mut state = match self.shared_state.lock() { Ok(s) => s, Err(_) => return, };
        let grid_spacing = egui::vec2(40.0, 12.0); 

        // Tabs
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            let colors = state.config.resolve_colors();
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
                SettingsTab::Visual => {
                    ui.horizontal(|ui| {
                        ui.label("Visual Profile:");
                        egui::ComboBox::from_id_salt("viz_profile_combo")
                            .selected_text(&state.config.profile.name)
                            .show_ui(ui, |ui| {
                                for vp in VisualProfile::built_in() {
                                    if ui.selectable_label(state.config.profile.name == vp.name, &vp.name).clicked() {
                                        state.config.profile = vp;
                                    }
                                }
                            });
                        if ui.button("💾").on_hover_text("Save Profile").clicked() { /* Save Logic */ }
                    });
                    ui.separator();

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
                            let current_bg = state.config.resolve_colors().background;
                            
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
                                    ui.selectable_value(&mut state.config.profile.overlay_font, crate::shared_state::ThemeFont::Standard, "Standard");
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
                },
                SettingsTab::Audio => {
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
                },
                SettingsTab::Colors => {
                    ui.heading("Colors");
                    // FIX: Don't hold refs to state.config inside local vars if we mutate it later via profile
                    let mut current_colors = state.config.resolve_colors();
                    let initial_colors = current_colors.clone();
                    let bar_opacity = state.config.profile.bar_opacity;

                    ui.horizontal(|ui| {
                        ui.label("Preset:");
                        let combo_text = match &state.config.profile.color_link {
                            ColorRef::Preset(name) => name.clone(),
                            ColorRef::Custom(_) => "Custom".to_string(),
                        };
                        egui::ComboBox::from_id_salt("color_preset_combo")
                            .selected_text(combo_text)
                            .show_ui(ui, |ui| {
                                for cp in ColorProfile::built_in() {
                                    if ui.selectable_label(false, &cp.name).clicked() {
                                        state.config.profile.color_link = ColorRef::Preset(cp.name);
                                        // Reset override when switching preset
                                        state.config.profile.background = None;
                                    }
                                }
                            });
                        if ui.button("💾").on_hover_text("Save Color Preset").clicked() { /* Save Logic */ }
                    });
                    ui.separator();

                    let mut egui_low = to_egui_color(current_colors.low);
                    let mut egui_high = to_egui_color(current_colors.high);
                    let mut egui_peak = to_egui_color(current_colors.peak);
                    let mut egui_bg = to_egui_color(current_colors.background);
                    let mut egui_text = to_egui_color(current_colors.text);
                    let mut egui_insp_bg = to_egui_color(current_colors.inspector_bg);
                    let mut egui_insp_fg = to_egui_color(current_colors.inspector_fg);

                    ui.group(|ui| {
                        egui::Grid::new("color_grid").num_columns(2).spacing(grid_spacing).show(ui, |ui| {
                            ui.label("Low");
                            ui.color_edit_button_srgba(&mut egui_low);
                            ui.end_row();

                            ui.label("High");
                            ui.color_edit_button_srgba(&mut egui_high);
                            ui.end_row();

                            ui.label("Peak");
                            ui.color_edit_button_srgba(&mut egui_peak);
                            ui.end_row();

                            ui.label("Background");
                            ui.color_edit_button_srgba(&mut egui_bg);
                            ui.end_row();
                            
                            ui.label("Overlay Text");
                            ui.color_edit_button_srgba(&mut egui_text);
                            ui.end_row();

                            ui.label("Inspector Box");
                            ui.color_edit_button_srgba(&mut egui_insp_bg);
                            ui.end_row();

                            ui.label("Inspector Text/Line");
                            ui.color_edit_button_srgba(&mut egui_insp_fg);
                            ui.end_row();
                        });
                    });

                    // NEW: Render the Live Preview
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
                        state.config.profile.background = None; // Clear override if user manually picks color
                    }
                },
                SettingsTab::Window => {
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
                },
                SettingsTab::Performance => {
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
                },
                // Removed unreachable pattern warning (all enum variants handled)
                //_ => {} 
            }
        });
    }  

    // == Helper Functions ==
    fn db_to_px(&self, db: f32, noise_floor: f32, max_height: f32) -> f32 {
        let range = (0.0 - noise_floor).max(1.0);
        let normalized = ((db - noise_floor) / range).clamp(0.0, 1.0);
        normalized * max_height
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


/// Convert our Color32 to egui::Color32
fn to_egui_color(color: StateColor32) -> egui::Color32 {
    egui::Color32::from_rgba_premultiplied(color.r, color.g, color.b, color.a)
}

fn from_egui_color(c: egui::Color32) -> StateColor32 {
    StateColor32 { r: c.r(), g: c.g(), b: c.b(), a: c.a() }
}

/// Linear interpolation between two egui colors
fn lerp_color(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    egui::Color32::from_rgba_premultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
        (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t) as u8,
    )
}

// =============== Tests ==================
