use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn grayscale_definition() -> NodeDefinition {
    node("vector_warp_grayscale", "Vector Warp Grayscale", CATEGORY)
        .desc("Warps a grayscale input using a 2D vector map (R=X displacement, G=Y displacement).")
        .gray_in("input", "Input")
        .primary_input("input")
        .color_in("vector_map", "Vector Map")
        .float("intensity", "Intensity", 1.0, 0.0, 10.0)
        .gray_out()
        .cook(cook_grayscale)
}

pub(crate) fn color_definition() -> NodeDefinition {
    node("vector_warp_color", "Vector Warp Color", CATEGORY)
        .desc("Warps a color input using a 2D vector map (R=X displacement, G=Y displacement).")
        .color_in("input", "Input")
        .primary_input("input")
        .color_in("vector_map", "Vector Map")
        .float("intensity", "Intensity", 1.0, 0.0, 10.0)
        .color_out()
        .cook(cook_color)
}

fn cook_grayscale(context: &CookContext<'_>) -> CookResult {
    let input = context.gray_or(0, 0.0);
    cook_impl(context, &input, Kind::Gray)
}

fn cook_color(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, Kind::Color, [0.0; 4]);
    cook_impl(context, &input, Kind::Color)
}

fn cook_impl(context: &CookContext<'_>, input: &Image, kind: Kind) -> CookResult {
    let vector_map = context.sized(1);
    let intensity = context.f("intensity").clamp(0.0, 10.0);
    let tiling = context.tiling_axes();

    Ok(vec![Image::generate_uv(context.w, context.h, kind, |u, v| {
        let (du, dv) = if let Some(vm) = &vector_map {
            let sample = vm.sample_axes(u, v, tiling);
            ((sample[0] * 2.0 - 1.0) * intensity * 0.1, (sample[1] * 2.0 - 1.0) * intensity * 0.1)
        } else {
            (0.0, 0.0)
        };

        input.sample_axes(u + du, v + dv, tiling)
    })])
}
