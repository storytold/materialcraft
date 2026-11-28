use super::ATOMIC;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use materialcraft_core::Value;

pub(super) fn definition() -> NodeDefinition {
    node("emboss", "Emboss", ATOMIC)
        .desc("Lights the input with the relief of a height map.")
        .atomic()
        .any_in("input", "Input")
        .gray_in("height", "Intensity input")
        .float("intensity", "Intensity", 5.0, 0.0, 32.0)
        .angle("light_angle", "Light angle", 0.0)
        .color("highlight", "Highlight color", [1.0, 1.0, 1.0, 1.0])
        .color("shadow", "Shadow color", [0.0; 4])
        .any_out()
        .cook(emboss)
}

fn emboss(context: &CookContext) -> CookResult {
    let input = context.any_or(0, Kind::Gray, [0.0; 4]);
    let height_map = context.gray_or(1, 0.0);
    let pixel_size = match context.vars.get("$pixelsize") {
        Some(Value::Float2(size)) => *size,
        _ => [1.0; 2],
    };
    let (sin, cos) = (context.f("light_angle") * std::f32::consts::TAU).sin_cos();
    let intensity = context.f("intensity") * (input.width().min(input.height()) as f32 / 256.0);
    let light_x = cos * pixel_size[0] * intensity;
    let light_y = sin * pixel_size[1] * intensity;
    let tint = |id: &str| {
        let [r, g, b, _] = context.v4(id);
        if input.is_gray() { [(r + g + b) / 3.0; 3] } else { [r, g, b] }
    };
    let highlight = tint("highlight");
    let shadow = tint("shadow").map(|v| 1.0 - v);
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate(context.w, context.h, input.kind(), |x, y| {
        let (x, y) = (i64::from(x), i64::from(y));
        let center = height_map.get_gray(x, y, false);
        let dx = center - height_map.get_axes(x - 1, y, tiling)[0];
        let dy = center - height_map.get_axes(x, y - 1, tiling)[0];
        let light = light_y.mul_add(dy, light_x * dx);
        let mut pixel = input.get(x, y, false);
        let weights = if light >= 0.0 { highlight } else { shadow };
        for (value, weight) in pixel.iter_mut().zip(weights) {
            *value = light.mul_add(weight, *value);
        }
        pixel
    })])
}
