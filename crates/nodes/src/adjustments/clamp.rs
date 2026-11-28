use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(crate) fn definition() -> NodeDefinition {
    node("clamp", "Clamp", CATEGORY)
        .desc("Clamps values to a specified minimum and maximum range.")
        .any_in("input", "Input")
        .float("min", "Minimum", 0.0, 0.0, 1.0)
        .float("max", "Maximum", 1.0, 0.0, 1.0)
        .boolean("clamp_alpha", "Clamp alpha", false)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let min_val = context.f("min");
    let max_val = context.f("max");
    let clamp_alpha = context.b("clamp_alpha");
    let is_gray = input.is_gray();

    let (low, high) = if min_val <= max_val { (min_val, max_val) } else { (max_val, min_val) };
    let low = if low.is_finite() { low } else { 0.0 };
    let high = if high.is_finite() { high } else { 1.0 };

    Ok(vec![input.map(input.kind(), |p| {
        let clamp_val = |v: f32| if v.is_finite() { v.clamp(low, high) } else { low };
        if is_gray {
            [clamp_val(p[0]), 0.0, 0.0, 0.0]
        } else {
            let a = if clamp_alpha { clamp_val(p[3]) } else { p[3] };
            [clamp_val(p[0]), clamp_val(p[1]), clamp_val(p[2]), a]
        }
    })])
}
