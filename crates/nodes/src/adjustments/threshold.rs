use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(crate) fn definition() -> NodeDefinition {
    node("threshold", "Threshold", CATEGORY)
        .desc("Binarizes values around a threshold value with optional transition softness.")
        .any_in("input", "Input")
        .float("threshold", "Threshold", 0.5, 0.0, 1.0)
        .float("softness", "Softness", 0.0, 0.0, 1.0)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let threshold = context.f("threshold");
    let softness = context.f("softness");
    let is_gray = input.is_gray();

    let th = if threshold.is_finite() { threshold.clamp(0.0, 1.0) } else { 0.5 };
    let soft = if softness.is_finite() { softness.clamp(0.0, 1.0) } else { 0.0 };

    let apply = move |v: f32| -> f32 {
        if !v.is_finite() {
            return 0.0;
        }
        if soft <= 1e-5 {
            if v >= th { 1.0 } else { 0.0 }
        } else {
            let half = soft * 0.5;
            let low = th - half;
            let t = ((v - low) / soft).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        }
    };

    Ok(vec![input.map(input.kind(), |p| if is_gray { [apply(p[0]), 0.0, 0.0, 0.0] } else { [apply(p[0]), apply(p[1]), apply(p[2]), p[3]] })])
}
