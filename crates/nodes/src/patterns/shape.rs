use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub const SHAPES: &[&str] = &["Disc", "Square", "Paraboloid", "Bell", "Cone", "Pyramid", "Hemisphere", "Wave"];

pub(crate) fn definition() -> NodeDefinition {
    node("shape", "Shape", CATEGORY)
        .desc("Generates parametric geometric 2D shapes, forms, and height profiles.")
        .generator()
        .choice("pattern", "Pattern", SHAPES, 0)
        .float("scale", "Scale", 1.0, 0.0, 2.0)
        .float2("size", "Size", [1.0, 1.0], 0.0, 2.0)
        .float("roundness", "Roundness", 0.0, 0.0, 1.0)
        .angle("angle", "Angle", 0.0)
        .boolean("tiling", "Tiling", false)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let pattern = context.i("pattern");
    let scale = context.f("scale");
    let size = context.v2("size");
    let roundness = context.f("roundness");
    let angle = context.f("angle") * PI;
    let tile = context.b("tiling");

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    let sx = (scale * size[0]).max(1e-4);
    let sy = (scale * size[1]).max(1e-4);
    let r_round = (roundness * 0.5).clamp(0.0, 0.499);

    let (w_f, h_f) = (context.w as f32, context.h as f32);
    let pixel_radius = 1.0 / (w_f.min(h_f) * scale.max(0.1));

    Ok(vec![Image::generate(context.w, context.h, Kind::Gray, |x, y| {
        let u = (x as f32 + 0.5) / w_f;
        let v = (y as f32 + 0.5) / h_f;

        let (dx, dy) = if tile {
            let u_t = (u - 0.5).rem_euclid(1.0) - 0.5;
            let v_t = (v - 0.5).rem_euclid(1.0) - 0.5;
            (u_t, v_t)
        } else {
            (u - 0.5, v - 0.5)
        };

        // Rotate
        let rx = dx * cos_a - dy * sin_a;
        let ry = dx * sin_a + dy * cos_a;

        // Scale
        let px = rx / sx;
        let py = ry / sy;

        let val = match pattern {
            // Disc
            0 => {
                let dist = (px * px + py * py).sqrt();
                ((0.5 - dist) / pixel_radius).clamp(0.0, 1.0)
            }
            // Square (with optional roundness)
            1 => {
                if r_round <= 1e-4 {
                    let d = px.abs().max(py.abs());
                    ((0.5 - d) / pixel_radius).clamp(0.0, 1.0)
                } else {
                    let qx = px.abs() - (0.5 - r_round);
                    let qy = py.abs() - (0.5 - r_round);
                    let dist = (qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0)) - r_round;
                    ((-dist) / pixel_radius).clamp(0.0, 1.0)
                }
            }
            // Paraboloid
            2 => {
                let dist_sq = px * px + py * py;
                if dist_sq < 0.25 { (1.0 - dist_sq * 4.0).max(0.0) } else { 0.0 }
            }
            // Bell (Gaussian)
            3 => {
                let dist_sq = px * px + py * py;
                (-16.0 * dist_sq).exp()
            }
            // Cone
            4 => {
                let dist = (px * px + py * py).sqrt();
                (1.0 - dist * 2.0).clamp(0.0, 1.0)
            }
            // Pyramid
            5 => {
                let d = px.abs().max(py.abs());
                (1.0 - d * 2.0).clamp(0.0, 1.0)
            }
            // Hemisphere
            6 => {
                let dist_sq = px * px + py * py;
                if dist_sq < 0.25 { (1.0 - dist_sq * 4.0).sqrt() } else { 0.0 }
            }
            // Wave
            7 if py.abs() <= 0.5 => 0.5 + 0.5 * (px * 2.0 * PI).sin(),
            7 => 0.0,
            _ => 0.0,
        };

        [val.clamp(0.0, 1.0), 0.0, 0.0, 0.0]
    })])
}
