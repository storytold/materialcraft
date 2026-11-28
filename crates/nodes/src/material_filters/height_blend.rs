use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("height_blend", "Height Blend", CATEGORY)
        .desc("Blends two height maps based on their elevation values and an offset, outputting the blended height and transition mask.")
        .gray_in("height_top", "Height Top")
        .primary_input("height_top")
        .gray_in("height_bottom", "Height Bottom")
        .gray_in("mask", "Mask")
        .float("height_offset", "Height Offset", 0.5, 0.0, 1.0)
        .float("contrast", "Contrast", 0.0, 0.0, 1.0)
        .gray_out() // output 0: Height
        .named_gray_out("mask", "Mask") // output 1: Transition Mask
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let top = context.gray_or(0, 0.5);
    let bottom = context.gray_or(1, 0.0);
    let mask_input = context.sized(2);

    let offset = context.f("height_offset").clamp(0.0, 1.0);
    let contrast = context.f("contrast").clamp(0.0, 1.0);
    let tiling = context.tiling_axes();

    let slope = 1.0 / (1.0 - contrast * 0.99);

    let mut out_height_data = vec![0.0f32; (context.w * context.h) as usize];
    let mut out_mask_data = vec![0.0f32; (context.w * context.h) as usize];

    let top_data = top.data();
    let bottom_data = bottom.data();

    for y in 0..context.h {
        let v = (y as f32 + 0.5) / context.h as f32;
        for x in 0..context.w {
            let u = (x as f32 + 0.5) / context.w as f32;
            let idx = (y * context.w + x) as usize;

            let ht = top_data.get(idx).copied().unwrap_or(0.5);
            let hb = bottom_data.get(idx).copied().unwrap_or(0.0);
            let user_mask = if let Some(m) = &mask_input { m.sample_axes(u, v, tiling)[0].clamp(0.0, 1.0) } else { 1.0 };

            // Height difference offset: positive when top is higher than bottom
            let diff = (ht + (offset * 2.0 - 1.0)) - hb;
            let raw_t = diff * 0.5 + 0.5;
            let t = ((raw_t - 0.5) * slope + 0.5).clamp(0.0, 1.0) * user_mask;

            let blended_h = (hb * (1.0 - t) + ht * t).clamp(0.0, 1.0);

            if let Some(target) = out_height_data.get_mut(idx) {
                *target = blended_h;
            }
            if let Some(target) = out_mask_data.get_mut(idx) {
                *target = t;
            }
        }
    }

    let limits = craft_texture::Limits::default();
    let out_height = Image::from_data(context.w, context.h, Kind::Gray, out_height_data, &limits).map_err(|e| e.to_string())?;
    let out_mask = Image::from_data(context.w, context.h, Kind::Gray, out_mask_data, &limits).map_err(|e| e.to_string())?;

    Ok(vec![out_height, out_mask])
}
