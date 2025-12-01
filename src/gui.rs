use eframe:: egui;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::fft_config::FIXED_FFT_SIZE;
use crate::shared_state::{SharedState, Color32 as StateColor32};

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
    
    /// Settings window state
    settings_open: bool,

    /// Current active settings tab
    active_tab: SettingsTab,

    /// Performance tracking
    last_frame_time :  Instant, 
    frame_times: Vec<f32>,

    /// Track window size to only log changes
    last_window_size: Option<egui::Vec2>,
}

impl SpectrumApp {
    pub fn new(shared_state: Arc<Mutex<SharedState>>) -> Self {
        Self {
            shared_state,
            settings_open: false,
            active_tab: SettingsTab::Visual,
            last_frame_time: Instant::now(),
            frame_times: Vec::with_capacity(60),
            last_window_size: None,
        }
    }

    /// A custom "Pill" style tab button with animations and theme integration
    fn ui_tab_button(
        &self,
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
}

impl eframe::App for SpectrumApp {

    /// Tell eframe to clear the window with total transparency
    /// this alllows the OS background to show through when our CentralPanel
    /// is also transparent.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Return RGBA array directly [Red, Green, Blue, Alpha]
        [0.0, 0.0, 0.0, 0.0] // Fully transparent   
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        
        // --- Main Window Size tracking ---
        let current_size = ctx.available_rect().size();

        // Only print if the size has changed since the last fraome (or is None)
        if self.last_window_size != Some(current_size) {
            println!("[GUI] Main Window Resized: {:.0} x {:.0}", current_size.x, current_size.y);
            self.last_window_size = Some(current_size);
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
        
        // Grab the background opacity from the shared state
        let bg_opacity = if let Ok(state) = self.shared_state.lock() {
            state.config.background_opacity
        } else {
            1.0 // Default to opaque on error
        };

        // Create the custom frame for the centralPanel
        // This frame will draw the background and handle window interactions
        let bg_color = egui::Color32::from_black_alpha((bg_opacity * 255.0) as u8);

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
                // 1. Render the main visualization content
                self.render_visualizer(ui);
                // 2. Handle window controls (dragging and resizing)
                self.window_controls(ctx, ui);
            });
        
        //  === SETTINGS WINDOW (Separate Viewport) ===
        if self.settings_open {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("settings_viewport"),
                egui::ViewportBuilder::default()
                    .with_title("BeAnal Settings")
                    .with_inner_size([450.0, 500.0])
                    .with_resizable(false)
                    .with_maximize_button(false)
                    .with_always_on_top(),
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
    fn window_controls(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        // use max_rect to get the area of everyhting drawn so far (the whole window!)
        let rect = ui.max_rect();

        // 1. Handle Window Movement (Dragging the background)
        // We keep this so we can still move the window!!
        let interaction = ui.interact(rect, ui.id().with("window_drag"), 
            egui::Sense::click_and_drag());

        // Dragging moves the window
        if interaction.dragged() {
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
                ui.close_menu();
            }

            ui.separator();

            if ui.button("❌ Exit").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        // 2. Lower-Right Resize Grip
        let corner_size = 20.0; // Size of the resize handle area

        // Calculate the rectangel for the bottom-right corner
        let grip_rect = egui::Rect::from_min_size(
        egui::pos2(rect.right() - corner_size, rect.bottom() - corner_size),
        egui::Vec2::splat(corner_size)
        );

        let response = ui.interact(grip_rect, ui.id()
            .with("resize_grip"), egui::Sense::drag());

        if response.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeSouthEast);
        }

        if response.dragged() {
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

    /// Render the main spectrum visualizer
    fn render_visualizer(&mut self, ui: &mut egui::Ui) {
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

        // Get drawing area
        let available_size = ui.available_size();
        let (response, painter) = ui.allocate_painter(available_size, egui::Sense::hover());
        let rect = response.rect;

        // Calculate bar dimensions
        let num_bars = viz_data.bars.len();
        if num_bars == 0 {
            drop(state);
            ui.centered_and_justified(|ui| {
                ui.label("⏸ Waiting for audio...");
            });
            return;
        }

        let bar_gap = config.bar_gap_px as f32;
        let total_width = rect.width();
        let bar_slot_width = total_width / num_bars as f32;
        let bar_width = (bar_slot_width - bar_gap).max(1.0);

        // Get Colors
        let (low_color, high_color, peak_color) = config.get_colors();
        let low = to_egui_color(low_color).linear_multiply(config.bar_opacity);
        let high = to_egui_color(high_color).linear_multiply(config.bar_opacity);
        let peak = to_egui_color(peak_color).linear_multiply(config.bar_opacity);

        // Draw Bars
        for (i, &bar_height_db) in viz_data.bars.iter().enumerate() {
            let x = rect.left() + (i as f32 * bar_slot_width);

            let floor_db = config.noise_floor_db;
            let db_range = (0.0 - floor_db).max(1.0); // defensive max(1.0) to garentee no div/zero later

            // Map dB (-60 to 0) to screen height (0 to 1)
            let normalized_height = ((bar_height_db - floor_db) / db_range).clamp(0.0, 1.0);
            let bar_height_px = normalized_height * rect.height();

            // Calculate bar color (gradient from low to high)
            let bar_color = lerp_color(low, high, normalized_height);

            // Draw bar from bottom
            let bar_rect = egui::Rect::from_min_max(
                egui::pos2(x, rect.bottom() - bar_height_px),
                egui::pos2(x + bar_width, rect.bottom()),
            );

            use egui::epaint::Vertex;
            let mut mesh = egui::Mesh::default();



            // MESH GRADIENT
            // Define the 4 corners of the bar
            // Bootom vertices uses the 'low' color
            // Top vertices uses the 'high' color
            // Connnect vertices to form two triangles (0-1-2 and 0-2-3)
            // Add it to the painter
            mesh.vertices.push(Vertex {pos: bar_rect.left_bottom(), uv: egui::Pos2::ZERO, color: low});
            mesh.vertices.push(Vertex {pos: bar_rect.right_bottom(),uv: egui::Pos2::ZERO, color: low,});
            mesh.vertices.push(Vertex {pos: bar_rect.right_top(), uv: egui::Pos2::ZERO, color: bar_color,});
            mesh.vertices.push(Vertex {pos: bar_rect.left_top(), uv: egui::Pos2::ZERO, color: bar_color,});
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(egui::Shape::mesh(mesh));

            // Draw peak indicator if enabled
            if config.show_peaks && i < viz_data.peaks.len() {
                let peak_height_db = viz_data.peaks[i];
                let peak_normalized = ((peak_height_db - floor_db) / db_range).clamp(0.0, 1.0);
                let peak_y = rect.bottom() - (peak_normalized * rect.height());

                let peak_rect = egui::Rect::from_min_max(
                    egui::pos2(x, peak_y - 2.0),
                    egui::pos2(x + bar_width, peak_y),
                );

                painter.rect_filled(peak_rect, 0.0, peak);

            }
        }

        // Draw performance stats if enabled
        if config.show_stats {
            let perf_clone = perf.clone();
            drop(state);  // Release lock before rendering text
            self.render_stats(ui, &rect, &perf_clone);
        }
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
            ui.selectable_value(&mut self.active_tab, SettingsTab::Visual, " 🎨 Visual ");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Audio, " 🔊 Audio ");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Colors, " 🌈 Colors ");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Window, " 🪟 Window ");
            ui.selectable_value(&mut self.active_tab, SettingsTab::Performance, " 📊 Stats ");
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
                    
                    // Use a Group for a "Card" look
                    ui.group(|ui| {
                        egui::Grid::new("visual_grid")
                            .num_columns(2)
                            .spacing(grid_spacing)
                            .striped(true) // Subtle alternating row colors (very spreadsheet/windows like)
                            .show(ui, |ui| {
                                
                                ui.label("Bar Count");
                                ui.add(egui::Slider::new(&mut state.config.num_bars, 16..=512));
                                ui.end_row();

                                ui.label("Bar Gap");
                                ui.add(egui::Slider::new(&mut state.config.bar_gap_px, 0..=10).suffix(" px"));
                                ui.end_row();

                                ui.label("Bar Opacity");
                                ui.add(egui::Slider::new(&mut state.config.bar_opacity, 0.0..=1.0));
                                ui.end_row();

                                ui.label("Background Opacity");
                                ui.add(egui::Slider::new(&mut state.config.background_opacity, 0.0..=1.0));
                                ui.end_row();

                                ui.label("Peak Indicators");
                                ui.checkbox(&mut state.config.show_peaks, "Enabled");
                                ui.end_row();
                            });
                    });

                    ui.add_space(10.0);
                    ui.group(|ui| {
                        ui.label("Aggregation Mode:");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut state.config.use_peak_aggregation, true, "Peak (Dramatic)");
                            ui.radio_value(&mut state.config.use_peak_aggregation, false, "Average (Smooth)");
                        });
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
                                ui.add(egui::Slider::new(&mut state.config.sensitivity, 0.01..=3.0)
                                    .logarithmic(true));
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
                                    ui.separator();
                                    if ui.selectable_label(current_scheme == "Rainbow", "Rainbow").clicked() {
                                        state.config.color_scheme = crate::shared_state::ColorScheme::Rainbow;
                                    }
                                });
                        });
                    });

                    ui.add_space(10.0);
                    ui.label("Preview:");
                    let (low, high, peak) = state.config.get_colors();
                    
                    // Draw a simple gradient preview
                    let rect = ui.available_rect_before_wrap(); 
                    let preview_height = 30.0;
                    let preview_rect = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), preview_height));
                    ui.allocate_rect(preview_rect, egui::Sense::hover());
                    
                    // Paint gradient
                    use egui::epaint::{Mesh, Vertex};
                    let mut mesh = Mesh::default();
                    let l = to_egui_color(low);
                    let h = to_egui_color(high);
                    
                    mesh.vertices.push(Vertex { pos: preview_rect.left_bottom(), uv: egui::Pos2::ZERO, color: l });
                    mesh.vertices.push(Vertex { pos: preview_rect.right_bottom(), uv: egui::Pos2::ZERO, color: h });
                    mesh.vertices.push(Vertex { pos: preview_rect.right_top(), uv: egui::Pos2::ZERO, color: h });
                    mesh.vertices.push(Vertex { pos: preview_rect.left_top(), uv: egui::Pos2::ZERO, color: l });
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(0, 2, 3);
                    ui.painter().add(egui::Shape::mesh(mesh));
                    
                    ui.add_space(10.0);
                    ui.small(format!("Peak Color: RGB({}, {}, {})", peak.r, peak.g, peak.b));
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
                                ui.checkbox(&mut state.config.always_on_top, "Always on Top");
                                ui.end_row();

                                ui.label("Decorations");
                                if ui.checkbox(&mut state.config.window_decorations, "Show Title Bar").changed() {
                                    let show = state.config.window_decorations;
                                    ui.ctx().send_viewport_cmd_to(
                                        egui::ViewportId::ROOT,
                                        egui::ViewportCommand::Decorations(show));
                                }
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


