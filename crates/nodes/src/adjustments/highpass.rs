use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, atomic::box_filter_axis, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("highpass", "Highpass", CATEGORY)
        .desc("Extracts high-frequency details and balances overall luminance differences across the surface.")
        .any_in("input", "Input")
        .primary_input("input")
        .float("radius", "Radius", 16.0, 0.5, 64.0)
        .float("contrast", "Contrast", 1.0, 0.0, 4.0)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let radius = context.f("radius").clamp(0.5, 64.0);
    let contrast = context.f("contrast").clamp(0.0, 4.0);
    let tile = context.tiling_axes();
    let channels = input.kind().channels();

    // Low-frequency blurred version
    let blurred = box_filter_axis(&box_filter_axis(&input, radius, true, tile[0]), radius, false, tile[1]);

    let src_data = input.data();
    let blur_data = blurred.data();
    let num_pixels = (context.w * context.h) as usize;

    let mut out_data = vec![0.0f32; num_pixels * channels];

    for i in 0..num_pixels {
        let base_idx = i * channels;
        for c in 0..channels.min(3) {
            let s = src_data.get(base_idx + c).copied().unwrap_or(0.0);
            let b = blur_data.get(base_idx + c).copied().unwrap_or(0.0);
            let hp = (0.5 + (s - b) * contrast).clamp(0.0, 1.0);
            out_data[base_idx + c] = hp;
        }
        if channels == 4 {
            out_data[base_idx + 3] = src_data.get(base_idx + 3).copied().unwrap_or(1.0);
        }
    }

    Image::from_data(context.w, context.h, input.kind(), out_data, &craft_texture::Limits::default()).map(|img| vec![img]).map_err(|e| e.to_string())
}
