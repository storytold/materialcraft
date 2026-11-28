use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(crate) fn definition() -> NodeDefinition {
    node("histogram_range", "Histogram Range", CATEGORY)
        .desc("Remaps the histogram values to a new minimum and maximum range.")
        .any_in("input", "Input")
        .float("range_min", "Range minimum", 0.0, 0.0, 1.0)
        .float("range_max", "Range maximum", 1.0, 0.0, 1.0)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let range_min = context.f("range_min");
    let range_max = context.f("range_max");
    let is_gray = input.is_gray();

    let r_min = if range_min.is_finite() { range_min.clamp(0.0, 1.0) } else { 0.0 };
    let r_max = if range_max.is_finite() { range_max.clamp(0.0, 1.0) } else { 1.0 };
    let span = r_max - r_min;

    let remap = move |v: f32| -> f32 {
        if !v.is_finite() {
            return r_min;
        }
        (r_min + v.clamp(0.0, 1.0) * span).clamp(0.0, 1.0)
    };

    Ok(vec![input.map(input.kind(), |p| if is_gray { [remap(p[0]), 0.0, 0.0, 0.0] } else { [remap(p[0]), remap(p[1]), remap(p[2]), p[3]] })])
}
