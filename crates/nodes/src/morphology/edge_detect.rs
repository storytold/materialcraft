use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node, ops};
use craft_texture::{Image, Kind, Limits};
use rayon::prelude::*;

pub(crate) fn definition() -> NodeDefinition {
    node("edge_detect", "Edge Detect", CATEGORY)
        .desc("Extracts uniform-width contours and boundaries from patterns or masks with optional masking.")
        .gray_in("input", "Input")
        .primary_input("input")
        .gray_in("mask", "Mask")
        .float("edge_width", "Edge Width", 2.0, 1.0, 16.0)
        .float("edge_roundness", "Edge Roundness", 0.0, 0.0, 16.0)
        .float("tolerance", "Tolerance", 0.05, 0.0, 1.0)
        .boolean("invert", "Invert", false)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    let mask_img = context.sized(1);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    let width_param = if context.value("edge_width").is_some() { context.f("edge_width") } else { context.f("width") };
    let tolerance_param = if context.value("tolerance").is_some() { context.f("tolerance") } else { context.f("threshold") };
    let roundness_param = if context.value("edge_roundness").is_some() { context.f("edge_roundness") } else { context.f("roundness") };
    let invert = context.b("invert");

    let scale = (w as f32) / 256.0;
    let width_pixels = (width_param * scale).max(0.5);
    let threshold = tolerance_param.max(0.001);
    let roundness = (roundness_param * 0.1).clamp(0.0, 1.0);

    let total_pixels = (w as usize) * (h as usize);

    // Identify edge seed pixels: where neighbor difference exceeds threshold
    let mut is_edge = vec![false; total_pixels];

    is_edge.par_chunks_mut(w as usize).enumerate().for_each(|(y, row)| {
        let y_i = y as i64;
        for (x, edge_flag) in row.iter_mut().enumerate() {
            let x_i = x as i64;
            let current = input.get_gray(x_i, y_i, tile);
            let right = input.get_gray(x_i + 1, y_i, tile);
            let down = input.get_gray(x_i, y_i + 1, tile);

            let diff_x = (right - current).abs();
            let diff_y = (down - current).abs();

            *edge_flag = diff_x > threshold || diff_y > threshold;
        }
    });

    // Check if any edges were found
    let has_any_edges = is_edge.par_iter().any(|&e| e);
    if !has_any_edges {
        let flat_val = if invert { 1.0 } else { 0.0 };
        return if let Some(m_img) = &mask_img {
            Ok(vec![Image::generate(w, h, Kind::Gray, |x, y| {
                let m = m_img.get_gray(x as i64, y as i64, tile).clamp(0.0, 1.0);
                [flat_val * m, 0.0, 0.0, 1.0]
            })])
        } else {
            Ok(vec![Image::filled(w, h, Kind::Gray, [flat_val; 4])])
        };
    }

    let dist_sq = ops::distance_sq(w, h, |idx| is_edge.get(idx).copied().unwrap_or(false), tile);

    let mut out_data = vec![0.0f32; total_pixels];
    let half_width = (width_pixels * 0.5).max(0.25);
    let inv_half_width = 1.0 / half_width;

    out_data.par_chunks_mut(w as usize).enumerate().for_each(|(y, row)| {
        let row_offset = y * (w as usize);
        let y_i = y as i64;
        for (x, pixel) in row.iter_mut().enumerate() {
            let idx = row_offset + x;
            let x_i = x as i64;
            let d = dist_sq.get(idx).copied().unwrap_or(0.0).sqrt();
            let u = d * inv_half_width;

            let intensity = if u >= 1.0 {
                0.0
            } else {
                let linear = 1.0 - u;
                if roundness > 0.0 {
                    let smooth = (1.0 - u * u).max(0.0);
                    linear * (1.0 - roundness) + smooth * roundness
                } else {
                    linear
                }
            };

            let m = mask_img.as_ref().map_or(1.0, |m_img| m_img.get_gray(x_i, y_i, tile).clamp(0.0, 1.0));
            let val = if invert { 1.0 - intensity } else { intensity };
            *pixel = (val * m).clamp(0.0, 1.0);
        }
    });

    let result = Image::from_data(w, h, Kind::Gray, out_data, &Limits::default()).unwrap_or_else(|_| input.clone());
    Ok(vec![result])
}
