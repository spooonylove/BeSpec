use eframe::egui::{self, Ui, Rect, Context, Sense, RichText, Color32};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::shared_state::{self, ColorProfile, SharedState};
use crate::media::MediaController;

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