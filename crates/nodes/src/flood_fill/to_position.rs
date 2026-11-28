use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("flood_fill_to_position", "Flood Fill to Position", CATEGORY)
        .desc("Outputs the normalized centroid coordinates (UV position) of each flood-filled island.")
        .color_in("input", "Flood Fill Data")
        .primary_input("input")
        .choice("mode", "Mode", &["Vector (RG)", "Position X", "Position Y"], 0)
        .color_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let mode = context.i("mode");
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Color, |u, v| {
        let sample = input.sample_axes(u, v, tiling);
        let cu = sample[0];
        let cv = sample[1];
        let bw = sample[2];
        let bh = sample[3];

        if bw == 0.0 && bh == 0.0 && cu == 0.0 && cv == 0.0 {
            return [0.0, 0.0, 0.0, 0.0];
        }

        match mode {
            1 => [cu, cu, cu, 1.0],
            2 => [cv, cv, cv, 1.0],
            _ => [cu, cv, 0.0, 1.0],
        }
    })])
}
