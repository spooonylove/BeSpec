use egui::{Painter, Rect, Pos2, Stroke};
use crate::shared_state::{AppConfig, ColorProfile, PerformanceStats, VisualMode, 
    VisualProfile, VisualizationData, MediaDisplayMode};
use crate::gui::theme::{to_egui_color, db_to_px, lerp_color};
use crate::fft_processor::FFTProcessor;

pub fn draw_main_visualizer(
    painter: &Painter,
    rect: Rect, 
    data: &VisualizationData,
    config: &AppConfig,
    colors: &crate::shared_state::ColorProfile,
    perf: &PerformanceStats,
    mouse_pos: Option<egui::Pos2>,
){

    /*// [TRACE 3] Dependency-free version
    // This will print every frame. Run the app for 5 seconds then close it.
    if !data.bars.is_empty() { 
        tracing::info!("[TRACE 3] Drawing! Rect: {:?}, First Bar: {:.1} dB", 
            rect, 
            data.bars[0]
        );
    }*/


    let profile = &config.profile;
    let num_bars = data.bars.len();

    // Early exit if no data (unless in Oscope mode)
    if num_bars == 0 && profile.visual_mode != VisualMode::Oscilloscope {
        // Draw "Waiting..." in the center
        let text_color = to_egui_color(colors.text).linear_multiply(0.5);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "⏸ Waiting for audio...",
            egui::FontId::proportional(20.0),
            text_color,
        );
        
        // NOW we can return, because we've drawn *something* to verify window position
        return;
    }

    // DEBUG
    // [NEW] 1.5. Draw Background Plate
    // This ensures the window has a "body" even if the bars are 0 height.
    // We use a low opacity version of the background color.
    let bg_plate = to_egui_color(colors.background).linear_multiply(0.5);
    painter.rect_filled(rect, 4.0, bg_plate);

    // 1. Calculate Common Layout Helpers
    // Ensure we don't divide by zero even if bars are missing
    let bar_slot_width = rect.width() / num_bars.max(1) as f32;
    let bar_width = (bar_slot_width - profile.bar_gap_px as f32).max(1.0);

    // 2. Handle mouse interactions (for frequency modes)
    // Calculate hovered index using the passed-in mouse_pos
    let hovered_bar_index = if config.inspector_enabled && profile.visual_mode != VisualMode::Oscilloscope {
        mouse_pos.and_then(|pos| {
            if rect.contains(pos) {
                let relative_x = pos.x - rect.left();
                let index = (relative_x / bar_slot_width).floor() as usize;
                if index < num_bars {Some(index)} else { None }
            }else { None }
        })
    } else { None };

    // 3. Dispatch Drawing Strategy
    match profile.visual_mode {
            VisualMode::SolidBars => {
                draw_solid_bars(
                    &painter,
                    rect,
                    profile,
                    &colors,                 
                    data,                    
                    bar_width,
                    bar_slot_width,
                    hovered_bar_index,
                    config.noise_floor_db);
            },
            VisualMode::SegmentedBars => {
                draw_segmented_bars(
                    &painter,
                    rect,
                    profile,
                    &colors,
                    data,
                    bar_width,
                    bar_slot_width,
                    hovered_bar_index,
                    config.noise_floor_db);
            },
            VisualMode::LineSpectrum => {
                draw_line_spectrum(
                    &painter,
                    rect,
                    profile,
                    &colors,
                    data,
                    hovered_bar_index,
                    config.noise_floor_db);
            },
            VisualMode::Oscilloscope => {
                draw_oscilloscope(
                    &painter,
                    rect,
                    profile,
                    &colors,
                    data,
                );
            },
        }
        
        // 7. Draw Overlays
        if let Some(index) = hovered_bar_index {
            draw_inspector_overlay(
                &painter,
                rect,
                &colors,
                data,
                perf,
                index,
                bar_slot_width,
                config.noise_floor_db);
        }

        if config.show_stats {
            draw_stats_overlay(
                &painter,
                rect,
                &colors,
                perf);
        }
}

/// Draw solid gradient bars
pub fn draw_solid_bars(
    painter: &Painter,
    rect: Rect,
    profile: &VisualProfile,
    colors: &ColorProfile,
    data: &VisualizationData,    
    bar_width: f32,
    bar_slot_width: f32,
    hovered_index: Option<usize>,
    noise_floor_db: f32,
){
    let low = to_egui_color(colors.low).gamma_multiply(profile.bar_opacity);
    let high = to_egui_color(colors.high).gamma_multiply(profile.bar_opacity);
    let peak = to_egui_color(colors.peak).gamma_multiply(profile.bar_opacity);

    for (i, &db) in data.bars.iter().enumerate() {
        let x = rect.left() + (i as f32 * bar_slot_width);
        

        let bar_height = db_to_px(db, noise_floor_db, rect.height());
        
        
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
            let peak_h = db_to_px(data.peaks[i], noise_floor_db, rect.height());
            
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
///(painter, rect, profile, colors, data, bar_width, bar_slot_width, hovered_bar_index, config.noise_floor_db);
pub fn draw_segmented_bars(
    painter: &egui::Painter, 
    rect: egui::Rect,
    profile: &VisualProfile,
    colors: &ColorProfile, 
    data: &crate::shared_state::VisualizationData,
    bar_width: f32,
    bar_slot_width: f32,
    _hovered_index: Option<usize>,
    noise_floor_db: f32
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
            let x = rect.left() + (i as f32 * bar_slot_width);
            
            // Convert dB to pixel height
            let total_h = db_to_px(db, noise_floor_db, rect.height());
            
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
                let peak_h = db_to_px(data.peaks[i], noise_floor_db, rect.height());
                
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

/// Draw line representation of spetrum data
pub fn draw_line_spectrum(
    painter: &egui::Painter,
    rect: egui::Rect,
    profile: &VisualProfile,
    colors: &ColorProfile,
    data: &crate::shared_state::VisualizationData,
    hovered_index: Option<usize>,
    noise_floor_db: f32
) {
    if data.bars.is_empty() { return; }
    
    // Use Profile colors
    let high = to_egui_color(colors.high).gamma_multiply(profile.bar_opacity);

    // Pre-calculate points 
    let points: Vec<egui::Pos2> = data.bars.iter().enumerate().map(|(i, &db)| {
        let x = rect.left() + (i as f32 / data.bars.len() as f32) * rect.width();
        let height = db_to_px(db, noise_floor_db, rect.height());
    
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

/// Draws a classic oscillioscope waveform
pub fn draw_oscilloscope(
    painter: &Painter,
    rect: Rect,
    profile: &VisualProfile,
    colors: &ColorProfile,
    data: &VisualizationData,
) {
    if data.waveform.is_empty() { return; }

    let line_color = to_egui_color(colors.high).gamma_multiply(profile.bar_opacity);

    let samples = &data.waveform;
    let len = samples.len();
    let middle_y = rect.center().y;
    let height_scale = rect.height() * 0.45; // Leave some cushion for the pushing

    let points: Vec<egui::Pos2> = samples
        .iter()
        .enumerate()
        .map(|(i, &sample)|{
            let x = rect.left() + (i as f32 / len as f32) *  rect.width();
            let y = middle_y - (sample * height_scale);
            egui::pos2(x,y)
        })
        .collect();

   
    painter.add(egui::Shape::line(
        points,
        Stroke::new(1.5, line_color),
    ));
}


pub fn draw_inspector_overlay( 
    painter: &egui::Painter, 
    rect: egui::Rect, 
    colors: &ColorProfile,
    data: &crate::shared_state::VisualizationData,
    perf: &crate::shared_state::PerformanceStats,
    index: usize,
    slot_width: f32,
    _noise_floor: f32,
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
pub fn draw_stats_overlay(
    painter: &egui::Painter,
    rect: egui::Rect,
    colors: &ColorProfile,
    perf: &crate::shared_state::PerformanceStats
) {
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
    let bg_color = crate::gui::theme::to_egui_color(colors.inspector_bg);
    let text_color = crate::gui::theme::to_egui_color(colors.inspector_fg);

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
    painter.galley(pos + egui::vec2(pad, pad), galley, egui::Color32::WHITE); // Text color is baked into galley
}

pub fn draw_media_overlay(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    media_info: Option<&crate::media::MediaTrackInfo>,
    media_display_mode: crate::shared_state::MediaDisplayMode,
    overlay_font: &crate::shared_state::ThemeFont,
    media_opacity: f32,
    colors: &crate::shared_state::ColorProfile,
    album_art_texture: Option<&egui::TextureHandle>,
) {

    // 1. Early Exit (Invisible or Off)
    if media_display_mode == MediaDisplayMode::Off {
        return;
    }

    // 2. Setup Styles
    let base_text_color = to_egui_color(colors.text);
    let base_font_id = crate::gui::theme::to_egui_font(overlay_font);
    let font_family = base_font_id.family;
    
    // 3. Layout calculation
    // Anchor relative to the visulalizer 'rect' passed in
    let overlay_w = rect.width() * 0.5;
    let overlay_h = 100.0;
    let pos = egui::pos2(rect.right() - overlay_w - 20.0, rect.top() + 20.0);
    let overlay_rect = egui::Rect::from_min_size(pos, egui::vec2(overlay_w, overlay_h));


    // 4. Draw Content in an Allocatd Rect 
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(overlay_rect), |ui| {
        // Force right-to-left layout
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            // Manually restrict size
            ui.set_max_width(overlay_w);

            match media_info{
                Some(info) =>{
                    // === CASE A: Track Info Present ===
                    
                    // Album Art
                    if let Some(texture) = album_art_texture {
                        let tint = egui::Color32::WHITE.linear_multiply(media_opacity);

                        let response = ui.add(
                            egui::Image::new(texture)
                                .max_height(50.0)
                                .rounding(4.0)
                                .tint(tint)
                                .sense(egui::Sense::click())
                            );   

                        // Interaction: Initiate Wiki Search!!
                        if response.clicked() {
                            // Clone string data to move into the thread
                            let artist = info.artist.clone();
                            let title = info.title.clone();
                            let album = info.album.clone();

                            // Spawn a thread to prevent blocking the GUI during the network request
                            std::thread::spawn(move || {
                                // Generate the URL (Blocking call to ureq inside fetch_wikipedia_url)
                                let url = crate::media::fetch_wikipedia_url(&artist, &title, &album);
                                tracing::info!("[GUI] Opening Wiki URL: {}", url);

                                // Open in default system browser
                                if let Err(e) = open::that(&url) {
                                    tracing::error!("[GUI] Failed to open URL: {}", e);
                                }
                            });
                        }

                        if response.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            response.on_hover_text(format!("Search Wikipedia for '{}'", info.artist));
                        }
                        
                        ui.add_space(10.0); 

                    }

                    // Text Stack
                    ui.vertical(|ui| {
                        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                            // Title
                            ui.add(egui::Label::new(
                                egui::RichText::new(&info.title)
                                    .font(egui::FontId::new(16.0, font_family.clone()))
                                    .strong()
                                    .color(base_text_color.linear_multiply(media_opacity))
                            ));

                            // Artist
                            ui.add(egui::Label::new(
                                egui::RichText::new(format!("{} - {}", info.artist, info.album))
                                    .font(egui::FontId::new(11.0, font_family.clone()))
                                    .color(base_text_color.linear_multiply(0.8).linear_multiply(media_opacity))
                            ));

                            ui.add_space(2.0);

                            // Controls
                            // TODO: Move render_transport_controls to widgets.rs
                            /*
                            
                            ui.add_space(4.0);
                            crate::gui::widgets::render_transport_controls(
                                ui,
                                info.is_playing,
                                media_opacity,
                                base_text_color
                            );

                            */

                            /*  OLD CODE
                            if cfg!(not(target_os = "macos")) {
                                ui.add_space(4.0);
                                self.render_transport_controls(ui, info.is_playing, media_opacity, base_text_color);
                            } else {
                                ui.add(egui::Label::new(
                                    egui::RichText::new(format!("via {}", info.source_app))
                                        .font(egui::FontId::new(10.0, font_family.clone()))
                                        .color(base_text_color.linear_multiply(0.5).linear_multiply(media_opacity))
                                ));
                            }*/
                        });
                    });
                },
                None => {
                    // === Case B: No info, but Always On ===
                    if media_display_mode == crate::shared_state::MediaDisplayMode::AlwaysOn {
                        ui.vertical(|ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                ui.label(egui::RichText::new("Waiting for media...")
                                    .family(font_family.clone())
                                    .size(14.0)
                                    .color(base_text_color.gamma_multiply(media_opacity * 0.6))
                                );
                            });
                        });
                    }
                }
            }
        });
    });
}