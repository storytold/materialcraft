use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::TAU;

pub const DIRECTION_OPTIONS: &[&str] = &["2 Directions", "4 Directions", "8 Directions"];
pub const MODES: &[&str] = &["Average", "Min", "Max"];

pub(crate) fn definition() -> NodeDefinition {
    node("multi_directional_warp", "Multi Directional Warp Grayscale", CATEGORY)
        .desc("Warps an image in multiple symmetric directions simultaneously along a driver map to create organic distortion without directional streaking.")
        .any_in("input", "Input")
        .primary_input("input")
        .gray_in("warp", "Warp Map")
        .gray_in("mask", "Mask")
        .float("intensity", "Intensity", 16.0, 0.0, 128.0)
        .choice("directions", "Directions", DIRECTION_OPTIONS, 1)
        .choice("mode", "Mode", MODES, 0)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let warp_map = context.gray_or(1, 0.0);
    let mask_map = context.sized(2);

    let intensity_param = context.f("intensity");
    let dir_choice = context.i("directions");
    let mode = context.i("mode");

    let num_dirs = match dir_choice {
        0 => 2,
        2 => 8,
        _ => 4,
    };

    let scale = (context.w.min(context.h) as f32) / 256.0;
    let intensity = (intensity_param * scale).max(0.0);
    let tiling = context.tiling_axes();
    let tile = tiling[0] && tiling[1];

    let inv_w = 1.0 / context.w as f32;
    let inv_h = 1.0 / context.h as f32;
    let channels = input.channels().clamp(1, 4);

    if intensity <= 1e-4 {
        return Ok(vec![input]);
    }

    Ok(vec![Image::generate(context.w, context.h, input.kind(), |x, y| {
        let x_i = x as i64;
        let y_i = y as i64;
        let orig = input.get(x_i, y_i, tile);

        let m = mask_map.as_ref().map_or(1.0, |m_img| m_img.get_gray(x_i, y_i, tile).clamp(0.0, 1.0));
        if m <= 1e-4 {
            return orig;
        }

        let driver_val = warp_map.get_gray(x_i, y_i, tile);
        let dist = driver_val * intensity;

        let u_center = (x as f32 + 0.5) * inv_w;
        let v_center = (y as f32 + 0.5) * inv_h;

        let mut accum = [0.0f32; 4];
        let mut min_val = [f32::MAX; 4];
        let mut max_val = [f32::MIN; 4];

        for d in 0..num_dirs {
            let angle = (d as f32 / num_dirs as f32) * TAU;
            let u_sample = u_center + angle.cos() * dist * inv_w;
            let v_sample = v_center + angle.sin() * dist * inv_h;

            let s = input.sample_axes(u_sample, v_sample, tiling);
            for c in 0..channels {
                accum[c] += s[c];
                if s[c] < min_val[c] {
                    min_val[c] = s[c];
                }
                if s[c] > max_val[c] {
                    max_val[c] = s[c];
                }
            }
        }

        let mut res = [0.0f32; 4];
        let inv_dirs = 1.0 / num_dirs as f32;
        for c in 0..channels {
            res[c] = match mode {
                1 => min_val[c],
                2 => max_val[c],
                _ => accum[c] * inv_dirs,
            };
        }

        // Apply mask blend
        if (m - 1.0).abs() > 1e-4 {
            for c in 0..channels {
                res[c] = orig[c] * (1.0 - m) + res[c] * m;
            }
        }

        if channels == 1 { [res[0], 0.0, 0.0, 1.0] } else { res }
    })])
}
