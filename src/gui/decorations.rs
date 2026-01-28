use eframe::egui;
use crate::gui::theme::*;
use crate::shared_state::AppConfig;
use egui::epaint::Vertex;
use egui::{Mesh, Pos2};

pub struct ChromeLayout {
    pub content_rect: egui::Rect,
    pub is_collapsed: bool,
}

fn draw_haiku_rect(
    painter: &egui::Painter,
    rect: egui::Rect,
    col_top: egui::Color32,
    col_bot: egui::Color32,
    border_col: egui::Color32,
) {
    let mut mesh = Mesh::default();
    let l = rect.left();
    let r = rect.right();
    let t = rect.top();
    let b = rect.bottom();
    
    let idx = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex { pos: Pos2::new(l, t), uv: Pos2::ZERO, color: col_top });
    mesh.vertices.push(Vertex { pos: Pos2::new(r, t), uv: Pos2::ZERO, color: col_top });
    mesh.vertices.push(Vertex { pos: Pos2::new(r, b), uv: Pos2::ZERO, color: col_bot });
    mesh.vertices.push(Vertex { pos: Pos2::new(l, b), uv: Pos2::ZERO, color: col_bot });
    
    mesh.add_triangle(idx, idx + 1, idx + 2);
    mesh.add_triangle(idx, idx + 2, idx + 3);
    painter.add(egui::Shape::mesh(mesh));

    painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, border_col));
}

pub fn draw_beos_window_frame(
    ui: &mut egui::Ui,
    _ctx: &egui::Context, 
    window_rect: egui::Rect,
    config: &mut AppConfig,
    background_color: egui::Color32,
) -> ChromeLayout {
    
    if !config.beos_mode {
        return ChromeLayout { content_rect: window_rect, is_collapsed: false };
    }

    let painter = ui.painter();
    
    // === METRICS & COLORS ===
    let tab_height = BEOS_TAB_HEIGHT; 
    let border_width = BEOS_BORDER_WIDTH;
    
    let col_tab_top = BEOS_TAB_GRADIENT_TOP;
    let col_tab_bot = BEOS_TAB_GRADIENT_BOT;
    let col_light = BEOS_TAB_HIGHLIGHT;
    let col_shadow = BEOS_TAB_SHADOW;
    
    let col_text = egui::Color32::BLACK;
    let col_btn_border = BEOS_BUTTON_BORDER; 

    // === TAB CALCS ===
    let title_text = "BeSpec";
    let title_font = egui::FontId::proportional(13.0);
    let galley = painter.layout_no_wrap(title_text.to_string(), title_font.clone(), col_text);
    
    let dynamic_tab_width = galley.rect.width() + 80.0;
    let max_tab_width = window_rect.width() - (border_width * 2.0);
    let tab_width = dynamic_tab_width.min(max_tab_width);
    
    let max_offset = window_rect.width() - tab_width;
    config.beos_tab_offset = config.beos_tab_offset.clamp(border_width, max_offset);
    
    let tab_rect = egui::Rect::from_min_size(
        window_rect.min + egui::vec2(config.beos_tab_offset, 0.0),
        egui::vec2(tab_width, tab_height)
    );

    // Interaction
    let tab_id = ui.make_persistent_id("beos_tab_interact");
    let tab_sense = ui.interact(tab_rect, tab_id, egui::Sense::drag());
    
    if tab_sense.dragged() {
        if ui.input(|i| i.modifiers.shift) {
            config.beos_tab_offset += tab_sense.drag_delta().x;
        } else {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }
    if tab_sense.double_clicked() {
        config.beos_window_collapsed = !config.beos_window_collapsed;
        let new_height = if config.beos_window_collapsed { tab_height + border_width } else { config.window_size[1] };
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(config.window_size[0], new_height)));
    }

    // === 1. DRAW TAB (Squared Gradient) ===
    draw_haiku_rect(painter, tab_rect, col_tab_top, col_tab_bot, egui::Color32::TRANSPARENT);

    // Tab Highlights
    painter.line_segment([tab_rect.left_top(), tab_rect.right_top()], egui::Stroke::new(1.0, col_light));
    painter.line_segment([tab_rect.left_top(), tab_rect.left_bottom()], egui::Stroke::new(1.0, col_light));
    painter.line_segment([tab_rect.right_top(), tab_rect.right_bottom()], egui::Stroke::new(1.0, col_shadow));

    // Title
    let text_pos = egui::pos2(
        tab_rect.center().x - (galley.rect.width() / 2.0),
        tab_rect.center().y - (galley.rect.height() / 2.0) - 1.0 
    );
    painter.galley(text_pos, galley, col_text);


    // === 2. BUTTONS ===

    // A. Close Button (Left)
    let close_size = 14.0;
    let close_rect = egui::Rect::from_center_size(
        egui::pos2(tab_rect.left() + 16.0, tab_rect.center().y),
        egui::vec2(close_size, close_size)
    );
    
    draw_haiku_rect(
        painter, 
        close_rect, 
        BEOS_CLOSE_TOP, 
        BEOS_CLOSE_BOT, 
        col_btn_border 
    );
    
    if ui.interact(close_rect, tab_id.with("close"), egui::Sense::click()).clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }

    // B. Zoom Button (Right)
    let zoom_center = egui::pos2(tab_rect.right() - 14.0, tab_rect.center().y);
    let zoom_hitbox = egui::Rect::from_center_size(zoom_center, egui::vec2(16.0, 16.0));

    // RECT 1: The "Large" Square (Background) -> Bottom-Right
    let back_size = 9.0;
    let back_rect = egui::Rect::from_min_size(
        zoom_center + egui::vec2(-2.0, -2.0), // Start near center, extend down-right
        egui::vec2(back_size, back_size)
    );
    draw_haiku_rect(
        painter,
        back_rect,
        BEOS_ZOOM_BACK_TOP, 
        BEOS_ZOOM_BACK_BOT,
        col_btn_border 
    );

    // RECT 2: The "Small" Square (Foreground/Top) -> Top-Left
    let front_size = 7.0;
    let front_rect = egui::Rect::from_min_size(
        zoom_center + egui::vec2(-7.0, -7.0), // Up and Left
        egui::vec2(front_size, front_size)
    );
    draw_haiku_rect(
        painter,
        front_rect,
        BEOS_ZOOM_FRONT_TOP, 
        BEOS_ZOOM_FRONT_BOT,
        col_btn_border
    );

    if ui.interact(zoom_hitbox, tab_id.with("zoom"), egui::Sense::click()).clicked() {
        let is_maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
    }


    // === 3. WINDOW FRAME ===
    let frame_rect = if config.beos_window_collapsed {
        egui::Rect::from_min_size(
            window_rect.min + egui::vec2(0.0, tab_height), 
            egui::vec2(window_rect.width(), border_width)
        )
    } else {
        egui::Rect::from_min_size(
            window_rect.min + egui::vec2(0.0, tab_height), 
            window_rect.size() - egui::vec2(0.0, tab_height)
        )
    };

    if !config.beos_window_collapsed { 
        painter.rect_filled(frame_rect, 0.0, background_color); 
    }

    // Outer Bevel
    painter.line_segment([frame_rect.left_top(), frame_rect.right_top()], egui::Stroke::new(1.0, BEOS_FRAME_MID)); 
    painter.line_segment([frame_rect.left_top(), frame_rect.left_bottom()], egui::Stroke::new(1.0, BEOS_FRAME_MID)); 
    painter.line_segment([frame_rect.right_top(), frame_rect.right_bottom()], egui::Stroke::new(1.0, BEOS_FRAME_SHADOW)); 
    painter.line_segment([frame_rect.left_bottom(), frame_rect.right_bottom()], egui::Stroke::new(1.0, BEOS_FRAME_SHADOW)); 

    // Inner Bevel
    let inner_frame = frame_rect.shrink(1.0);
    painter.line_segment([inner_frame.left_top(), inner_frame.right_top()], egui::Stroke::new(1.0, BEOS_FRAME_LIGHT)); 
    painter.line_segment([inner_frame.left_top(), inner_frame.left_bottom()], egui::Stroke::new(1.0, BEOS_FRAME_LIGHT)); 
    painter.line_segment([inner_frame.right_top(), inner_frame.right_bottom()], egui::Stroke::new(1.0, BEOS_FRAME_DARK)); 
    painter.line_segment([inner_frame.left_bottom(), inner_frame.right_bottom()], egui::Stroke::new(1.0, BEOS_FRAME_DARK)); 
    
    if border_width > 2.0 {
        let body_rect = inner_frame.shrink(1.0);
        painter.rect_stroke(body_rect, 0.0, egui::Stroke::new(2.0, BEOS_FRAME_MID));
    }

    ChromeLayout {
        content_rect: if config.beos_window_collapsed { egui::Rect::from_min_size(frame_rect.min, egui::vec2(0.0, 0.0)) } else { frame_rect.shrink(border_width) },
        is_collapsed: config.beos_window_collapsed,
    }
}