use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("checker", "Checker", CATEGORY)
        .desc("Generates an alternating checkerboard grid pattern.")
        .generator()
        .int("tiling_x", "Tiling X", 2, 1, 64)
        .int("tiling_y", "Tiling Y", 2, 1, 64)
        .boolean("invert", "Invert", false)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_1() -> NodeDefinition {
    node("checker_1", "Checker 1", CATEGORY)
        .desc("Standard alternating 2-color checkerboard tile pattern.")
        .generator()
        .int("tiling_x", "Tiling X", 2, 1, 64)
        .int("tiling_y", "Tiling Y", 2, 1, 64)
        .boolean("invert", "Invert", false)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let tx = context.i("tiling_x").max(1);
    let ty = context.i("tiling_y").max(1);
    let invert = context.b("invert");

    let (w_f, h_f) = (context.w as f32, context.h as f32);

    Ok(vec![Image::generate(context.w, context.h, Kind::Gray, |x, y| {
        let u = (x as f32) / w_f;
        let v = (y as f32) / h_f;

        let cx = (u * tx as f32).floor() as i32;
        let cy = (v * ty as f32).floor() as i32;

        let white = (cx + cy).rem_euclid(2) == 0;
        let is_white = if invert { !white } else { white };
        let val = if is_white { 1.0 } else { 0.0 };

        [val, 0.0, 0.0, 0.0]
    })])
}
