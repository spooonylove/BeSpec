use crossbeam_channel::Receiver;
use eframe:: egui;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::fft_config::FIXED_FFT_SIZE;
use crate::shared_state::{Color32 as StateColor32, MediaDisplayMode, SharedState, VisualMode};
use crate::fft_processor::FFTProcessor;

// Tabs for the settings windowe
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
    ) -> Self {
        Self {
            shared_state,
            media_rx,
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
                state.media_info = Some(track);
                state.last_media_update = Some(Instant::now());
            }
        }
        

        // --- Main Window Size tracking ---
        let current_rect = ctx.screen_rect();
        let current_size = current_rect.size();

        // Check if size changed (ignoring tiny sub-pixel float differences)
        let size_changed = self.last_window_size.map_or(true, |old|{
            (old.x - current_size.x).abs() > 1.0 || (old.y - current_size.y).abs() > 1.0
        });

        if size_changed {
            // Filter out 0x0 or tiny screens (startup  artifacts)
            if current_size.x > 10.0 && current_size.y > 10.0 {
                // Log at INFO level so we can verify it happens
                tracing::info!("[GUI] Main Window Resized: w: {:.0}, h: {:.0}", current_size.x, current_size.y);
                self.last_window_size = Some(current_size);

                if let Ok(mut state) = self.shared_state.lock() {
                    state.config.window_size = [current_size.x, current_size.y];
                }
            }
        }

        // --- Main Window Position tracking ---
        if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            let current_pos = rect.min;

            if self.last_window_pos != Some(current_pos) {
                // Determine if we sohuld log (don't log first detection to avaoid spam on startup)
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

        // === Sonpar Pin ===
        // 1. Flash on Focus logic
        let is_focused = ctx.input(|i| i.focused);

        // Trigger flash if focus gained
        if is_focused && !self.was_focused {
            self.flash_start = Some(Instant::now());
        }
        self.was_focused = is_focused;
        
        // Calculate Animnation strength (0.0 to 1.0)
        let mut flash_strength = 0.0;
        if let Some(start) = self.flash_start {
            let elapsed = start.elapsed().as_secs_f32();
            let duration = 0.8; // slightly longer duration for the glow

            if elapsed < duration {
                // easing function: cubic out (starts fast, slows down)
                let t = 1.0 - (elapsed / duration);
                flash_strength = t.powi(3);

                ctx.request_repaint();
            } else {
                self.flash_start = None;
            }
        }

        // === 2. Acquire State and Apply Flash ===
        let (base_opacity, window_locked) = if let Ok(state) = self.shared_state.lock() {
            (state.config.background_opacity, state.config.window_locked)
        } else {
            (1.0, false) // Default to opaque on error
        };

        // 1. Boost background slightly so the window body is found  ( max + 0.2 opacity)
        let final_opacity = (base_opacity + (flash_strength * 0.2)).min(1.0);

    
        // === 3. Ghost Mode Logic === (Focus-to-Wake) ===
        // Logic:
        // - If Locked AND Transparent : we want to be a ghost (click-thru)
        // - BUT: If the user alt-tabs to us (is-focused), we must wake up so
        //        they can click the lock
        let is_transparent = base_opacity <= 0.05; // Threshold for "invisible"

        let should_passthrough = if window_locked && is_transparent{
            !is_focused // If focused, disable passthrough. If not focused, enable it
        } else {
            false // Not locked or not transparent, no passthrough
        };

        // Only send command if state changed (prevents spamming the OS Window manager)
        if should_passthrough != self.last_passthrough_state {
            let status = if should_passthrough { "GHOST MODE" } else { "INTERACTIVE" };
            tracing::info!("[GUI] Window State: {}", status);

            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(should_passthrough));
            self.last_passthrough_state = should_passthrough;
        }

        // === 4. Render Window ===
        let bg_color = egui::Color32::from_black_alpha((final_opacity * 255.0) as u8);

        // Use egui::Frame::central_panel() as the base
        // this base has window drag/resize enabled by default.
        // We just change its fill color.
        let custom_frame = egui::Frame::central_panel(&ctx.style())
            .fill(bg_color)
            .inner_margin(1.0);

        // Show the CentralPanel using the new frame
        egui::CentralPanel::default()
            .frame(custom_frame)
            .show(ctx, |ui| {
                // A. Render the main visualization content
                self.render_visualizer(ui);
                
                // B. Render Sonar Ping overlay (the glow!)
                if flash_strength > 0.0 {
                    let draw_rect = ui.max_rect().shrink(5.0);
                    self.draw_sonar_ping(ui, draw_rect, flash_strength);
                }

                // C. Render Meida Overlay
                self.render_media_overlay(ui);
                
                // D. Handle window controls (dragging and resizing)
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
        // We keep this so we can still move the window!!
        let interaction = ui.interact(rect, ui.id().with("window_drag"), 
            egui::Sense::click_and_drag());

        // Dragging moves the window
        // Use button_pressed() for instant, single-fire trigger
        // This fires ONCE exactly when the button goes down, preventing spam (stuck window)
        // and avoiding the drag threshold delay (double click issue).
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
                //    ... this ensures it pops up even if it was open but hidden
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
        let viz_data = &state.visualization;
        let perf = &state.performance;

        // 2. Allocate Drawing Space
        let available_size = ui.available_size();
        let (response, painter) = ui.allocate_painter(available_size, egui::Sense::hover());
        let rect = response.rect;

        // 3. Early exit if no data (unless in Oscope mode)
        let num_bars = viz_data.bars.len();
        if num_bars == 0 && config.visual_mode != VisualMode::Oscilloscope {
            drop(state);
            ui.centered_and_justified(|ui| {
                ui.label("⏸ Waiting for audio...");
            });
            return;
        }

        // 4. Calculate Common Layout Helpers
        // Ensure we don't eveide by zero even if bars are missing
        let bar_slot_width = rect.width() / num_bars.max(1) as f32;
        let bar_width = (bar_slot_width - config.bar_gap_px as f32).max(1.0);

        // 5. Handle mouse interactions (for frequency modes)
        let hovered_bar_index = if config.inspector_enabled && config.visual_mode != VisualMode::Oscilloscope {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                if rect.contains(pos) {
                    let relative_x = pos.x - rect.left();
                    let index = (relative_x / bar_slot_width).floor() as usize;
                    if index < num_bars {Some(index)} else { None }
                }else { None }
            }else { None }
        } else { None };

        // 6. Dispatch Drawing Strategy
        match config.visual_mode {
            VisualMode::SolidBars => {
                self.draw_solid_bars(&painter, &rect, config, viz_data, bar_width, bar_slot_width, hovered_bar_index);
            },
            VisualMode::SegmentedBars => {
                self.draw_segmented_bars(&painter, &rect, config, viz_data, bar_width, bar_slot_width, hovered_bar_index);
            },
            VisualMode::LineSpectrum => {
                self.draw_line_spectrum(&painter, &rect, config, viz_data, hovered_bar_index);
            },
            VisualMode::Oscilloscope => {
                self.draw_oscilloscope(&painter, &rect, config, viz_data);
            },
        }
        
        // 7. Draw Overlays
        if let Some(index) = hovered_bar_index {
            self.draw_inspector_overlay(&painter, &rect, config, viz_data, perf, index, bar_slot_width);
        }

        if config.show_stats {
            let perf_clone = perf.clone();
            drop(state); // Release lock before rendering stats
            self.render_stats(ui, &rect, &perf_clone);
        }
    }

    // ========== DRAWING HELPERS ==========

    fn render_media_overlay(&self, ui: &mut egui::Ui) {
        let state = self.shared_state.lock().unwrap();
        let config = &state.config;

        // 1. Check Mode
        if config.media_display_mode == MediaDisplayMode::Off {
            return;
        }

        // 2. Check Data
        let info = match &state.media_info {
            Some(i) => i, 
            None => return,
        };
        
        // 3. Calculate Opacity 
        let mut opacity = 1.0;

        if config.media_display_mode == MediaDisplayMode::FadeOnUpdate {
            if let Some(last_update) = state.last_media_update {
                let elapsed = last_update.elapsed().as_secs_f32();
                let duration = config.media_fade_duration_sec;
                let fade_time = 1.5; // Time to fully fade out after duration

                if elapsed > (duration + fade_time) {
                    return; // Fully faded out
                } else if elapsed > duration {
                    // Fading out
                    let fade_progress = (elapsed - duration) / fade_time;
                    opacity = 1.0 - fade_progress;
                }
            } else {
                // should not happen if data exists, but fail safe
                return;
            }
        }

        // 4. Draw time!
        let rect = ui.max_rect();
        // Position: Top Right, with some padding
        let pos = egui::pos2(rect.right() - 20.0, rect.top() + 20.0);

        // Use an "Area" so it floats over the specturm without pushing layout
        egui::Area::new(egui::Id::new("media_overlay"))
            .fixed_pos(pos)
            .pivot(egui::Align2::RIGHT_TOP)
            .interactable(false)
            .show(ui.ctx(), |ui| {
                ui.scope(|ui| {
                    // Apply Opacity
                    ui.visuals_mut().widgets.noninteractive.fg_stroke.color = 
                        egui::Color32::WHITE.linear_multiply(opacity);

                    // --- Font Choices
                    // Using "Heading" for Song, 'Body' for Artist looks clean and native
                    ui.label(egui::RichText::new(&info.title)
                        .font(egui::FontId::proportional(24.0))
                        .strong()
                        .color(egui::Color32::WHITE.linear_multiply(opacity))
                    );

                    ui.label(egui::RichText::new(format!("{} - {}", info.artist, info.album))
                        .font(egui::FontId::proportional(16.0))
                        .color(egui::Color32::from_white_alpha(200).linear_multiply(opacity))
                    );

                    ui.add_space(4.0);
                    
                    // Small Source app badge (eg. "Spotfiy")
                    ui.label(egui::RichText::new(format!("via {}", info.source_app))
                        .font(egui::FontId::monospace(10.0))
                        .color(egui::Color32::from_white_alpha(120).linear_multiply(opacity))
                    );
                });
            });
    }

    /// Draw solid gradient bars
    fn draw_solid_bars(
        &self, 
        painter: &egui::Painter, 
        rect: &egui::Rect, 
        config: &crate::shared_state::AppConfig, 
        data: &crate::shared_state::VisualizationData,
        bar_width: f32,
        slot_width: f32,
        hovered_index: Option<usize>,
    ) {
        let (low_c, high_c, peak_c) = config.get_colors();
        let low = to_egui_color(low_c).linear_multiply(config.bar_opacity);
        let high = to_egui_color(high_c).linear_multiply(config.bar_opacity);
        let peak = to_egui_color(peak_c).linear_multiply(config.bar_opacity);

        use egui::epaint::Vertex;

        for (i, &db) in data.bars.iter().enumerate() {
            let x = rect.left() + (i as f32 * slot_width);
            let bar_height = self.db_to_px(db, config, rect.height());
            // Safe clamp for gradient
            let norm_height = (bar_height / rect.height()).clamp(0.0, 1.0);

            // Gradient Base Color
            let mut bar_color = lerp_color(low, high, norm_height);
            if Some(i) == hovered_index {
                bar_color = lerp_color(bar_color, egui::Color32::WHITE, 0.5);
            }

            let bar_rect;
            let mesh_base;
            let mesh_tip;

            if config.inverted_spectrum {
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
            if config.inverted_spectrum {
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
            if config.show_peaks && i < data.peaks.len() {
                let peak_h = self.db_to_px(data.peaks[i], config, rect.height());
                
                let peak_rect = if config.inverted_spectrum {
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
    
    ///  Draw segmented bars helper function
    fn draw_segmented_bars(
        &self, 
        painter: &egui::Painter, 
        rect: &egui::Rect,
        config: &crate::shared_state::AppConfig, 
        data: &crate::shared_state::VisualizationData,
        bar_width: f32,
        slot_width: f32,
        hovered_index: Option<usize>,
    ) {
        let (low_c, high_c, peak_c) = config.get_colors();
        let low = to_egui_color(low_c).linear_multiply(config.bar_opacity);
        let high = to_egui_color(high_c).linear_multiply(config.bar_opacity);
        let peak = to_egui_color(peak_c).linear_multiply(config.bar_opacity);

        let seg_h = config.segment_height_px;
        let total_seg_size = seg_h + config.segment_gap_px;

        for (i, &db) in data.bars.iter().enumerate() {
            let x = rect.left() + (i as f32 * slot_width);

            // 1. Calculate active segments
            let height_px = self.db_to_px(db, config, rect.height());
            let active_segments = (height_px / total_seg_size).floor() as i32;

            // 2. Calculate peak position
            let mut peak_seg_idx = -1;
            if config.show_peaks && i < data.peaks.len() {
                let peak_px = self.db_to_px(data.peaks[i], config, rect.height());
                peak_seg_idx = (peak_px / total_seg_size).floor() as i32;
            }

            // 3. Determine how high to draw loop
            let limit = if config.fill_peaks {
                peak_seg_idx.max(active_segments)
            } else {
                active_segments
            };

            for s in 0..limit {
                let offset = s as f32 * total_seg_size;
                // Safety, don't draw outside bounds
                if offset + seg_h > rect.height() {break; }

                // Color logic: Gradient if signal, solid peak color if extension
                let color = if s < active_segments {
                    let norm_pos = (offset + (seg_h / 2.0)) / rect.height();
                    let mut c = lerp_color(low, high, norm_pos);
                    if Some(i) == hovered_index { c = lerp_color(c, egui::Color32::WHITE, 0.5); }
                    c
                } else {
                    // in "extension" zone
                    if !config.fill_peaks { continue; }
                    peak
                };

                let seg_rect = if config.inverted_spectrum {
                    egui::Rect::from_min_size(egui::pos2(x, rect.top() + offset),egui::vec2( bar_width,seg_h))
                } else {
                    egui::Rect::from_min_size(egui::pos2(x, rect.bottom() - offset - seg_h), egui::vec2(bar_width, seg_h))
                };

                painter.rect_filled(seg_rect, 1.0, color);
            }

            // 4. Floating Peak (if not filling)
            if config.show_peaks && !config.fill_peaks && peak_seg_idx >= 0 {
                // Ensure floating peak doesn't overlap active segment
                if peak_seg_idx >= active_segments {
                    let offset = peak_seg_idx as f32 * total_seg_size;
                    let seg_rect = if config.inverted_spectrum {
                        egui::Rect::from_min_size(egui::pos2(x, rect.top() + offset), egui::vec2(bar_width, seg_h))
                    } else {
                        egui::Rect::from_min_size(egui::pos2(x, rect.bottom() - offset - seg_h), egui::vec2(bar_width, seg_h))
                    };
                    painter.rect_filled(seg_rect, 1.0, peak);
                }
            }
        }
    }


    fn draw_line_spectrum(
        &self, 
        painter: &egui::Painter, 
        rect: &egui::Rect, 
        config: &crate::shared_state::AppConfig, 
        data: &crate::shared_state::VisualizationData,
        hovered_index: Option<usize>,
    ) {
        if data.bars.is_empty() { return;}

        // Pre-calculate points
        let points: Vec<egui::Pos2> = data.bars.iter().enumerate().map(|(i, &db)| {
            let x = rect.left() + (i as f32 / data.bars.len() as f32) * rect.width();
            let height = self.db_to_px(db, config, rect.height());
        
            let y = if config.inverted_spectrum {
                rect.top() + height
            } else {
                rect.bottom() - height
            };

            egui::pos2(x, y)
        }).collect();
            
        // Draw Glow (thick transparent line)
        let (_, high, _) = config.get_colors();
        let glow_c = to_egui_color(high).linear_multiply(0.3);
        painter.add(egui::Shape::line(points.clone(), egui::Stroke::new(2.0, glow_c)));

        // Draw Core (thin bright line)
        let core_c = to_egui_color(high);
        painter.add(egui::Shape::line(points.clone(), egui::Stroke::new(2.0, core_c)));

        // Draw hover Indicator
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
        config: &crate::shared_state::AppConfig, 
        data: &crate::shared_state::VisualizationData,
    ) {
        if data.waveform.is_empty() { return; }
    
        let center_y = rect.center().y;
        // Scale: Audio is +/- 1.0, we map that to +/- half height
        // Sensitivity scales the amplitude
        let scale = (rect.height() / 2.0 ) * config.sensitivity;

        // Downsampling for performance if buffer is huge
        // Just drawing every Nth sample or average could work, but simple stride is fast
        let step_x = rect.width() / (data.waveform.len() as f32 - 1.0);

        let points: Vec<egui::Pos2> = data.waveform.iter().enumerate().map(|(i, &sample)| {
            let x = rect.left() + (i as f32 * step_x);
            let y = center_y - (sample.clamp(-1.0, 1.0) * scale);
            egui::pos2(x, y)
        }).collect();
        
        let (_, high, _) = config.get_colors();
        let color = to_egui_color(high);
        painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, color)));
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

        // Only show if background is transparent, otherwise its confusing
        if state.config.background_opacity > 0.05 {
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
        let (_, high_color, _) = self.shared_state.lock().unwrap().config.get_colors();
        let base_color = to_egui_color(high_color);
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
        config: &crate::shared_state::AppConfig, 
        data: &crate::shared_state::VisualizationData,
        perf: &crate::shared_state::PerformanceStats,
        index: usize,
        slot_width: f32,
    ) {

        // Crosshair
        let center_x = rect.left() + (index as f32 * slot_width) + (slot_width / 2.0);
        painter.line_segment(
            [egui::pos2(center_x, rect.top()), egui::pos2(center_x, rect.bottom())],
            egui::Stroke::new(1.0, egui::Color32::WHITE.linear_multiply(0.5))
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
        painter.rect_filled(label_rect, 4.0,
             egui::Color32::from_black_alpha((config.inspector_opacity * 255.0) as u8));
        painter.rect_stroke(label_rect, 4.0, 
            egui::Stroke::new(1.0, egui::Color32::WHITE.linear_multiply(config.inspector_opacity)));
        painter.galley(label_rect.min + egui::vec2(padding, padding), galley, egui::Color32::WHITE);
    }
    
    /// Render performance statitstics overlay
    fn render_stats(&self, ui: &mut egui::Ui, rect: &egui::Rect, perf: &crate::shared_state::PerformanceStats){
        let state = match self.shared_state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };

        let config = &state.config;

        let info = &perf.fft_info;

        // Use strict padding:
        // {:>5.1} -> Right aligned, 5 chars wide, 1 decimal point (e.g. " 60.0" or "144.1")
        // {:>5.2} -> Right aligned, 5 chars wide, 2 decimal points (e.g. " 1.25")
        // {:>6}   -> Right aligned, 6 chars wide integer
        let stats_text = format!(
            "FPS: {:>5.1} | Lat: {:>4.1}ms | Res: {:>4.1}Hz | {}Hz",
            perf.gui_fps,
            info.latency_ms,
            info.frequency_resolution,
            info.sample_rate,
        );

        let text_color = egui::Color32::WHITE.linear_multiply(config.stats_opacity);
        
        ui.painter().text(
            egui::pos2(rect.left() + 10.0, rect.top() + 10.0),
            egui::Align2::LEFT_TOP,
            stats_text,
            egui::FontId::monospace(12.0),
            text_color,
        );
    }

    /// Render settings window content
    fn render_settings_window(&mut self, ui: &mut egui::Ui) {
        let mut state = match self.shared_state.lock() {
            Ok(state) => state,
            Err(_) => {
                ui.label("⚠ Error: Cannot access settings");
                return;
            }
        };

        // Define a standard grid spacing for consistency
        let grid_spacing = egui::vec2(40.0, 12.0); 

        // 1. Tab Bar (Add some padding around it)
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            // Get the highlight color (High Color from config)
            let (_, high, _) = state.config.get_colors();
            let highlight = to_egui_color(high);

            ui_tab_button(ui, " 🎨 Visual ", SettingsTab::Visual, &mut self.active_tab, highlight);
            ui_tab_button(ui, " 🔊 Audio ", SettingsTab::Audio, &mut self.active_tab, highlight);
            ui_tab_button(ui, " 🌈 Colors ", SettingsTab::Colors, &mut self.active_tab, highlight);
            ui_tab_button(ui, " 🪟 Window ", SettingsTab::Window, &mut self.active_tab, highlight);
            ui_tab_button(ui, " 📊 Stats ", SettingsTab::Performance, &mut self.active_tab, highlight);
        });
        ui.add_space(5.0);
        ui.separator();
        ui.add_space(10.0);

        // 2. Tab Content
        egui::ScrollArea::vertical().show(ui, |ui| {
            match self.active_tab {
                SettingsTab::Visual => {
                    ui.heading("Visual Configuration");
                    ui.add_space(5.0);
                    
                    ui.group(|ui| {
                        egui::Grid::new("visual_grid")
                            .num_columns(2)
                            .spacing(grid_spacing)
                            .striped(true)
                            .show(ui, |ui| {
                                
                                // Visual Mode Selector
                                ui.label("Mode");
                                egui::ComboBox::from_id_salt("viz_mode")
                                    .selected_text(format!("{:?}", state.config.visual_mode))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut state.config.visual_mode, VisualMode::SolidBars, "Solid Bars");
                                        ui.selectable_value(&mut state.config.visual_mode, VisualMode::SegmentedBars, "Segmented (LED)");
                                        ui.selectable_value(&mut state.config.visual_mode, VisualMode::LineSpectrum, "Line Spectrum");
                                        ui.selectable_value(&mut state.config.visual_mode, VisualMode::Oscilloscope, "Oscilloscope");
                                    });
                                ui.end_row();

                                // Controls specific to Spectrum Modes
                                if state.config.visual_mode != VisualMode::Oscilloscope {
                                    ui.label("Bar Count");
                                    ui.add(egui::Slider::new(&mut state.config.num_bars, 10..=512)
                                        .step_by(1.0)
                                        .drag_value_speed(1.0)
                                        .smart_aim(false));
                                    ui.end_row();

                                    ui.label("Bar Gap");
                                    ui.add(egui::Slider::new(&mut state.config.bar_gap_px, 0..=10).suffix(" px"));
                                    ui.end_row();
                                }

                                ui.label("Bar Opacity");
                                ui.add(egui::Slider::new(&mut state.config.bar_opacity, 0.0..=1.0));
                                ui.end_row();

                                ui.label("Background Opacity");
                                ui.add(egui::Slider::new(&mut state.config.background_opacity, 0.0..=1.0));
                                ui.end_row();

                                // Segmented Mode Options
                                if state.config.visual_mode == VisualMode::SegmentedBars {
                                    ui.label("Segment Height");
                                    ui.add(egui::Slider::new(&mut state.config.segment_height_px, 1.0..=20.0).suffix(" px"));
                                    ui.end_row();

                                    ui.label("Segment Gap");
                                    ui.add(egui::Slider::new(&mut state.config.segment_gap_px, 0.0..=10.0).suffix(" px"));
                                    ui.end_row();
                                }

                                // Peaks (Disable for O-scope)
                                if state.config.visual_mode != VisualMode::Oscilloscope {
                                    ui.label("Peak Indicators");
                                    ui.horizontal(|ui| {
                                        ui.checkbox(&mut state.config.show_peaks, "Show");
                                        if state.config.show_peaks && state.config.visual_mode == VisualMode::SegmentedBars {
                                            ui.checkbox(&mut state.config.fill_peaks, "Fill to Peak");
                                        }
                                    });
                                    ui.end_row();
                                }
                            });
                    });

                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.label("Aggregation Mode:");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut state.config.use_peak_aggregation, true, "Peak (Dramatic)");
                            ui.radio_value(&mut state.config.use_peak_aggregation, false, "Average (Smooth)");
                        });

                        ui.add_space(5.0);
                        ui.label("Orientation:");
                        ui.checkbox(&mut state.config.inverted_spectrum, "Inverted (Top-Down)");
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
                               ui.add(
                                    egui::Slider::new(&mut state.config.sensitivity, 0.01..=100.0)
                                        .logarithmic(true)
                                        .custom_formatter(|v, _| format!("{:+.1} dB", 20.0 * v.log10()))
                                        .custom_parser(|s| {
                                            // Parse "+6 dB" style input
                                            s.trim().trim_end_matches(" dB").trim_end_matches("dB")
                                                .parse::<f64>().ok()
                                                .map(|db| 10_f64.powf(db / 20.0))
                                        }),
                                );
                                
                                
                                ui.end_row();

                                ui.label("Noise Floor");
                                ui.add(egui::Slider::new(&mut state.config.noise_floor_db, -120.0..=-20.0)
                                    .suffix(" dB"));
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
                                ui.add(egui::Slider::new(&mut state.config.attack_time_ms, 1.0..=500.0).suffix(" ms"));
                                ui.end_row();

                                ui.label("Bar Release (Fall)");
                                ui.add(egui::Slider::new(&mut state.config.release_time_ms, 1.0..=2000.0).suffix(" ms"));
                                ui.end_row();

                                if state.config.show_peaks {
                                    ui.label("Peak Hold Time");
                                    ui.add(egui::Slider::new(&mut state.config.peak_hold_time_ms, 0.0..=2000.0).suffix(" ms"));
                                    ui.end_row();

                                    ui.label("Peak Fall Speed");
                                    ui.add(egui::Slider::new(&mut state.config.peak_release_time_ms, 10.0..=2000.0).suffix(" ms"));
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
                                    // Clone data to avoid holding lock while drawing complex UI
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
                    ui.heading("Color Scheme");
                    ui.add_space(5.0);

                    let current_scheme = state.config.scheme_name();
                    
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Active Preset:");
                            egui::ComboBox::from_id_salt("color_combo")
                                .selected_text(&current_scheme)
                                .height(300.0)
                                .show_ui(ui, |ui| {
                                    let presets = crate::shared_state::ColorPreset::preset_names();
                                    for preset_name in presets {
                                        if ui.selectable_label(current_scheme == preset_name, &preset_name).clicked() {
                                            state.config.apply_preset(&preset_name);
                                        }
                                    }
                                });
                        });
                    });
                    
                    ui.add_space(10.0);
                    ui.label("Preview:");
                    
                    // 1. Setup the drawing area
                    let height = 60.0; // Taller to see the gradient better
                    let (response, painter) = ui.allocate_painter(
                        egui::vec2(ui.available_width(), height), 
                        egui::Sense::hover()
                    );
                    let rect = response.rect;

                    // 2. Draw Background (using opacity)
                    let bg_color = egui::Color32::from_black_alpha((state.config.background_opacity * 255.0) as u8);
                    painter.rect_filled(rect, 4.0, bg_color);

                    // 3. Define a "Mock" Audio Signal (0.0 to 1.0)
                    let mock_bars = [
                        // Bass (Heavy)
                        0.10, 0.40, 0.75, 0.95, 0.90, 0.85, 0.70, 
                        // Low Mids (Dip)
                        0.55, 0.40, 0.30, 0.25, 
                        // High Mids (Vocal Peak)
                        0.40, 0.60, 0.50, 0.35, 
                        // Highs (Sparkle & Roll off)
                        0.25, 0.15, 0.25, 0.40, 0.30, 0.20, 0.15, 0.10, 0.08, 0.04, 0.01
                    ];
                    
                    let (low_rgb, high_rgb, peak_rgb) = state.config.get_colors();
                    let low = to_egui_color(low_rgb).linear_multiply(state.config.bar_opacity);
                    let high = to_egui_color(high_rgb).linear_multiply(state.config.bar_opacity);
                    let peak = to_egui_color(peak_rgb).linear_multiply(state.config.bar_opacity);

                    let bar_width = rect.width() / mock_bars.len() as f32;
                    let gap = 2.0;

                    // 4. Render the Mock Bars
                    for (i, &level) in mock_bars.iter().enumerate() {
                        let x = rect.left() + (i as f32 * bar_width) + gap/2.0;
                        let w = bar_width - gap;
                        
                        // Calculate height in pixels
                        let h = level * rect.height();

                        // Interpolate bar color based on height (just like the real thing)
                        let bar_color = lerp_color(low, high, level);

                        let bar_rect;
                        let mesh_base;
                        let mesh_tip;
                        let peak_y;

                        if state.config.inverted_spectrum {
                            // Top-down
                            bar_rect = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, h));
                            peak_y = bar_rect.bottom() + 2.0;
                            mesh_base = low;
                            mesh_tip = bar_color;
                        } else {
                            // Bottom-up (Standard)
                            bar_rect = egui::Rect::from_min_size(egui::pos2(x, rect.bottom() - h), egui::vec2(w, h));
                            peak_y = bar_rect.top() - 4.0; // Slightly above bar
                            mesh_base = low;
                            mesh_tip = bar_color;
                        }

                        // Draw Bar Gradient
                        use egui::epaint::{Mesh, Vertex};
                        let mut mesh = Mesh::default();
                        
                        if state.config.inverted_spectrum {
                            mesh.vertices.push(Vertex { pos: bar_rect.left_top(), uv: egui::Pos2::ZERO, color: mesh_base });
                            mesh.vertices.push(Vertex { pos: bar_rect.right_top(), uv: egui::Pos2::ZERO, color: mesh_base });
                            mesh.vertices.push(Vertex { pos: bar_rect.right_bottom(), uv: egui::Pos2::ZERO, color: mesh_tip });
                            mesh.vertices.push(Vertex { pos: bar_rect.left_bottom(), uv: egui::Pos2::ZERO, color: mesh_tip });
                        } else {
                            mesh.vertices.push(Vertex { pos: bar_rect.left_bottom(), uv: egui::Pos2::ZERO, color: mesh_base });
                            mesh.vertices.push(Vertex { pos: bar_rect.right_bottom(), uv: egui::Pos2::ZERO, color: mesh_base });
                            mesh.vertices.push(Vertex { pos: bar_rect.right_top(), uv: egui::Pos2::ZERO, color: mesh_tip });
                            mesh.vertices.push(Vertex { pos: bar_rect.left_top(), uv: egui::Pos2::ZERO, color: mesh_tip });
                        }
                        
                        mesh.add_triangle(0, 1, 2);
                        mesh.add_triangle(0, 2, 3);
                        painter.add(egui::Shape::mesh(mesh));

                        // Draw Peak
                        // Only draw peak if the bar isn't basically empty
                        if level > 0.05 {
                            let peak_rect = egui::Rect::from_min_size(
                                egui::pos2(x, peak_y), 
                                egui::vec2(w, 2.0)
                            );
                            painter.rect_filled(peak_rect, 0.0, peak);
                        }
                    }
                    
                    ui.add_space(5.0);
                    ui.small(format!("Peak Color: RGB({}, {}, {})", peak_rgb.r, peak_rgb.g, peak_rgb.b));
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

                                if state.config.inspector_enabled {
                                    ui.label("Inspector Opacity");
                                    ui.add(egui::Slider::new(&mut state.config.inspector_opacity, 0.1..=1.0));
                                    ui.end_row();
                                }

                                // === Media Settings ===
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

                    ui.add_space(10.0);
                    ui.heading("On-Screen Display");
                    ui.group(|ui| {
                        egui::Grid::new("osd_grid")
                            .num_columns(2)
                            .spacing(grid_spacing)
                            .show(ui, |ui| {
                                ui.label("Performance Stats");
                                ui.checkbox(&mut state.config.show_stats, "Visible");
                                ui.end_row();

                                if state.config.show_stats {
                                    ui.label("Text Opacity");
                                    ui.add(egui::Slider::new(&mut state.config.stats_opacity, 0.1..=1.0));
                                    ui.end_row();
                                }
                            });
                    });
                },

                SettingsTab::Performance => {
                    ui.heading("Diagnostics");
                    ui.add_space(5.0);
                    
                    let info = &state.performance.fft_info;
                    
                    ui.group(|ui| {
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
            }
        });
    }  

    // == Helper Functions ==
    fn db_to_px(&self, db: f32, config: &crate::shared_state::AppConfig, max_height: f32) -> f32 {
        let floor = config.noise_floor_db;
        let range = (0.0 - floor).max(1.0);
        let normalized = ((db - floor) / range).clamp(0.0, 1.0);
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
#[cfg(test)]
mod tests {
    use super::*;

    use crate::shared_state::AppConfig;

    #[test]
    fn test_db_to_px_scaling() {
        let app = SpectrumApp::new(
            Arc::new(Mutex::new(SharedState::new())),
            crossbeam_channel::bounded(1).1 // Dummy channel for test
        );
        let config = AppConfig { noise_floor_db: -100.0, ..Default::default() };
        
        // Test Floor
        assert_eq!(app.db_to_px(-100.0, &config, 500.0), 0.0);
        // Test Max
        assert_eq!(app.db_to_px(0.0, &config, 500.0), 500.0);
        // Test Middle (-50dB is half of -100dB range)
        assert_eq!(app.db_to_px(-50.0, &config, 500.0), 250.0);
    }


    #[test]
    fn test_lerp_color() {
        let c1 = egui::Color32::from_rgb(0, 0, 0);       // Black
        let c2 = egui::Color32::from_rgb(200, 100, 50); // Orange-ish
        
        let res = lerp_color(c1, c2, 0.5);
        
        assert_eq!(res.r(), 100);
        assert_eq!(res.g(), 50);
        assert_eq!(res.b(), 25);
    }

}
