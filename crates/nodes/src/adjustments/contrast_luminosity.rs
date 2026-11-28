use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(crate) fn definition() -> NodeDefinition {
    node("contrast_luminosity", "Contrast / Luminosity", CATEGORY)
        .desc("Adjusts the contrast and brightness of an image.")
        .any_in("input", "Input")
        .float("contrast", "Contrast", 0.0, -1.0, 1.0)
        .float("luminosity", "Luminosity", 0.0, -1.0, 1.0)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let contrast = context.f("contrast");
    let luminosity = context.f("luminosity");
    let is_gray = input.is_gray();

    let factor = if !contrast.is_finite() {
        1.0
    } else if contrast < 0.0 {
        (1.0 + contrast).max(0.0)
    } else {
        1.0 / (1.0 - contrast.min(0.999))
    };
    let lum = if luminosity.is_finite() { luminosity } else { 0.0 };

    let adjust = move |v: f32| -> f32 {
        if !v.is_finite() {
            return 0.0;
        }
        let adjusted = (v - 0.5) * factor + 0.5 + lum;
        adjusted.clamp(0.0, 1.0)
    };

    Ok(vec![input.map(input.kind(), |p| if is_gray { [adjust(p[0]), 0.0, 0.0, 0.0] } else { [adjust(p[0]), adjust(p[1]), adjust(p[2]), p[3]] })])
}
