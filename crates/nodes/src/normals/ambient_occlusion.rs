use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind, Limits};
use rayon::prelude::*;

pub const QUALITY_OPTIONS: &[&str] = &["Low (8 samples)", "Medium (16 samples)", "High (24 samples)"];

pub(crate) fn definition() -> NodeDefinition {
    node("ambient_occlusion", "Ambient Occlusion (HBAO)", CATEGORY)
        .desc("Calculates Horizon-Based Ambient Occlusion (HBAO) from a grayscale height map with optional masking.")
        .gray_in("height", "Height")
        .primary_input("height")
        .gray_in("mask", "Mask")
        .float("radius", "Radius", 16.0, 1.0, 128.0)
        .float("height_depth", "Height Depth", 1.0, 0.05, 10.0)
        .float("intensity", "Intensity", 1.0, 0.0, 3.0)
        .float("spread", "Spread", 1.0, 0.1, 3.0)
        .choice("quality", "Quality", QUALITY_OPTIONS, 1)
        .gray_out()
        .cook(cook)
}

#[inline]
fn sample_bilinear(img: &Image, x: f32, y: f32, tile: bool) -> f32 {
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let ix0 = x0 as i64;
    let iy0 = y0 as i64;
    let ix1 = ix0 + 1;
    let iy1 = iy0 + 1;

    let p00 = img.get_gray(ix0, iy0, tile);
    let p10 = img.get_gray(ix1, iy0, tile);
    let p01 = img.get_gray(ix0, iy1, tile);
    let p11 = img.get_gray(ix1, iy1, tile);

    let top = p00 * (1.0 - fx) + p10 * fx;
    let bottom = p01 * (1.0 - fx) + p11 * fx;
    top * (1.0 - fy) + bottom * fy
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    let mask_img = context.sized(1);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    let scale = (w as f32) / 256.0;
    let radius = (context.f("radius") * scale).max(1.0).min(w as f32);
    let height_depth = context.f("height_depth").max(0.001);
    let intensity = context.f("intensity").max(0.0);
    let spread = context.f("spread").max(0.1);
    let quality = context.i("quality");

    let (num_dirs, num_steps) = match quality {
        0 => (8, 6),
        2 => (16, 16),
        _ => (12, 10),
    };

    let total_pixels = (w as usize) * (h as usize);
    let mut out_data = vec![1.0f32; total_pixels];

    out_data.par_chunks_mut(w as usize).enumerate().for_each(|(y, row)| {
        let py = y as f32;
        let y_i = y as i64;
        for (x, pixel) in row.iter_mut().enumerate() {
            let px = x as f32;
            let x_i = x as i64;

            let m = mask_img.as_ref().map_or(1.0, |m_img| m_img.get_gray(x_i, y_i, tile).clamp(0.0, 1.0));
            if m <= 1e-4 {
                *pixel = 1.0;
                continue;
            }

            let h0 = input.get_gray(x_i, y_i, tile) * height_depth;

            // Deterministic spatial rotation jitter
            let spatial_hash = (px * 0.067_110_56 + py * 0.005_837_15).fract();
            let jitter = ((spatial_hash * 52.982_92).fract()) * (std::f32::consts::TAU / num_dirs as f32);

            let mut total_occlusion = 0.0f32;

            for d in 0..num_dirs {
                let angle = (d as f32 / num_dirs as f32) * std::f32::consts::TAU + jitter;
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                let mut max_tan = 0.0f32;

                for s in 1..=num_steps {
                    let step_norm = (s as f32) / (num_steps as f32);
                    let dist = (step_norm * step_norm * radius).max(1.0);
                    let sample_x = px + cos_a * dist;
                    let sample_y = py + sin_a * dist;

                    let hs = sample_bilinear(&input, sample_x, sample_y, tile) * height_depth;
                    let diff = hs - h0;

                    if diff > 1e-4 {
                        let dist_factor = (1.0 - (dist / radius) * spread * 0.5).max(0.0);
                        let tan_elevation = (diff / dist) * dist_factor;
                        if tan_elevation > max_tan {
                            max_tan = tan_elevation;
                        }
                    }
                }

                // Horizon elevation angle
                let sin_horizon = max_tan / (1.0 + max_tan * max_tan).sqrt();
                total_occlusion += sin_horizon;
            }

            let avg_occlusion = total_occlusion / (num_dirs as f32);
            let raw_ao = (1.0 - avg_occlusion * intensity).clamp(0.0, 1.0);
            *pixel = 1.0 - (1.0 - raw_ao) * m;
        }
    });

    let result = Image::from_data(w, h, Kind::Gray, out_data, &Limits::default()).unwrap_or_else(|_| input.clone());
    Ok(vec![result])
}
