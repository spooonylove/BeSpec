use eframe::egui::{self, Ui, Rect, Context, Sense, RichText, Color32};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::shared_state::{SharedState, ColorProfile};
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