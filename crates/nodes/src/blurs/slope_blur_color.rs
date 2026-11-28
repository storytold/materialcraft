use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub const MODE_OPTIONS: &[&str] = &["Blur", "Min/Chipping", "Max"];

pub(crate) fn definition() -> NodeDefinition {
    node("slope_blur_color", "Slope Blur Color", CATEGORY)
        .desc("Directional erosion and slope-guided blur along a height map gradient for color textures.")
        .color_in("input", "Input")
        .primary_input("input")
        .gray_in("slope", "Slope Map")
        .gray_in("mask", "Mask")
        .int("samples", "Samples", 16, 1, 32)
        .float("intensity", "Intensity", 4.0, 0.0, 16.0)
        .choice("mode", "Mode", MODE_OPTIONS, 0)
        .color_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let slope = context.sized(1);
    let mask = context.sized(2);

    let samples = context.i("samples").clamp(1, 32) as usize;
    let intensity = context.f("intensity").clamp(0.0, 16.0);
    let mode = context.i("mode");
    let tiling = context.tiling_axes();

    let inv_w = 1.0 / context.w as f32;
    let inv_h = 1.0 / context.h as f32;
    let step_scale = intensity * 0.01 / samples as f32;

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let mask_val = if let Some(m) = &mask { m.sample_axes(u, v, tiling)[0].clamp(0.0, 1.0) } else { 1.0 };

        if mask_val < 1e-4 || slope.is_none() {
            return input.sample_axes(u, v, tiling);
        }

        let slope_img = match &slope {
            Some(s) => s,
            None => return input.sample_axes(u, v, tiling),
        };

        // Estimate local gradient
        let h_r = slope_img.sample_axes(u + inv_w, v, tiling)[0];
        let h_l = slope_img.sample_axes(u - inv_w, v, tiling)[0];
        let h_d = slope_img.sample_axes(u, v + inv_h, tiling)[0];
        let h_u = slope_img.sample_axes(u, v - inv_h, tiling)[0];

        let gx = (h_r - h_l) * 0.5;
        let gy = (h_d - h_u) * 0.5;
        let g_len = (gx * gx + gy * gy).sqrt();

        if g_len < 1e-6 {
            return input.sample_axes(u, v, tiling);
        }

        let dir_x = (gx / g_len) * step_scale * mask_val;
        let dir_y = (gy / g_len) * step_scale * mask_val;

        let mut current_u = u;
        let mut current_v = v;

        match mode {
            // Min / Chipping
            1 => {
                let mut best_color = input.sample_axes(u, v, tiling);
                let mut min_lum = 0.299 * best_color[0] + 0.587 * best_color[1] + 0.114 * best_color[2];
                for _ in 1..samples {
                    current_u += dir_x;
                    current_v += dir_y;
                    let sample = input.sample_axes(current_u, current_v, tiling);
                    let lum = 0.299 * sample[0] + 0.587 * sample[1] + 0.114 * sample[2];
                    if lum < min_lum {
                        min_lum = lum;
                        best_color = sample;
                    }
                }
                best_color
            }
            // Max
            2 => {
                let mut best_color = input.sample_axes(u, v, tiling);
                let mut max_lum = 0.299 * best_color[0] + 0.587 * best_color[1] + 0.114 * best_color[2];
                for _ in 1..samples {
                    current_u += dir_x;
                    current_v += dir_y;
                    let sample = input.sample_axes(current_u, current_v, tiling);
                    let lum = 0.299 * sample[0] + 0.587 * sample[1] + 0.114 * sample[2];
                    if lum > max_lum {
                        max_lum = lum;
                        best_color = sample;
                    }
                }
                best_color
            }
            // Blur (average)
            _ => {
                let mut accum = input.sample_axes(u, v, tiling);
                for _ in 1..samples {
                    current_u += dir_x;
                    current_v += dir_y;
                    let sample = input.sample_axes(current_u, current_v, tiling);
                    for c in 0..3 {
                        accum[c] += sample[c];
                    }
                }
                let inv_s = 1.0 / samples as f32;
                [(accum[0] * inv_s).clamp(0.0, 1.0), (accum[1] * inv_s).clamp(0.0, 1.0), (accum[2] * inv_s).clamp(0.0, 1.0), accum[3]]
            }
        }
    })])
}
