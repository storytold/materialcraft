use super::CATEGORY;
use super::ccl::analyze_mask;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("flood_fill", "Flood Fill", CATEGORY)
        .desc("Segments connected islands from a mask and encodes centroid and bbox data.")
        .gray_in("input", "Input")
        .float("threshold", "Threshold", 0.5, 0.0, 1.0)
        .boolean("tiling", "Tiling", true)
        .color_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    let threshold = context.f("threshold").clamp(0.0, 1.0);
    let tiling = context.b("tiling");

    let (labels, islands) = analyze_mask(&input, threshold, tiling);
    let w = context.w as usize;

    Ok(vec![Image::generate(context.w, context.h, Kind::Color, |x, y| {
        let idx = (y as usize) * w + (x as usize);
        let label = labels.get(idx).copied().unwrap_or(0);
        if label == 0 {
            [0.0, 0.0, 0.0, 0.0]
        } else if let Some(info) = islands.get((label - 1) as usize) {
            [info.centroid_u, info.centroid_v, info.bbox_w, info.bbox_h]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    })])
}
