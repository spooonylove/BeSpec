use eframe::egui::{self, Ui, Rect, Context, Sense, Color32};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::shared_state::{SharedState};
use crate::shared_state::{ColorProfile, MediaDisplayMode, VisualMode, VisualProfile};
use crate::shared_state::ColorRef;use crate::media::MediaController;
use crate::gui::{theme::*, visualizers};
use crate::fft_config::FIXED_FFT_SIZE;

/// Settings Tab Definition
#[derive(PartialEq, Debug)]
pub enum SettingsTab {
    Visual, 
    Audio,
    Colors,
    Window,
    Performance,
}

/// Save Dialog Box Types
#[derive(PartialEq)]
pub enum SaveTarget {
    None,
    Visual,
    Color,
}

// =======================================================================================
// WINDOW CONTROLS  (Drag, Resize, Lock)
// =======================================================================================

/// Handle high-level windo interaction:
/// 1. Dragging (primary click)
/// 2. Maximize/Restore (double click)
/// 3. Settings Menu (right click)
pub fn handle_window_interaction(
    ui: &mut Ui,
    ctx: &Context,
    rect: Rect,
    settings_open: &mut bool
){
    // We use "window_bg" as the ID to represent the background layer interaction
    let interaction = ui.interact(rect, ui.id().with("window_bg_interaction"), Sense::click());

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
    if interaction.hovered() && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)){
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }

    // 2. Double-clicking toggles Maximize
    if interaction.double_clicked() {
        let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
    }

    // 3. Right-Click opens the Settings Menu
    interaction.context_menu(|ui| {
        if ui.button("⚙ Settings").clicked() {
            *settings_open = true;
            // Forces focus to settings window
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

/// Draw the discrete resize grip in the bottom-right (or tope-right if inverted)
pub fn draw_resize_grip(
    ui: &mut Ui,
    ctx: &Context,
    rect: Rect,
    is_inverted: bool
){
  
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

pub fn draw_lock_button(
    ui: &mut Ui,
    rect: Rect,
    shared_state: &Arc<Mutex<SharedState>>,
    last_interaction: &mut Option<Instant>,
    is_focused: bool
) {
        // We lock briefly to check config/colors and update state if clicked
        let mut state = match shared_state.lock() {
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
            // wake up on click
            *last_interaction = Some(Instant::now());  
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
            *last_interaction = Some(Instant::now()); 
        }

        // 4. Calculate Opacity
        let mut opacity = 1.0;

        if is_locked {
            let cooldown = 3.0; // Seconds to wait before fading
            let fade_duration = 1.0;
            let resting_opacity = 0.1; // Dim state

            if let Some(last_interact) = last_interaction{
                let elapsed = last_interact.elapsed().as_secs_f32();
                let t = ((elapsed - cooldown) / fade_duration).clamp(0.0, 1.0);
                opacity = 1.0 - (t * (1.0 - resting_opacity));

                if t < 1.0{ ui.ctx().request_repaint();}

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

// =======================================================================================
// MEDIA CONTROLS
// =======================================================================================

/// Helper to draw vector media buttons (Prev / Play / Next)
/// (ISO 60417 standard geometry)
pub fn draw_transport_controls(
    ui: &mut Ui,
    controller: &dyn MediaController,
    is_playing: bool,
    opacity: f32,
    base_color: egui::Color32
) {
    let btn_size = egui::vec2(28.0, 28.0); 
    let color = base_color.linear_multiply(opacity);
    let hover_bg = base_color.linear_multiply(0.15 * opacity);

    // Use Right-to-Left to anchor to the right side
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;

        // === 3. NEXT (ISO 60417-5862) ===
        // Drawn FIRST so it appears on the Far Right
        let (rect, resp) = ui.allocate_exact_size(btn_size, egui::Sense::click());
        if resp.hovered() { ui.painter().rect_filled(rect.expand(2.0), 4.0, hover_bg); }
        if resp.clicked() { controller.try_next(); }

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
        if resp.clicked() { controller.try_play_pause(); }

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
        if resp.clicked() { controller.try_prev(); }

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


// =======================================================================================
// SETTINGS 
// =======================================================================================
/// Render settings window content
pub fn show_settings_window(
    ui: &mut egui::Ui,
    state: &mut SharedState,
    active_tab: &mut SettingsTab,
    save_target: &mut SaveTarget,
    new_preset_name: &mut String
) {
    // Tabs
    ui.add_space(5.0);
    ui.horizontal(|ui| {
        let colors = state.config.resolve_colors(&state.user_color_presets);
        let highlight = to_egui_color(colors.high);
        ui_tab_button(ui, " 🎨 Visual ", SettingsTab::Visual, active_tab, highlight);
        ui_tab_button(ui, " 🔊 Audio ", SettingsTab::Audio, active_tab, highlight);
        ui_tab_button(ui, " 🌈 Colors ", SettingsTab::Colors, active_tab, highlight);
        ui_tab_button(ui, " 🪟 Window ", SettingsTab::Window, active_tab, highlight);
        ui_tab_button(ui, " 📊 Stats ", SettingsTab::Performance, active_tab, highlight);
    });
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        match active_tab {
            SettingsTab::Visual => settings_tab_visual(ui, state, save_target, new_preset_name),
            SettingsTab::Audio => settings_tab_audio(ui, state),
            SettingsTab::Colors => settings_tab_colors(ui, state, save_target, new_preset_name),
            SettingsTab::Window => settings_tab_window(ui, state),
            SettingsTab::Performance => settings_tab_performance(ui, state),
        }
    });
}

pub fn settings_tab_visual(
    ui: &mut egui::Ui,
    state: &mut SharedState,
    save_target: &mut SaveTarget,
    new_preset_name: &mut String
) {
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
                *save_target = SaveTarget::Visual;
                *new_preset_name = state.config.profile.name.clone(); // Pre-fill
            }
        });

        // -- Save Popup --
        if *save_target == SaveTarget::Visual {
            ui_save_popup(ui, new_preset_name, |name| {
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
            }, save_target);
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

pub fn settings_tab_audio(ui: &mut egui::Ui, state: &mut SharedState) {
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

pub fn settings_tab_colors(
    ui: &mut egui::Ui, 
    state: &mut SharedState,
    save_target: &mut SaveTarget,
    new_preset_name: &mut String
) {
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
                *save_target = SaveTarget::Color;
                new_preset_name.clear(); // Colors usually saved as new name
        }
        });

        // -- Save Popup --
        if *save_target == SaveTarget::Color {
            ui_save_popup(ui, new_preset_name, |name: String| {
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
            }, save_target);
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
        visualizers::draw_preview_spectrum(ui, &current_colors, bar_opacity);

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

pub fn settings_tab_window(ui: &mut egui::Ui, state: &mut SharedState) {
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

pub fn settings_tab_performance(ui: &mut egui::Ui, state: &mut SharedState) {

    
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


// =======================================================================================
// SETTING UI COMPONENTS
// =======================================================================================

/// "Pill" style tab button
pub fn ui_tab_button(
    ui: &mut Ui,
    label: &str,
    tab: SettingsTab,
    active_tab: &mut SettingsTab,
    highlight_color: Color32,
){
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

/// Simple Text Entry Pop-up
pub fn ui_save_popup( 
    ui: &mut Ui,
    name_buffer: &mut String,
    mut on_save: impl FnMut(String),
    target_flag: &mut SaveTarget,
){
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