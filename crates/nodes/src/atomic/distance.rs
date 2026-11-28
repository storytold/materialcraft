use super::ATOMIC;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use materialcraft_core::Value;

mod mask_field;

pub(super) fn definition() -> NodeDefinition {
    node("distance", "Distance", ATOMIC)
        .desc("Grows white areas of a mask into a distance gradient; with a source, spreads its colors outward.")
        .atomic()
        .gray_in("mask", "Mask input")
        .any_in("source", "Source input")
        .boolean("color_mode", "Color output", false)
        .float("distance", "Maximum distance", 10.0, 0.0, 256.0)
        .choice("metric", "Distance metric", &["Euclidean", "Manhattan", "Chebyshev"], 0)
        .boolean("combine", "Combine distance with source", true)
        .any_out()
        .cook(distance)
}

fn distance(context: &CookContext) -> CookResult {
    let mask = context.gray_or(0, 0.0);
    let source = context.sized(1);
    let kind = if context.b("color_mode") { Kind::Color } else { Kind::Gray };
    // An incompatible source produces zero channels. Only a disconnected source produces a mask ramp.
    if source.as_ref().is_some_and(|image| image.kind() != kind) {
        return Ok(vec![Image::filled(mask.width(), mask.height(), kind, [0.0; 4])]);
    }
    let radius = (context.f("distance").max(0.0) * (mask.width().min(mask.height()) as f32 / 256.0)).min(f32::MAX);
    let stretched = context.vars.get("$pixelratio").and_then(Value::as_i32) == Some(0);
    let image = mask_field::render(&mask, source.as_ref(), radius, context.tiling_axes(), context.i("metric"), context.b("combine"), stretched)?;
    Ok(vec![if kind == Kind::Color && image.is_gray() { image.to_color() } else { image }])
}
