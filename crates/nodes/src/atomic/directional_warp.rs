use super::ATOMIC;
use super::input_resize::sampled_input;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Image;

pub(super) fn definition() -> NodeDefinition {
    node("directional_warp", "Directional warp", ATOMIC)
        .desc("Offsets the input along a direction, by an intensity map.")
        .atomic()
        .any_in("input", "Input")
        .gray_in("intensity_input", "Intensity input")
        .float("intensity", "Intensity", 10.0, 0.0, 64.0)
        .angle("angle", "Warp angle", 0.0)
        .any_out()
        .cook(directional_warp)
}

fn directional_warp(context: &CookContext) -> CookResult {
    let target = [context.w, context.h].map(|side| side.clamp(1, craft_texture::MAX_SIDE));
    let tiling = context.tiling_axes();
    let (image, format) = sampled_input(context, 0);
    let (map, map_format) = sampled_input(context, 1);
    let map = map.to_gray();
    let same_size = [map.width(), map.height()] == target;
    let radians = context.f("angle") * std::f32::consts::TAU;
    // The horizontal phase includes a quarter turn rounded in radians before evaluating the sine.
    let component = |phase: f32| crate::function::truncate_float(f64::from(phase).sin());
    // At zero turns the full horizontal displacement reaches half-texel filter ties.
    let direction = if radians == 0.0 { [1.0, 0.0] } else { [component(radians + std::f32::consts::FRAC_PI_2), component(radians)] };
    const BASE_RESOLUTION: f32 = 256.0;
    let [width, height] = target.map(|side| side as f32);
    let strength = context.f("intensity") / BASE_RESOLUTION;
    let shorter = width.min(height);
    let step = [direction[0] * strength * (shorter / width), direction[1] * strength * (shorter / height)];
    Ok(vec![Image::generate(context.w, context.h, image.kind(), |x, y| {
        let (u, v) = ((x as f32 + 0.5) / width, (y as f32 + 0.5) / height);
        let amount =
            if same_size { map.get_gray(i64::from(x), i64::from(y), false) } else { crate::texture_sampler::sample(&map, [u, v], tiling, map_format, None)[0] };
        let uv = [step[0].mul_add(amount, u), step[1].mul_add(amount, v)];
        crate::texture_sampler::sample(&image, uv, tiling, format, None)
    })])
}
