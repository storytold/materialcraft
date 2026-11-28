use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(crate) fn definition() -> NodeDefinition {
    node("min_max", "Min / Max", CATEGORY)
        .desc("Computes pixel-wise minimum or maximum between two input images.")
        .any_in("input_a", "Input A")
        .primary_input("input_a")
        .any_in("input_b", "Input B")
        .choice("mode", "Mode", &["Min (Darken)", "Max (Lighten)"], 0)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input_a = context.any_or(0, Kind::Gray, [0.0; 4]);
    let input_b = context.any_or(1, input_a.kind(), [0.0; 4]);
    let mode = context.i("mode");
    let tiling = context.tiling_axes();
    let kind = input_a.kind();

    Ok(vec![craft_texture::Image::generate_uv(context.w, context.h, kind, |u, v| {
        let a = input_a.sample_axes(u, v, tiling);
        let b = input_b.sample_axes(u, v, tiling);

        if mode == 1 {
            // Max
            [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2]), a[3].max(b[3])]
        } else {
            // Min
            [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2]), a[3].min(b[3])]
        }
    })])
}
