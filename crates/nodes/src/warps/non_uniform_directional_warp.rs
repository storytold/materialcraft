use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("non_uniform_directional_warp", "Non Uniform Directional Warp", CATEGORY)
        .desc("Directional warp where displacement intensity varies per-pixel driven by an intensity mask map.")
        .any_in("input", "Input")
        .primary_input("input")
        .gray_in("intensity_map", "Intensity Map")
        .angle("angle", "Angle", 0.0)
        .float("intensity", "Intensity", 16.0, 0.0, 64.0)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let intensity_map = context.sized(1);
    let angle = context.f("angle") * PI;
    let intensity = context.f("intensity").clamp(0.0, 64.0);
    let tiling = context.tiling_axes();
    let kind = input.kind();

    let cos_a = angle.cos();
    let sin_a = angle.sin();
    let inv_res = 1.0 / context.w.max(context.h) as f32;
    let base_step = intensity * inv_res;

    Ok(vec![craft_texture::Image::generate_uv(context.w, context.h, kind, |u, v| {
        let mask = intensity_map.as_ref().map_or(1.0, |m| m.sample_axes(u, v, tiling)[0].clamp(0.0, 1.0));
        let disp = mask * base_step;

        let sample_u = u - cos_a * disp;
        let sample_v = v - sin_a * disp;

        input.sample_axes(sample_u, sample_v, tiling)
    })])
}
