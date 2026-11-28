use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("gradient_axial", "Gradient Axial", CATEGORY)
        .desc("Generates a symmetrical axial gradient extending outwards from a center axis.")
        .generator()
        .angle("angle", "Angle", 0.0)
        .int("tiling", "Tiling", 1, 1, 64)
        .boolean("reverse", "Reverse", false)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let angle = context.f("angle") * PI;
    let tiling = context.i("tiling").max(1) as f32;
    let reverse = context.b("reverse");

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    let (w_f, h_f) = (context.w as f32, context.h as f32);

    Ok(vec![Image::generate(context.w, context.h, Kind::Gray, |x, y| {
        let u = (x as f32 + 0.5) / w_f;
        let v = (y as f32 + 0.5) / h_f;

        let dx = u - 0.5;
        let dy = v - 0.5;

        let proj = dx * cos_a - dy * sin_a;
        let dist = proj.abs() * 2.0;
        let val = (dist * tiling).rem_euclid(1.0);

        let val = if reverse { 1.0 - val } else { val };
        [val.clamp(0.0, 1.0), 0.0, 0.0, 0.0]
    })])
}
