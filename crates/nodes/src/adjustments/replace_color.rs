use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(crate) fn definition() -> NodeDefinition {
    node("replace_color", "Replace Color", CATEGORY)
        .desc("Replaces a targeted source color with a replacement color within a specified tolerance and softness.")
        .any_in("input", "Input")
        .primary_input("input")
        .gray_in("mask", "Mask")
        .color("source_color", "Source Color", [1.0, 0.0, 0.0, 1.0])
        .color("target_color", "Target Color", [0.0, 1.0, 0.0, 1.0])
        .float("tolerance", "Tolerance", 0.2, 0.0, 1.0)
        .float("softness", "Softness", 0.1, 0.0, 1.0)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let user_mask = context.sized(1);
    let src_col = context.v4("source_color");
    let tgt_col = context.v4("target_color");
    let tolerance = context.f("tolerance").clamp(0.0, 1.0);
    let softness = context.f("softness").clamp(0.001, 1.0);
    let tiling = context.tiling_axes();
    let kind = input.kind();

    Ok(vec![craft_texture::Image::generate_uv(context.w, context.h, kind, |u, v| {
        let p = input.sample_axes(u, v, tiling);
        let m_factor = user_mask.as_ref().map_or(1.0, |m| m.sample_axes(u, v, tiling)[0].clamp(0.0, 1.0));

        let dr = p[0] - src_col[0];
        let dg = p[1] - src_col[1];
        let db = p[2] - src_col[2];
        let dist = (dr * dr + dg * dg + db * db).sqrt() / 1.732_050_8; // normalized by sqrt(3)

        let factor = if dist <= tolerance {
            1.0
        } else if dist < tolerance + softness {
            let t = (dist - tolerance) / softness;
            (1.0 - t).clamp(0.0, 1.0)
        } else {
            0.0
        } * m_factor;

        let r = p[0] * (1.0 - factor) + tgt_col[0] * factor;
        let g = p[1] * (1.0 - factor) + tgt_col[1] * factor;
        let b = p[2] * (1.0 - factor) + tgt_col[2] * factor;
        let a = p[3] * (1.0 - factor) + tgt_col[3] * factor;

        [r, g, b, a]
    })])
}
