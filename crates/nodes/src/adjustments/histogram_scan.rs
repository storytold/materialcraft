use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("histogram_scan", "Histogram Scan", CATEGORY)
        .desc("Selects and sharpens a slice of a height or grayscale map by position and contrast, with optional position map.")
        .gray_in("input", "Input")
        .primary_input("input")
        .gray_in("position_map", "Position Map")
        .float("position", "Position", 0.5, 0.0, 1.0)
        .float("contrast", "Contrast", 0.5, 0.0, 1.0)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    let pos_map = context.sized(1);
    let tile = context.tile;

    let position = context.f("position");
    let contrast = context.f("contrast");

    let pos = if position.is_finite() { position.clamp(0.0, 1.0) } else { 0.5 };
    let con = if contrast.is_finite() { contrast.clamp(0.0, 1.0) } else { 0.5 };

    let width = (1.0 - con * 0.9999).max(1e-5);
    let inv_width = 1.0 / width;

    Ok(vec![Image::generate(context.w, context.h, Kind::Gray, |x, y| {
        let (x_i, y_i) = (x as i64, y as i64);
        let v = input.get_gray(x_i, y_i, tile);
        if !v.is_finite() {
            return [0.0, 0.0, 0.0, 0.0];
        }

        let effective_pos = pos_map.as_ref().map_or(pos, |pm| {
            let p_val = pm.get_gray(x_i, y_i, tile);
            (pos * p_val).clamp(0.0, 1.0)
        });

        let out = ((v - effective_pos) * inv_width + 0.5).clamp(0.0, 1.0);
        [out, 0.0, 0.0, 0.0]
    })])
}
