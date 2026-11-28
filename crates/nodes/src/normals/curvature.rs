use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, PortKind, node};
use craft_texture::{Image, Kind, Limits};
use rayon::prelude::*;

pub const MODES: &[&str] = &["Combined", "Convex / Edges", "Concave / Cavities"];

pub(crate) fn definition() -> NodeDefinition {
    node("curvature", "Curvature", CATEGORY)
        .desc("Extracts surface curvature (combined map, isolated convex edges, and isolated concave cavities) from a normal map or height map.")
        .any_in("input", "Normal or Height")
        .primary_input("input")
        .float("intensity", "Intensity", 1.0, 0.0, 10.0)
        .float("smoothness", "Smoothness", 1.0, 0.0, 16.0)
        .choice("mode", "Mode", MODES, 0)
        .output("output", "Curvature", PortKind::Gray)
        .output("convexity", "Convexity", PortKind::Gray)
        .output("concavity", "Concavity", PortKind::Gray)
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let (w, h) = (context.w, context.h);
    let tile = context.tile;

    let intensity = context.f("intensity").max(0.0);
    let smoothness = context.f("smoothness").max(0.0);
    let mode = context.i("mode");

    let scale = (w as f32) / 256.0;
    let step = (smoothness * scale).round() as i64;
    let step = step.max(1);

    let is_color = input.kind() == Kind::Color;
    let total_pixels = (w as usize) * (h as usize);

    let mut main_data = vec![0.5f32; total_pixels];
    let mut convex_data = vec![0.0f32; total_pixels];
    let mut concave_data = vec![0.0f32; total_pixels];

    // Parallel row computation: compute main, convex, and concave in one pass
    main_data.par_chunks_mut(w as usize).zip(convex_data.par_chunks_mut(w as usize)).zip(concave_data.par_chunks_mut(w as usize)).enumerate().for_each(
        |(y, ((main_row, convex_row), concave_row))| {
            let y_i = y as i64;
            for (x, ((main_pixel, convex_pixel), concave_pixel)) in main_row.iter_mut().zip(convex_row.iter_mut()).zip(concave_row.iter_mut()).enumerate() {
                let x_i = x as i64;

                let curvature = if is_color {
                    let right = input.get(x_i + step, y_i, tile);
                    let left = input.get(x_i - step, y_i, tile);
                    let down = input.get(x_i, y_i + step, tile);
                    let up = input.get(x_i, y_i - step, tile);

                    let nx_r = right[0] * 2.0 - 1.0;
                    let nx_l = left[0] * 2.0 - 1.0;
                    let ny_d = down[1] * 2.0 - 1.0;
                    let ny_u = up[1] * 2.0 - 1.0;

                    let div_x = (nx_r - nx_l) / (2.0 * step as f32);
                    let div_y = (ny_d - ny_u) / (2.0 * step as f32);
                    (div_x + div_y) * 4.0
                } else {
                    let c = input.get_gray(x_i, y_i, tile);
                    let r = input.get_gray(x_i + step, y_i, tile);
                    let l = input.get_gray(x_i - step, y_i, tile);
                    let d = input.get_gray(x_i, y_i + step, tile);
                    let u = input.get_gray(x_i, y_i - step, tile);

                    let laplacian = (r + l + d + u - 4.0 * c) / ((step * step) as f32);
                    -laplacian * 16.0
                };

                let k = curvature * intensity;
                let comb = (0.5 + 0.5 * k).clamp(0.0, 1.0);
                let convex = k.clamp(0.0, 1.0);
                let concave = (-k).clamp(0.0, 1.0);

                *convex_pixel = convex;
                *concave_pixel = concave;
                *main_pixel = match mode {
                    1 => convex,
                    2 => concave,
                    _ => comb,
                };
            }
        },
    );

    let limits = Limits::default();
    let main_img = Image::from_data(w, h, Kind::Gray, main_data, &limits).unwrap_or_else(|_| input.to_gray());
    let convex_img = Image::from_data(w, h, Kind::Gray, convex_data, &limits).unwrap_or_else(|_| Image::filled(w, h, Kind::Gray, [0.0; 4]));
    let concave_img = Image::from_data(w, h, Kind::Gray, concave_data, &limits).unwrap_or_else(|_| Image::filled(w, h, Kind::Gray, [0.0; 4]));

    Ok(vec![main_img, convex_img, concave_img])
}
