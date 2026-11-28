use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("flood_fill_to_bbox_size", "Flood Fill to BBox Size", CATEGORY)
        .desc("Outputs bounding box dimensions, aspect ratio, or area of flood-filled islands.")
        .color_in("input", "Flood Fill Data")
        .choice("mode", "Mode", &["Area", "Width", "Height", "Aspect Ratio"], 0)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    let mode = context.i("mode");
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let sample = input.sample_axes(u, v, tiling);
        let cu = sample[0];
        let cv = sample[1];
        let bw = sample[2];
        let bh = sample[3];

        if bw == 0.0 && bh == 0.0 && cu == 0.0 && cv == 0.0 {
            return [0.0, 0.0, 0.0, 0.0];
        }

        let val = match mode {
            0 => bw * bh,
            1 => bw,
            2 => bh,
            3 => bw / bh.max(1e-4) * 0.5,
            _ => bw * bh,
        };

        [val.clamp(0.0, 1.0), 0.0, 0.0, 0.0]
    })])
}
