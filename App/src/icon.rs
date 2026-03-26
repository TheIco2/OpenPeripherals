// OpenPeripheral icon generator — creates RGBA icon data programmatically.
//
// Produces a clean "OP" branded icon with the app's green accent on dark
// background, suitable for both system tray (32×32) and window icons.

const ACCENT_R: u8 = 34;  // #22c55e green-500
const ACCENT_G: u8 = 197;
const ACCENT_B: u8 = 94;

const BG_R: u8 = 15;  // #0f172a slate-900
const BG_G: u8 = 23;
const BG_B: u8 = 42;

/// Generate the OpenPeripheral tray icon (32×32 RGBA).
pub fn generate_tray_icon() -> (Vec<u8>, u32, u32) {
    generate_icon(32)
}

/// Generate the OpenPeripheral window icon (64×64 RGBA).
pub fn generate_window_icon() -> (Vec<u8>, u32, u32) {
    generate_icon(64)
}

fn generate_icon(size: u32) -> (Vec<u8>, u32, u32) {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let s = size as f32;
    let corner_radius = s * 0.2;

    for y in 0..size {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            let fx = x as f32;
            let fy = y as f32;

            // Rounded rectangle mask via distance from edges.
            let alpha = rounded_rect_alpha(fx, fy, s, s, corner_radius);
            if alpha <= 0.0 {
                continue;
            }

            // Background with subtle gradient (darker at top, lighter at bottom).
            let gradient_t = fy / s;
            let bg_r = BG_R as f32 + 8.0 * gradient_t;
            let bg_g = BG_G as f32 + 10.0 * gradient_t;
            let bg_b = BG_B as f32 + 12.0 * gradient_t;

            // Draw "OP" letters using simple geometric shapes.
            let (lr, lg, lb) = draw_op_letters(fx, fy, s);

            // Composite letter color over background.
            let r = bg_r * (1.0 - lr) + ACCENT_R as f32 * lr;
            let g = bg_g * (1.0 - lg) + ACCENT_G as f32 * lg;
            let b = bg_b * (1.0 - lb) + ACCENT_B as f32 * lb;

            rgba[i] = r.clamp(0.0, 255.0) as u8;
            rgba[i + 1] = g.clamp(0.0, 255.0) as u8;
            rgba[i + 2] = b.clamp(0.0, 255.0) as u8;
            rgba[i + 3] = (alpha * 255.0).clamp(0.0, 255.0) as u8;
        }
    }

    (rgba, size, size)
}

/// Returns 0.0–1.0 alpha for a rounded rectangle.
fn rounded_rect_alpha(x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    // Distance from nearest corner circle.
    let dx = if x < r {
        r - x
    } else if x > w - r {
        x - (w - r)
    } else {
        0.0
    };
    let dy = if y < r {
        r - y
    } else if y > h - r {
        y - (h - r)
    } else {
        0.0
    };

    if dx > 0.0 && dy > 0.0 {
        let dist = (dx * dx + dy * dy).sqrt();
        if dist > r + 0.5 {
            0.0
        } else if dist > r - 0.5 {
            (r + 0.5 - dist).clamp(0.0, 1.0)
        } else {
            1.0
        }
    } else {
        1.0
    }
}

/// Draw "OP" letters — returns intensity (0.0–1.0) for the accent color.
fn draw_op_letters(x: f32, y: f32, size: f32) -> (f32, f32, f32) {
    // Normalised coordinates (0..1).
    let nx = x / size;
    let ny = y / size;
    let stroke = 0.08; // stroke width as fraction of size

    let mut intensity: f32 = 0.0;

    // "O" — centered ring in left half.
    let o_cx = 0.32;
    let o_cy = 0.50;
    let o_outer = 0.18;
    let o_inner = o_outer - stroke;
    let o_dist = ((nx - o_cx) * (nx - o_cx) + (ny - o_cy) * (ny - o_cy)).sqrt();
    if o_dist <= o_outer + 0.02 && o_dist >= o_inner - 0.02 {
        let outer_edge = smoothstep(o_outer + 0.02, o_outer - 0.01, o_dist);
        let inner_edge = smoothstep(o_inner - 0.02, o_inner + 0.01, o_dist);
        intensity = intensity.max(outer_edge * inner_edge);
    }

    // "P" — vertical stroke + half-circle bump in right half.
    let p_left = 0.58;
    let p_top = 0.28;
    let p_bottom = 0.72;

    // Vertical stroke of P.
    if nx >= p_left && nx <= p_left + stroke && ny >= p_top && ny <= p_bottom {
        intensity = intensity.max(1.0);
    }

    // P bump (half-circle on right side, upper half).
    let p_bump_cx = p_left + stroke;
    let p_bump_cy = (p_top + (p_top + p_bottom) / 2.0) / 2.0 + 0.04;
    let p_bump_outer = 0.15;
    let p_bump_inner = p_bump_outer - stroke;
    let p_dist = ((nx - p_bump_cx) * (nx - p_bump_cx) + (ny - p_bump_cy) * (ny - p_bump_cy)).sqrt();
    if nx >= p_left + stroke * 0.5 && p_dist <= p_bump_outer + 0.02 && p_dist >= p_bump_inner - 0.02 {
        let outer_e = smoothstep(p_bump_outer + 0.02, p_bump_outer - 0.01, p_dist);
        let inner_e = smoothstep(p_bump_inner - 0.02, p_bump_inner + 0.01, p_dist);
        intensity = intensity.max(outer_e * inner_e);
    }

    // Horizontal connector at top of P bump.
    if nx >= p_left && nx <= p_left + stroke + p_bump_outer * 0.3
        && ny >= p_top && ny <= p_top + stroke
    {
        intensity = intensity.max(1.0);
    }

    let i = intensity.clamp(0.0, 1.0);
    (i, i, i)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
