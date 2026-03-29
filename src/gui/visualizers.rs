use egui::{Painter, Rect, Stroke};
use crate::media::MediaController;
use crate::shared_state::{ColorProfile, PerformanceStats, VisualMode, 
    VisualProfile, VisualizationData, MediaDisplayMode};
use crate::gui::theme::{to_egui_color, db_to_px, lerp_color};
use crate::gui::widgets::draw_transport_controls;
use crate::fft_processor::FFTProcessor;

/// The physical thickness (in points) of the peak indicator blocks
const PEAK_THICKNESS: f32 = 2.0;


pub fn draw_main_visualizer(
    painter: &Painter,
    rect: Rect,
    config: &crate::shared_state::AppConfig,
    profile: &VisualProfile, 
    colors: &crate::shared_state::ColorProfile,
    data: &VisualizationData,
    perf: &PerformanceStats,
    mouse_pos: Option<egui::Pos2>,
    safe_bar_count: usize,
){

    // Determine the primary axis length (in physical/logical points) based on orientation
    let max_u= match profile.orientation {
        crate::shared_state::Orientation::BottomUp | crate::shared_state::Orientation::TopDown =>rect.width(),
        crate::shared_state::Orientation::LeftRight | crate::shared_state::Orientation::RightLeft =>rect.height(),
    };
    
    // Safety clmap: Ensure we never try to draw more bars than we have data for,
    // and never let display_bars hit 0 (which would cause a divide-by-zero panic)
    let display_bars = safe_bar_count.min(data.bars.len()).max(1);

    // --- ARCHITECTURAL DECISION: Pure Floating-Point Layout ---
    // We intentionally DO NOT use .floor() or .round() here.
    // Forcing this value to an integer causes either:
    //   a) Massive dead-space margins at the edges of the window (if floored)
    //   b) "Fat Bars" or uneven gaps (if we try to distribute the remainder)
    // Instead, we calculate the sub-pixel width. We rely on the GPU's native
    // anti-aliasing to gracefully blur the fractional pixel boundaries, resulting
    // a smooth, edge-to-edge layout without (as much) structural banding
    let bar_slot_width = max_u / display_bars as f32;
    let bar_width = (bar_slot_width - profile.bar_gap_px as f32).max(1.0);
  

    // Resolve hover interactions. We use the exact float slot width to reverse calculate
    // which mathematical slot the mouse cursor is currently residing in.
    let hovered_bar_index = if config.inspector_enabled && profile.visual_mode != VisualMode::Oscilloscope {
        mouse_pos.and_then(|pos| {
            if rect.contains(pos) {
                // Determine logical 'u' position based on orientation
                let u_pos = match profile.orientation {
                    crate::shared_state::Orientation::BottomUp | crate::shared_state::Orientation::TopDown =>{
                        pos.x - rect.left()
                    }
                    crate::shared_state::Orientation::LeftRight | crate::shared_state::Orientation::RightLeft => {
                        pos.y - rect.top()
                    } 
                };

                let index = (u_pos / bar_slot_width).floor() as usize;
                if index < display_bars { Some(index)} else { None }
            }else { None }
        })
    } else { None };

    // 3. Dispatch to the specific rendering algorithm...
    match profile.visual_mode {
        VisualMode::SolidBars => {
            draw_solid_bars(
                painter,
                rect,
                profile,
                colors,                 
                data,                    
                bar_width,
                bar_slot_width,
                hovered_bar_index,
                config.noise_floor_db);
        },
        VisualMode::SegmentedBars => {
            draw_segmented_bars(
                painter,
                rect,
                profile,
                colors,
                data,
                bar_width,
                bar_slot_width,
                hovered_bar_index,
                config.noise_floor_db);
        },
        VisualMode::LineSpectrum => {
            draw_line_spectrum(
                painter,
                rect,
                profile,
                colors,
                data,
                hovered_bar_index,
                config.noise_floor_db);
        },
        VisualMode::Oscilloscope => {
            draw_oscilloscope(
                painter,
                rect,
                profile,
                colors,
                data,
            );
        },
    }
        
    // Render Overlay UI...
    if let Some(index) = hovered_bar_index {
        draw_inspector_overlay(
            painter,
            rect,
            profile,
            colors,
            data,
            perf,
            index,
            bar_slot_width,
            config.noise_floor_db);
    }

    if config.show_stats {
        draw_stats_overlay(
            painter,
            rect,
            colors,
            perf,
            display_bars,
            profile.num_bars
        );
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

    // Determine the maximum magnitude dimension for db_to_px scaling
    let (max_u, max_v) = match profile.orientation {
        crate::shared_state::Orientation::BottomUp | crate::shared_state::Orientation::TopDown => (rect.width(), rect.height()),
        crate::shared_state::Orientation::LeftRight | crate::shared_state::Orientation::RightLeft => (rect.height(), rect.width()),
    };

    // Protect against drawing phantom bars off-screen during rapid window shrink
    let display_bars = (max_u / bar_slot_width).floor() as usize;

    for (i, &db) in data.bars.iter().take(display_bars).enumerate() {
        // Calculate the logical baseline coordinate.
        // By keeping this as a pure float (eg 4.25, 8.50), the GPU will apply
        // sub-pixel rendering. This avoids integer-snapping artifacts where gaps
        // appear rhythmically wider or narrower across the screen.
        let u = i as f32 * bar_slot_width;
        

        // Map audio dB to a physical screen dimension
        let bar_v = db_to_px(db, noise_floor_db, max_v);
        let norm_height = (bar_v / max_v).clamp(0.0, 1.0);
        
        // Calculate gradient coloring
        let mut bar_color = lerp_color(low, high, norm_height);
        if Some(i) == hovered_index {
            bar_color = lerp_color(bar_color, egui::Color32::WHITE, 0.5);
        }

        // Map logical u/v coordinates to physical x/y coordinates based on user orientation
        let p_base_left = map_uv_to_xy(rect, u, 0.0, profile.orientation);
        let p_base_right = map_uv_to_xy(rect, u + bar_width, 0.0, profile.orientation);
        let p_tip_right = map_uv_to_xy(rect, u + bar_width, bar_v, profile.orientation);
        let p_tip_left = map_uv_to_xy(rect, u, bar_v, profile.orientation);

        // Construct and submit the immmediate-mode mesh for this bar
        use egui::epaint::Vertex;
        let mut mesh = egui::Mesh::default();

        let v_idx = mesh.vertices.len() as u32;
        mesh.vertices.push(Vertex { pos: p_base_left, uv: egui::Pos2::ZERO, color: low });
        mesh.vertices.push(Vertex { pos: p_base_right, uv: egui::Pos2::ZERO, color: low });
        mesh.vertices.push(Vertex { pos: p_tip_right, uv: egui::Pos2::ZERO, color: bar_color });
        mesh.vertices.push(Vertex { pos: p_tip_left, uv: egui::Pos2::ZERO, color: bar_color });
        
        mesh.add_triangle(v_idx, v_idx + 1, v_idx + 2);
        mesh.add_triangle(v_idx, v_idx + 2, v_idx + 3);
        painter.add(egui::Shape::mesh(mesh));

        // Peaks
        if profile.show_peaks && i < data.peaks.len() {
            let peak_v = db_to_px(data.peaks[i], noise_floor_db, max_v);
            let peak_thickness = PEAK_THICKNESS;
            
            // Just grab the two opposing logical corners
            let p1 = map_uv_to_xy(rect, u, peak_v, profile.orientation);
            let p2 = map_uv_to_xy(rect, u + bar_width, peak_v + peak_thickness, profile.orientation);

            // from_two_pos automatically handles sorting the coordinates, 
            // no matter which cardinal direction they were mapped to!
            let peak_rect = egui::Rect::from_two_pos(p1, p2);
            painter.rect_filled(peak_rect, 0.0, peak);
        }
    }    
}

/// Draws the "Segmented" (LED-style) audio visualizer mode.
///
/// Renders the frequency spectrum as a series of discrete blocks, mimicking
/// vintage hardware LED VU meters.
///
/// # Rendering Optimizations
/// This function is heavily optimized to maintain 60 FPS under extreme conditions:
/// * **Batched Master Mesh:** Bypasses standard `egui` shape allocation by calculating 
///   raw vertices and triangles, submitting them to the GPU in a single draw call.
/// * **Vertical LOD Dynamic Scaling:** Automatically caps the maximum number of segments per bar 
///   (e.g., to 150) by dynamically scaling segment height/gap to prevent CPU geometry overload.
/// * **Horizontal LOD Aggregator:** Prevents geometry overdraw when the window is too narrow by 
///   looking ahead and aggregating frequencies that would occupy the same physical pixel column.
/// * **Physical Pixel Snapping:** Forces scaled segment corners to align perfectly with the 
///   physical pixel grid of the monitor, preventing sub-pixel rendering (Moiré aliasing).
///
/// # Arguments
/// * `painter` - The active `egui::Painter` to submit the master mesh to.
/// * `rect` - The bounding box allocated for this visualizer.
/// * `profile` - Current user settings (colors, segment heights, peak toggles).
/// * `colors` - The active color theme mapping (low, high, peak).
/// * `data` - The processed FFT visualization data (bar heights and peak data).
/// * `bar_width` - The physical width of the solid portion of a bar.
/// * `bar_slot_width` - The physical width of a bar plus its horizontal gap.
/// * `_hovered_index` - Optional index of a bar currently hovered by the user (unused in this mode).
/// * `noise_floor_db` - The absolute minimum dB value to map to 0 pixels high.
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

    // Determine the maximum magnitude dimension for LOD and scalling
    let (max_u, max_v) = match profile.orientation {
        crate::shared_state::Orientation::BottomUp | crate::shared_state::Orientation::TopDown => (rect.width(), rect.height()),
        crate::shared_state::Orientation::LeftRight | crate::shared_state::Orientation::RightLeft => (rect.height(), rect.width()),
    };

    // 2. Calculate Segment Geometry
    // Ensure we don't get stuck in infinite loops with 0 height
    let mut seg_h = profile.segment_height_px.max(1.0);
    let mut seg_gap = profile.segment_gap_px.max(0.0);

    // --- Vertical LOD Dynamic Scaling Governor
    let max_segments_allowed = 150.0;
    let requested_segments  = max_v / (seg_h + seg_gap);
    
    if requested_segments > max_segments_allowed {
        let scale_factor = requested_segments / max_segments_allowed;
        seg_h *= scale_factor;
        seg_gap *= scale_factor;
    }

    //  --- Aliasing Fix: Physical Pixel Snapping 
    // The above scale factor and user input settings can prompt this routine to draw
    // non-integer (read: fractional pixel) rectangle sizes, resulting in aliasing in the display
    // Snapping the sizes to a physical pixel size results in prettier boxes!
    let ppp = painter.ctx().pixels_per_point();
    let snap = |p: f32| (p * ppp).round() / ppp;

    // Force heights and gaps to be exact whole physical pixels
    seg_h = (seg_h * ppp).round().max(1.0) / ppp;
    seg_gap = (seg_gap * ppp).round().max(0.0) / ppp;

    let total_seg_h = seg_h + seg_gap;

    // --- Initialize the mesh ---
    let mut master_mesh = egui::Mesh::default();

    // Pre-allocate memory to prevent mid-loop reallocation overhead
    let max_segs_per_bar = (max_v / total_seg_h).ceil() as usize;
    let estimated_total = data.bars.len() * max_segs_per_bar;
    master_mesh.reserve_vertices(estimated_total * 4);
    master_mesh.reserve_triangles(estimated_total * 2);

    // Helper closure to push a rectangle into the mesh
    let push_rect = |mesh: &mut egui::Mesh, r: egui::Rect, color: egui::Color32| {
        let min_x = snap(r.min.x);
        let min_y = snap(r.min.y);
        let max_x = snap(r.max.x);
        let max_y = snap(r.max.y);
        
        
        let idx = mesh.vertices.len() as u32;
        mesh.vertices.push(egui::epaint::Vertex {pos: egui::pos2(min_x, min_y), uv: egui::Pos2::ZERO, color});
        mesh.vertices.push(egui::epaint::Vertex {pos: egui::pos2(max_x, min_y), uv: egui::Pos2::ZERO, color});
        mesh.vertices.push(egui::epaint::Vertex {pos: egui::pos2(max_x, max_y), uv: egui::Pos2::ZERO, color});
        mesh.vertices.push(egui::epaint::Vertex {pos: egui::pos2(min_x, max_y), uv: egui::Pos2::ZERO, color});
        mesh.add_triangle(idx, idx + 1 , idx + 2);
        mesh.add_triangle(idx, idx + 2 , idx + 3);
    };
    

    // Protect against geometry overdraw during rapid resize events
    let display_bars = (max_u / bar_slot_width).floor() as usize;

    // 3. Render Each Bar
    for (i, &db) in data.bars.iter().take(display_bars).enumerate() {
            // Retain pure floating-point precision for horizontal layout.
            // Vertically, the segments are strictly pixel-snapped (via the LOD governor)
            // to ensure crisp LED boxes, but horizontally we allow sub-pixel blending to 
            // maintain an exact edge-to-edge fit across the window
            let u = i as f32 * bar_slot_width;
            
            // Convert dB to logical v-axis magnitude
            let total_v = db_to_px(db, noise_floor_db, max_v);
            
            // Determine how many segments fit in this height
            let num_segments = (total_v / total_seg_h).floor() as i32;
            
            // --- Draw Active Segments ---
            for s in 0..num_segments {
                let segment_idx = s as f32;
                let v_offset = segment_idx * total_seg_h;
                
                // Calculate gradient color based on vertical position
                let norm_h = (v_offset / max_v).clamp(0.0, 1.0);
                let color = lerp_color(low, high, norm_h);

                // Map logical bounds to physical rect
                let p1 = map_uv_to_xy(rect, u, v_offset, profile.orientation);
                let p2 = map_uv_to_xy(rect, u + bar_width, v_offset + seg_h, profile.orientation);
                
                push_rect(&mut master_mesh, egui::Rect::from_two_pos(p1, p2), color);
            }

            // --- Draw Peak Indicators ---
            if profile.show_peaks && i < data.peaks.len() {
                let peak_v = db_to_px(data.peaks[i], noise_floor_db, max_v);
                
                // Snap peak to the nearest segment grid position
                let peak_seg_idx = (peak_v / total_seg_h).floor();
                let v_offset = peak_seg_idx * total_seg_h;
                
                let p1  = map_uv_to_xy(rect, u, v_offset, profile.orientation);
                let p2 = map_uv_to_xy(rect, u + bar_width, v_offset + seg_h, profile.orientation);
                
                push_rect(&mut master_mesh, egui::Rect::from_two_pos(p1, p2), peak_color);

                // --- Fill Gap to Peak (Warning Mode) ---
                // If enabled, fills the empty space between the current bar level and the peak
                // with a dim color. Useful for seeing dynamic range.
                if profile.fill_peaks {
                    let gap_segments = (peak_seg_idx as i32) - num_segments;
                    if gap_segments > 0 {
                        let fill_color = peak_color.linear_multiply(0.3);
                        for g in 0..gap_segments {
                            // Offset from the top of the current bar
                            let gap_v = (num_segments + g) as f32 * total_seg_h;
                            let gp1 = map_uv_to_xy(rect, u, gap_v, profile.orientation);
                            let gp2 = map_uv_to_xy(rect, u + bar_width, gap_v + seg_h, profile.orientation);

                            push_rect(&mut master_mesh, egui::Rect::from_two_pos(gp1, gp2), fill_color);
                        }
                    }
                }
            }
        }

    // 4. Submit the massive batch to the GPU for ONE draw call
    painter.add(egui::Shape::mesh(master_mesh));
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

    // Determine the maximum logical dimensions based on orientation
    let (max_u, max_v) = match profile.orientation {
        crate::shared_state::Orientation::BottomUp | crate::shared_state::Orientation::TopDown => {
            (rect.width(), rect.height())
        }
        crate::shared_state::Orientation::LeftRight |  crate::shared_state::Orientation::RightLeft => {
            (rect.height(), rect.width())
        }
    };

    // Pre-calculate points using logical (u,v) mapping
    let points: Vec<egui::Pos2> = data.bars.iter().enumerate().map(|(i, &db)| {
        // Logical position along the baseline
        let u = (i as f32 / data.bars.len() as f32) * max_u;

        // Logical magnitude extending from the baseline
        let v = db_to_px(db, noise_floor_db, max_v);

        // Lthe helper function figure out the physical screen coordinates
        map_uv_to_xy(rect, u, v, profile.orientation)
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

    // 1. Determine maximum logical dimensions based on orientation
    let (max_u, max_v) = match profile.orientation {
        crate::shared_state::Orientation::BottomUp | crate::shared_state::Orientation::TopDown => {
            (rect.width(), rect.height())
        }
        crate::shared_state::Orientation::LeftRight | crate::shared_state::Orientation::RightLeft => {
            (rect.height(), rect.width())
        }
    };

    let samples = &data.waveform;
    let len = samples.len();

    // Shift our logical "0" to the middle of the magnitdue axis
    let middle_v = max_v / 2.0;
    let v_scale = max_v * 0.45; // Leave some cushion for the pushing

    let points: Vec<egui::Pos2> = samples
        .iter()
        .enumerate()
        .map(|(i, &sample)|{
            // u progresses steadily across the available baseline
            let u = (i as f32 / len as f32) * max_u;

            // v oscillates around the middle point
            let v = middle_v + (sample * v_scale);
            
            //Translate to physical screen coordinates
            map_uv_to_xy(rect, u, v, profile.orientation)
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
    profile: &VisualProfile,
    colors: &ColorProfile,
    data: &crate::shared_state::VisualizationData,
    perf: &crate::shared_state::PerformanceStats,
    hovered_index: usize,
    bar_slot_width: f32,
    _noise_floor: f32,
) {
    let num_bars = data.bars.len();
    if hovered_index >= num_bars { return; }

    // === 1. Calculate Data ===
    let db_value = data.bars[hovered_index];
    let sr = perf.fft_info.sample_rate;
    let fft_size = perf.fft_info.fft_size;
    
    // Get precise frequency bounds from the engine
    let max_freq = FFTProcessor::calculate_bar_frequency(hovered_index, num_bars, sr, fft_size);
    let min_freq = if hovered_index == 0 {
        0.0
    } else {
        FFTProcessor::calculate_bar_frequency(hovered_index - 1, num_bars, sr, fft_size)
    };
    
    // Calculate Center (Average) Frequency for display
    let center_freq = (min_freq + max_freq) / 2.0;

    // === 2. Build Rich Text Layout ===
    // We use a LayoutJob to mix font sizes and colors in one text block
    let mut job = egui::text::LayoutJob::default();
    let text_color = to_egui_color(colors.inspector_fg);
    let faint_color = text_color.linear_multiply(0.7);

    // [Primary]: Center Freq & dB Level (Medium Size, Strong)
    job.append(
        &format!("{:.0} Hz  |  {:.1} dB\n", center_freq, db_value),
        0.0,
        egui::text::TextFormat {
            font_id: egui::FontId::proportional(14.0), // Medium text
            color: text_color,
            ..Default::default()
        },
    );

    // [Secondary]: Band # and Range (Small, Monospace for alignment)
    job.append(
        &format!("Band {}  [{:.0} - {:.0} Hz]", hovered_index + 1, min_freq, max_freq),
        0.0,
        egui::text::TextFormat {
            font_id: egui::FontId::monospace(10.0), // Small text
            color: faint_color,
            ..Default::default()
        },
    );

    // Create the Galley (Layout result)
    let galley = painter.layout_job(job);

    // === 3. Position the Tooltip ===
  
    // Position: Above the click position, centered horizontally
    // Add some padding around the text
    let padding = egui::vec2(8.0, 6.0);
    let tooltip_size = galley.size() + (padding * 2.0);

    // Determine maximum logical dimensions
    let max_v = match profile.orientation {
        crate::shared_state::Orientation::BottomUp | crate::shared_state::Orientation::TopDown => rect.height(),
        crate::shared_state::Orientation::LeftRight | crate::shared_state::Orientation::RightLeft => rect.width(),
    };

    // Calculate logical center of the hovered bar
    let u_center= (hovered_index as f32 * bar_slot_width) + (bar_slot_width / 2.0);

    // Calculate the physical position for the target dot (10 logical pixels away)
    let dot_pos = map_uv_to_xy(rect, u_center, 10.0, profile.orientation);

    // Calculate physical anchor point for the tooltip (30 logical pixels away);
    let anchor_pos = map_uv_to_xy(rect, u_center, max_v - 30.0, profile.orientation);
    
    let mut tooltip_pos = anchor_pos - (tooltip_size / 2.0);

    // Ensure the physical x,y, bounds never leave the window, regardless of orientation
    let min_x = rect.left() + 5.0;
    let max_x = (rect.right() - tooltip_size.x - 5.0).max(min_x);
    tooltip_pos.x = tooltip_pos.x.clamp(min_x, max_x);

    let min_y = rect.top() + 5.0;
    let max_y = (rect.bottom() - tooltip_size.y - 5.0).max(min_y);
    tooltip_pos.y = tooltip_pos.y.clamp(min_y, max_y);
    

    let tooltip_rect = Rect::from_min_size(tooltip_pos, tooltip_size);

    // === 4. Draw Background & Border ===
    let bg_color = to_egui_color(colors.inspector_bg);
    
    // Background with rounding
    painter.rect_filled(tooltip_rect, 4.0, bg_color);
    
    // "Very light border"
    painter.rect_stroke(
        tooltip_rect, 
        4.0, 
        egui::Stroke::new(1.0, text_color.linear_multiply(0.2)) // 20% opacity of text color
    );

    // === 5. Draw Text ===
    painter.galley(tooltip_pos + padding, galley, egui::Color32::WHITE);

    // === 6. Draw Target Dot ===
    painter.circle_filled(dot_pos, 2.5, text_color.linear_multiply(0.8));
}

/// Render performance statistics overlay
pub fn draw_stats_overlay(
    painter: &egui::Painter,
    rect: egui::Rect,
    colors: &ColorProfile,
    perf: &crate::shared_state::PerformanceStats,
    display_bars: usize,
    requested_bars: usize,
) {
    // Position in top-left (with padding)
    let pos = rect.left_top() + egui::vec2(10.0, 10.0);
    
    let text = format!(
        "FPS: {:.0}\nFFT: {:.1}ms\nMin/Max: {:.1}/{:.1}ms\nFFT Res: {:.2} Hz/bin\nBars: {} / {}",
        perf.gui_fps,
        perf.fft_ave_time.as_micros() as f32 / 1000.0,
        perf.fft_min_time.as_micros() as f32 / 1000.0,
        perf.fft_max_time.as_micros() as f32 / 1000.0,
        perf.fft_info.frequency_resolution, // Strictly the raw FFT math
        display_bars,      
        requested_bars     
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
    controller: &dyn MediaController,
) {

    // 1. Early Exit (Invisible or Off)
    if media_display_mode == MediaDisplayMode::Off {
        return;
    }

    let win_width = rect.width();

    // If the window is tiny, don't show overlay at all (let the spectrum speak for itself!)
    if win_width < 250.0 {
        return;
    }

    let show_album_art = win_width > 450.0;

    // 2. Setup Styles
    let base_text_color = to_egui_color(colors.text);
    let base_font_id = crate::gui::theme::to_egui_font(overlay_font);
    let font_family = base_font_id.family;
    
    // 3. Layout calculation
    // Anchor relative to the visulalizer 'rect' passed in
    let overlay_w = rect.width() * 0.5;
    let overlay_h = 100.0;

    // Position top-right with padding
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
                    if show_album_art{
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
                    }
                    
                    // Text Stack
                    ui.vertical(|ui| {
                        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                            
                            // Title (Scrolling)
                            let title_font = egui::FontId::new(16.0, font_family.clone());
                            let title_color = base_text_color.linear_multiply(media_opacity);
                            draw_scrolling_label(ui, &info.title, title_font, title_color);

                            // Artist
                            let artist_font = egui::FontId::new( 12.0, font_family.clone());
                            let artist_color = base_text_color.linear_multiply(media_opacity);
                            draw_scrolling_label(ui, &info.artist, artist_font, artist_color);

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

                            if cfg!(not(target_os = "macos")) {
                                ui.add_space(4.0);
                                draw_transport_controls(
                                    ui,
                                    controller,
                                    info.is_playing,
                                    media_opacity,
                                    base_text_color);
                            } 

                            ui.add(egui::Label::new(
                                egui::RichText::new(format!("via {}", info.source_app))
                                        .font(egui::FontId::new(10.0, font_family.clone()))
                                        .color(base_text_color.linear_multiply(0.5).linear_multiply(media_opacity))
                            ));
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

pub fn draw_sonar_ping(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    strength: f32,
    colors: &ColorProfile,
) {
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


pub fn draw_preview_spectrum(
    ui: &mut egui::Ui,
    current_colors: &ColorProfile,
    bar_opacity: f32
) {
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

/// Draw text that scrolls if it exceeds avaialable width
fn draw_scrolling_label(
    ui: &mut egui::Ui,
    text: &str, 
    font_id: egui::FontId,
    color: egui::Color32)
{
    let available_width = ui.available_width();

    // Create the galley for the text
    let galley = ui.painter().layout_no_wrap(text.to_string(), font_id.clone(), color);
    let text_width = galley.rect.width();
    let height = galley.rect.height();

    // Case 1: Text fits -> Draw static
    if text_width <= available_width {
        // We use allocate_space to ensure we take up the verticval room,
        // but let alignment happen naturally
        ui.add(egui::Label::new(
            egui::RichText::new(text)
                .font(font_id)
                .color(color)
        ));
        return;
    }

    // Case 2: Text overflows -> Marqee Time!!
    // Allocate exact space in the UI Layout
    let (rect, _) = ui.allocate_exact_size(egui::vec2(available_width, height), egui::Sense::hover());

    let clip_rect = rect.intersect(ui.clip_rect());
    let painter = ui.painter().with_clip_rect(clip_rect);

    let time = ui.input(|i| i.time);
    let speed = 30.0; // Pixels per second
    
    let gap = (available_width / 3.0).max(200.0); // Space between loops
    let cycle_len = text_width + gap;

    // Calculate offset (movers leftward)
    let offset = (time * speed as f64) % cycle_len as f64;
    let x_start = rect.min.x - offset as f32;

    // Draw first instance
    painter.galley(egui::pos2(x_start, rect.min.y), galley.clone(), egui::Color32::WHITE);

    // Draw Loop Instance (if the first one has moved enough to reveal the gap)
    if x_start + text_width + gap < rect.max.x {
        painter.galley(egui::pos2(x_start + cycle_len as  f32, rect.min.y), galley, egui::Color32::WHITE);
    }

    // Request repaint to keep animation smooth
    ui.ctx().request_repaint();


}

/// Translates logical (u, v) coordinates into physical (x, y) coordinate based on orientation
/// 
/// * 'rect': The bounding box of the visualizer area.
/// * 'u': The logical position along the baseline (0.0 to baseline_length).
/// * 'v': The logical magnitude extending from the baseline (0.0 to max_magnitude).
/// * 'orientation': The cardinal direction the baseline is mounted to.
#[inline]
fn map_uv_to_xy(
    rect: egui::Rect,
    u: f32, v: f32,
    orientation: crate::shared_state::Orientation
) -> egui::Pos2{
    match orientation {
        crate::shared_state::Orientation::BottomUp => {
            // Baseline is Bottom edge, growing up (-Y)
            egui::pos2(rect.left() + u, rect.bottom() - v)
        }
        crate::shared_state::Orientation::TopDown => {
            // Baseline is Top edge, growing down (+Y)
            egui::pos2(rect.left() + u, rect.top() + v)
        }
        crate::shared_state::Orientation::LeftRight => {
            // Baseline is Left edge, growing right (+X)
            egui::pos2(rect.left() + v, rect.top() + u)
        }
        crate::shared_state::Orientation::RightLeft => {
            // Baseline is Right edge, growing left (-X)
            egui::pos2(rect.right() - v, rect.bottom() - u)
        }
    }
}