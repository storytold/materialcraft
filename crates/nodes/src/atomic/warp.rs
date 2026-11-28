use super::ATOMIC;
use super::input_resize::sampled_input;
use crate::{CookContext, CookResult, NodeDefinition, node, texture_sampler};
use craft_texture::Image;
use materialcraft_core::Value;

pub(super) fn definition() -> NodeDefinition {
    node("warp", "Warp", ATOMIC)
        .desc("Distorts the input along the slopes of a gradient map.")
        .atomic()
        .any_in("input", "Input")
        .gray_in("gradient", "Gradient input")
        .float("intensity", "Intensity", 1.0, 0.0, 32.0)
        .boolean("nearest", "Nearest filtering", false)
        .any_out()
        .cook(warp)
}

fn warp(context: &CookContext) -> CookResult {
    let (width, height) = craft_texture::clamp_size(context.w, context.h);
    let (image, format) = sampled_input(context, 0);
    let (map, map_format) = sampled_input(context, 1);
    let map = map.to_gray();
    let same_size = (map.width(), map.height()) == (width, height);
    let (w, h) = (width as f32, height as f32);
    let (mw, mh) = (map.width() as f32, map.height() as f32);
    let stretched = context.vars.get("$pixelratio").and_then(Value::as_i32) == Some(0);
    let [extent_x, extent_y] = if stretched { [w, h] } else { [w.min(h); 2] };
    let intensity = context.f("intensity");
    // A slope spans one stored gradient texel. Stretched pixels measure displacement separately along each output axis.
    let axis_scale = |extent: f32, output: f32, gradient: f32| (intensity * (extent / 256.0)) * (extent / output) * (gradient / output);
    let scale = [axis_scale(extent_x, w, mw), axis_scale(extent_y, h, mh)];
    let tile = context.tiling_axes();
    let nearest = context.b("nearest");
    Ok(vec![Image::generate(width, height, image.kind(), |x, y| {
        let (u, v) = ((x as f32 + 0.5) / w, (y as f32 + 0.5) / h);
        let (center, left, above) = if same_size {
            let (x, y) = (i64::from(x), i64::from(y));
            (map.get_gray(x, y, false), map.get_axes(x - 1, y, tile)[0], map.get_axes(x, y - 1, tile)[0])
        } else {
            let read = |u, v| texture_sampler::sample(&map, [u, v], tile, map_format, None)[0];
            (read(u, v), read(u - mw.recip(), v), read(u, v - mh.recip()))
        };
        let uv = [scale[0].mul_add(center - left, u), scale[1].mul_add(center - above, v)];
        if nearest { image.sample_nearest_axes(uv[0], uv[1], tile) } else { texture_sampler::sample(&image, uv, tile, format, None) }
    })])
}
