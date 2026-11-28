use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(crate) fn definition() -> NodeDefinition {
    node("blur_hq", "Blur HQ", CATEGORY)
        .desc("High quality multi-pass Gaussian blur with precise fractional radius.")
        .any_in("input", "Input")
        .primary_input("input")
        .float("intensity", "Intensity", 4.0, 0.0, 64.0)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let intensity = context.f("intensity").clamp(0.0, 64.0);
    let tile = context.tile;

    if intensity <= 1e-4 {
        return Ok(vec![input]);
    }

    let scale = context.w.min(context.h) as f32 / 256.0;
    let sigma = intensity * scale;

    let blurred = crate::ops::gaussian(&input, sigma, tile);
    Ok(vec![blurred])
}
