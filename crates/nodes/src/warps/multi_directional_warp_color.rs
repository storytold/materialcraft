use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub const MODE_OPTIONS: &[&str] = &["Average", "Min", "Max"];
pub const DIRECTION_OPTIONS: &[&str] = &["2 Directions", "4 Directions", "8 Directions"];

pub(crate) fn definition() -> NodeDefinition {
    node("multi_directional_warp_color", "Multi Directional Warp Color", CATEGORY)
        .desc("Symmetrically warps a color image along 2, 4, or 8 directions driven by an intensity map.")
        .color_in("input", "Input")
        .primary_input("input")
        .gray_in("warp_angle", "Warp Angle")
        .float("intensity", "Intensity", 16.0, 0.0, 64.0)
        .choice("directions", "Directions", DIRECTION_OPTIONS, 1)
        .choice("mode", "Mode", MODE_OPTIONS, 0)
        .angle("warp_angle_param", "Warp Angle", 0.0)
        .color_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let warp_map = context.sized(1);

    let intensity = context.f("intensity").clamp(0.0, 64.0);
    let dir_choice = context.i("directions");
    let mode = context.i("mode");
    let base_angle = context.f("warp_angle_param") * PI;
    let tiling = context.tiling_axes();

    let num_dirs = match dir_choice {
        0 => 2,
        2 => 8,
        _ => 4,
    };

    let inv_w = 1.0 / context.w as f32;
    let inv_h = 1.0 / context.h as f32;
    let inv_res = inv_w.min(inv_h);

    let angle_step = 2.0 * PI / num_dirs as f32;

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let displacement = if let Some(wm) = &warp_map { wm.sample_axes(u, v, tiling)[0] * intensity * inv_res } else { 0.0 };

        if displacement < 1e-6 {
            return input.sample_axes(u, v, tiling);
        }

        match mode {
            // Min
            1 => {
                let mut best_color = [1.0f32; 4];
                let mut min_lum = f32::INFINITY;
                for d in 0..num_dirs {
                    let a = base_angle + d as f32 * angle_step;
                    let su = u + a.cos() * displacement;
                    let sv = v + a.sin() * displacement;
                    let s = input.sample_axes(su, sv, tiling);
                    let lum = 0.299 * s[0] + 0.587 * s[1] + 0.114 * s[2];
                    if lum < min_lum {
                        min_lum = lum;
                        best_color = s;
                    }
                }
                best_color
            }
            // Max
            2 => {
                let mut best_color = [0.0f32; 4];
                let mut max_lum = f32::NEG_INFINITY;
                for d in 0..num_dirs {
                    let a = base_angle + d as f32 * angle_step;
                    let su = u + a.cos() * displacement;
                    let sv = v + a.sin() * displacement;
                    let s = input.sample_axes(su, sv, tiling);
                    let lum = 0.299 * s[0] + 0.587 * s[1] + 0.114 * s[2];
                    if lum > max_lum {
                        max_lum = lum;
                        best_color = s;
                    }
                }
                best_color
            }
            // Average
            _ => {
                let mut accum = [0.0f32; 4];
                for d in 0..num_dirs {
                    let a = base_angle + d as f32 * angle_step;
                    let su = u + a.cos() * displacement;
                    let sv = v + a.sin() * displacement;
                    let s = input.sample_axes(su, sv, tiling);
                    for c in 0..4 {
                        accum[c] += s[c];
                    }
                }
                let inv_d = 1.0 / num_dirs as f32;
                [(accum[0] * inv_d).clamp(0.0, 1.0), (accum[1] * inv_d).clamp(0.0, 1.0), (accum[2] * inv_d).clamp(0.0, 1.0), (accum[3] * inv_d).clamp(0.0, 1.0)]
            }
        }
    })])
}
