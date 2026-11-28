use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("star", "Star", CATEGORY)
        .desc("Procedural star generator with ray count, inner/outer radius, rotation, and profile gradient.")
        .int("rays", "Rays", 5, 3, 32)
        .float("inner_radius", "Inner Radius", 0.4, 0.01, 1.0)
        .float("outer_radius", "Outer Radius", 0.9, 0.05, 1.5)
        .angle("rotation", "Rotation", 0.0)
        .boolean("gradient", "Gradient Profile", false)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let rays = context.i("rays").clamp(3, 32) as f32;
    let r_inner = context.f("inner_radius").clamp(0.005, 1.0) * 0.5;
    let r_outer = context.f("outer_radius").clamp(0.01, 1.5) * 0.5;
    let rot = context.f("rotation") * PI;
    let is_gradient = context.b("gradient");

    let cos_r = rot.cos();
    let sin_r = rot.sin();
    let ray_step = PI / rays;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;

        let rx = x * cos_r - y * sin_r;
        let ry = x * sin_r + y * cos_r;

        let r = (rx * rx + ry * ry).sqrt();
        let mut theta = ry.atan2(rx);
        if theta < 0.0 {
            theta += 2.0 * PI;
        }

        let step_idx = (theta / ray_step).floor();
        let t = (theta / ray_step).fract();

        // Linear interpolation between inner and outer tip
        let star_r = if (step_idx as i32).rem_euclid(2) == 0 { r_outer * (1.0 - t) + r_inner * t } else { r_inner * (1.0 - t) + r_outer * t };

        let val = if r <= star_r { if is_gradient { (1.0 - r / star_r).clamp(0.0, 1.0) } else { 1.0 } } else { 0.0 };

        [val, 0.0, 0.0, 1.0]
    })])
}
