use eframe:: egui;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::shared_state::{SharedState, Color32 as StateColor32};

// Main Application GUI - handles rendering and user interaction
pub struct SpectrumApp {

    /// shared state between FFT and GUI threads
    shared_state: Arc<Mutex<SharedState>>,
    
    /// Settings window state
    settings_open: bool,

    /// Performance tracking
    last_frame_time :  Instant, 
    frame_times: Vec<f32>,
}

impl SpectrumApp {
    pub fn new(shared_state: Arc<Mutex<SharedState>>) -> Self {
        Self {
            shared_state,
            settings_open: false,
            last_frame_time: Instant::now(),
            frame_times: Vec::with_capacity(60),
        }
    }
}

impl eframe::App for SpectrumApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

        // === Building custom frame to manage window resizing and movement
        
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
        let custom_frame = egui::Frame::central_panel(&ctx.style()).fill(bg_color);

        // Show the CentralPanel using the new frame
        egui::CentralPanel::default()
            .frame(custom_frame)
            .show(ctx, |ui| {
                self.render_visualizer(ui);
            });
        
        // Top Menu bar (context menu alternative)
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            ui.menu_button("☰ Menu", |ui| {
                if ui.button("⚙ Settings").clicked() {
                    self.settings_open = true;
                    ui.close_menu();
                }

                if ui.button("❌ Exit").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

        // Settings Window (non-blocking)
        let mut show_settings = self.settings_open;
        if show_settings {
            egui::Window::new("⚙ Settings")
                .open(&mut show_settings)
                .default_width(400.0)
                .resizable(false)
                .show(ctx, |ui| {
                    self.render_settings(ui);
                });
        }
        self.settings_open = show_settings;
    }
}

impl SpectrumApp {
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

            painter.rect_filled(bar_rect, 0.0, bar_color);

            // Draw peak indicator if enabled
            if config.show_peaks && i < viz_data.peaks.len() {
                let peak_height_db = viz_data.peaks[i];
                let peak_normalized = ((peak_height_db + 60.0) / 60.0).clamp(0.0, 1.0);
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
        let stats_text = format!(
            "FPS: {:.1} | FFT: {:.2?} | Frames: {} | Bars: {}",
            perf.gui_fps,
            perf.fft_ave_time,
            perf.frame_count,
            config.num_bars,
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
    fn render_settings(&mut self, ui: &mut egui::Ui) {
        let mut state = match self.shared_state.lock() {
            Ok(state) => state,
            Err(_) => {
                ui.label("⚠ Error: Cannot access settings");
                return;
            }
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            // --- VISUAL SETTINGS ---
            ui.collapsing("🎨 Visual Settings", |ui| {
                ui.add(
                    egui::Slider::new(&mut state.config.num_bars, 16..=512)
                        .text("Number of Bars")
                );
                
                ui.add(
                    egui::Slider::new(&mut state.config.bar_gap_px, 0..=10)
                        .text("Bar Gap (px)")
                );

                ui.add(
                    egui::Slider::new(&mut state.config.bar_opacity, 0.0..=1.0)
                        .text("Bar Opacity")
                );

                ui.add(
                    egui::Slider::new(&mut state.config.background_opacity, 0.0..=1.0)
                        .text("Background Opacity")
                );

                ui.checkbox(&mut state.config.show_peaks, "Show Peaks Indicators");
            });

            // === AUDIO SETTINGS ===
            ui.collapsing("🔊 Audio Settings", |ui| {
                ui.label("FFT Size:");
                ui.horizontal(|ui| {
                    for &size in &[512, 1024, 2048, 4096] {
                        if ui.selectable_label(state.config.fft_size == size, format!("{}", size)).clicked() {
                            state.config.fft_size = size;
                        }
                    }
                });

                ui.add(
                    egui::Slider::new(&mut state.config.sensitivity, 0.1..=10.0)
                        .text("Sensitivity")
                );

                ui.add(
                    egui::Slider::new(&mut state.config.noise_floor_db,-120.0..=-20.0)
                        .text("Noise Floor (dB)")
                        .suffix(" dB")
                );

                ui.add(
                    egui::Slider::new(&mut state.config.attack_time_ms, 1.0..=500.0)
                        .text("Attack Time (ms)")
                );

                ui.add(
                    egui::Slider::new(&mut state.config.release_time_ms, 1.0..=2000.0)
                        .text("Release Time (ms)")
                );

                if state.config.show_peaks {
                    ui.add(
                        egui::Slider::new(&mut state.config.peak_hold_time_ms, 0.0..=2000.0)
                            .text("Peak Hold Time (ms)")
                    );

                    ui.add(
                        egui::Slider::new(&mut state.config.peak_release_time_ms, 10.0..=2000.0)
                        .text("Peak Release Time (ms)")
                    );
                }
            });
            
            // === COLOR SETTINGS ===
            ui.collapsing("🎨 Color Settings", |ui| {
                let current_scheme = state.config.scheme_name();

                egui::ComboBox::from_label("Color Scheme")
                    .selected_text(&current_scheme)
                    .show_ui(ui, |ui| {
                        let presets = crate::shared_state::ColorPreset::preset_names();

                        for preset_name in presets {
                            if ui.selectable_label(&current_scheme == &preset_name, &preset_name).clicked() {
                                state.config.apply_preset(&preset_name);
                            }
                        }

                        ui.separator();

                        if ui.selectable_label(&current_scheme == "Rainbow", "Rainbow").clicked() {
                            state.config.color_scheme = crate::shared_state::ColorScheme::Rainbow;
                        }
                    });
                
                ui.separator();

                // Custom color pickers (simplified - full color picker would need additional widgth)
                ui.label("Custom Colors:");
                let (low, high, peak) = state.config.get_colors();

                ui.label(format!("Low: RGB({}, {}, {})", low.r, low.g, low.b));
                ui.label(format!("High: RGB({}, {}, {})", high.r, high.g, high.b));
                ui.label(format!("Peak: RGB({}, {}, {})", peak.r, peak.g, peak.b));

                ui.label("💡 Tip: Use presets or edit manually in code");
            });

            // === WINDOW SETTINGSF ===
            ui.collapsing("🪟 Window Settings", |ui| {
                ui.checkbox(&mut state.config.always_on_top, "Always on Top");
                ui.checkbox(&mut state.config.window_decorations, "Show Title Bar");
                ui.checkbox(&mut state.config.show_stats, "Show Performance Stats");

                if state.config.show_stats {
                    ui.add(
                        egui::Slider::new(&mut state.config.stats_opacity, 0.0..=1.0)
                            .text("Stats Opacity")
                    );
                }
            });

            // === PERFORMANCE INFO ===
            ui.collapsing("📊 Performance Info", |ui| {
                ui.label(format!("FFT Size: {} samples", state.config.fft_size));
                ui.label(format!("Frequency Resolution: {:.2} Hz/bin", state.config.frequency_resolution()));
                ui.label(format!("FFT Latency: {:.2} ms", state.config.fft_latency_ms()));
                ui.label(format!("Current FPS: {:.1}", state.performance.gui_fps));
            });
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


