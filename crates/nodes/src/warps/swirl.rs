use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("swirl", "Swirl", CATEGORY)
        .desc("Vortex swirl distortion rotating image pixels around a center pivot proportional to radial distance.")
        .any_in("input", "Input")
        .primary_input("input")
        .float("angle", "Angle", 2.0, -10.0, 10.0)
        .float("radius", "Radius", 0.5, 0.05, 1.5)
        .float2("center", "Center", [0.5, 0.5], 0.0, 1.0)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let angle_param = context.f("angle") * PI;
    let radius = context.f("radius").clamp(0.01, 2.0);
    let center = context.v2("center");
    let tiling = context.tiling_axes();
    let kind = input.kind();

    Ok(vec![craft_texture::Image::generate_uv(context.w, context.h, kind, |u, v| {
        let dx = u - center[0];
        let dy = v - center[1];
        let dist = (dx * dx + dy * dy).sqrt();

        if dist < radius {
            let percent = (radius - dist) / radius;
            let theta = percent * percent * angle_param;
            let cos_t = theta.cos();
            let sin_t = theta.sin();

            let rx = dx * cos_t - dy * sin_t;
            let ry = dx * sin_t + dy * cos_t;

            input.sample_axes(center[0] + rx, center[1] + ry, tiling)
        } else {
            input.sample_axes(u, v, tiling)
        }
    })])
}
