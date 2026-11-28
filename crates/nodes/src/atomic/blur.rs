use super::{ATOMIC, box_filter, input_resize};
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Image;
use materialcraft_core::Value;

pub(super) fn definition() -> NodeDefinition {
    node("blur", "Blur", ATOMIC)
        .desc("Box blur with fractional pixel coverage.")
        .atomic()
        .any_in("input", "Input")
        .float("intensity", "Intensity", 10.0, 0.0, 32.0)
        .any_out()
        .cook(blur)
}

fn blur(context: &CookContext) -> CookResult {
    let intensity = context.f("intensity");
    let (input, source_format) = input_resize::blur_input(context, intensity);
    const BASE_RESOLUTION: f32 = 256.0;
    let radius = intensity.max(0.0) * context.w.min(context.h) as f32 / BASE_RESOLUTION;
    let pixel_size = match context.vars.get("$pixelsize") {
        Some(Value::Float2(size)) => *size,
        _ => [1.0; 2],
    };
    let [reach_x, reach_y] = pixel_size.map(|scale| {
        let reach = radius * scale - 0.5;
        if reach.is_finite() { reach.max(0.0) } else { 0.0 }
    });
    let tile = context.tiling_axes();
    let filter = |image: Image, reach, horizontal, source| {
        if reach <= 0.0 {
            image
        } else if reach <= 2.0 {
            let step = if horizontal { [1.0, 0.0] } else { [0.0, -1.0] };
            box_filter::short_line(&image, reach, step, tile, source)
        } else {
            box_filter::axis(&image, reach, horizontal, if horizontal { tile[0] } else { tile[1] }, source, context.format)
        }
    };
    let horizontal = filter(input, reach_x, true, source_format);
    if reach_y == 0.0 {
        return Ok(vec![horizontal]);
    }
    let stored = crate::output_precision::apply(horizontal, context.format);
    Ok(vec![filter(stored, reach_y, false, context.format)])
}
