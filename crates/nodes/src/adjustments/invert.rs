use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(crate) fn definition() -> NodeDefinition {
    node("invert", "Invert", CATEGORY)
        .desc("Inverts color or grayscale values.")
        .any_in("input", "Input")
        .boolean("invert_alpha", "Invert alpha", false)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let invert_alpha = context.b("invert_alpha");
    let is_gray = input.is_gray();

    Ok(vec![input.map(input.kind(), |p| {
        if is_gray {
            let v = if p[0].is_finite() { (1.0 - p[0]).clamp(0.0, 1.0) } else { 0.0 };
            [v, 0.0, 0.0, 0.0]
        } else {
            let r = if p[0].is_finite() { (1.0 - p[0]).clamp(0.0, 1.0) } else { 0.0 };
            let g = if p[1].is_finite() { (1.0 - p[1]).clamp(0.0, 1.0) } else { 0.0 };
            let b = if p[2].is_finite() { (1.0 - p[2]).clamp(0.0, 1.0) } else { 0.0 };
            let a = if invert_alpha { if p[3].is_finite() { (1.0 - p[3]).clamp(0.0, 1.0) } else { 0.0 } } else { p[3] };
            [r, g, b, a]
        }
    })])
}
