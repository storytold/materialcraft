use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, PortKind, node, ops};
use craft_texture::{Image, Kind, Limits};
use rayon::prelude::*;

pub const CURVE_TYPES: &[&str] = &["Linear", "Smooth", "Round (Convex)", "Concave"];
pub const CORNER_TYPES: &[&str] = &["Round", "Angular"];

pub(crate) fn definition() -> NodeDefinition {
    node("bevel", "Bevel", CATEGORY)
        .desc("Creates a beveled edge profile inwards from the boundaries of a mask or height shape, outputting both beveled height and normal map.")
        .gray_in("input", "Input")
        .primary_input("input")
        .float("distance", "Distance", 4.0, 0.0, 64.0)
        .choice("corner_type", "Corner Type", CORNER_TYPES, 0)
        .choice("curve_type", "Curve Type", CURVE_TYPES, 0)
        .float("smoothing", "Smoothing", 0.0, 0.0, 16.0)
        .boolean("invert", "Invert", false)
        .output("output", "Height", PortKind::Gray)
        .output("normal", "Normal", PortKind::Color)
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    let dist_param = context.f("distance");
    let smoothing_param = context.f("smoothing");
    let corner_type = context.i("corner_type");
    let curve_type = context.i("curve_type");
    let invert = context.b("invert");

    let scale = (w as f32) / 256.0;
    let dist_pixels = (dist_param * scale).max(0.0);
    let smoothing_pixels = (smoothing_param * scale).max(0.0);

    let src_data = input.data();
    let total_pixels = (w as usize) * (h as usize);

    if dist_pixels <= 1e-4 {
        let mut out = if invert {
            Image::generate(w, h, Kind::Gray, |x, y| {
                let v = input.get_gray(x as i64, y as i64, tile);
                [1.0 - v, 0.0, 0.0, 1.0]
            })
        } else {
            input
        };
        if smoothing_pixels > 0.05 {
            out = ops::gaussian(&out, smoothing_pixels, tile);
        }
        let normal = crate::atomic::height_to_normal(&out, 1.0, false, tile, false);
        return Ok(vec![out, normal]);
    }

    // Determine boundaries: pixels where mask is considered outside/background
    let is_outside = |idx: usize| -> bool {
        let val = src_data.get(idx).copied().unwrap_or(0.0);
        if invert { val > 0.5 } else { val <= 0.5 }
    };

    let (dist_sq, nearest_sites) = ops::distance_sq_with_sites(w, h, is_outside, tile);

    let mut out_data = vec![0.0f32; total_pixels];
    let inv_dist = 1.0 / dist_pixels;

    out_data.par_chunks_mut(w as usize).enumerate().for_each(|(y, row)| {
        let row_offset = y * (w as usize);
        for (x, pixel) in row.iter_mut().enumerate() {
            let idx = row_offset + x;
            let m_val = src_data.get(idx).copied().unwrap_or(0.0);
            let inside = if invert { m_val <= 0.5 } else { m_val > 0.5 };

            if !inside {
                *pixel = 0.0;
                continue;
            }

            let d = if corner_type == 1 {
                // Angular (L-infinity Chebychev distance to boundary site)
                let site_idx = nearest_sites.get(idx).copied().unwrap_or(usize::MAX);
                if site_idx != usize::MAX {
                    let sx = (site_idx % (w as usize)) as f32;
                    let sy = (site_idx / (w as usize)) as f32;
                    let mut dx = (x as f32 - sx).abs();
                    let mut dy = (y as f32 - sy).abs();
                    if tile {
                        dx = dx.min(w as f32 - dx);
                        dy = dy.min(h as f32 - dy);
                    }
                    dx.max(dy)
                } else {
                    dist_sq.get(idx).copied().unwrap_or(0.0).sqrt()
                }
            } else {
                dist_sq.get(idx).copied().unwrap_or(0.0).sqrt()
            };

            let t = (d * inv_dist).clamp(0.0, 1.0);

            let profile = match curve_type {
                // Smooth (cubic smoothstep)
                1 => t * t * (3.0 - 2.0 * t),
                // Round / Convex quarter-circle
                2 => {
                    let rem = 1.0 - t;
                    (1.0 - rem * rem).max(0.0).sqrt()
                }
                // Concave quarter-circle
                3 => 1.0 - (1.0 - t * t).max(0.0).sqrt(),
                // Linear
                _ => t,
            };

            let height_scale = if invert { 1.0 - m_val } else { m_val };
            *pixel = (profile * height_scale).clamp(0.0, 1.0);
        }
    });

    let mut height_result = Image::from_data(w, h, Kind::Gray, out_data, &Limits::default()).unwrap_or_else(|_| input.clone());

    if smoothing_pixels > 0.05 {
        height_result = ops::gaussian(&height_result, smoothing_pixels, tile);
    }

    let normal_result = crate::atomic::height_to_normal(&height_result, 1.0, false, tile, false);

    Ok(vec![height_result, normal_result])
}
