use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("plasma", "Plasma", CATEGORY)
        .desc("High-energy plasma noise featuring sinusoidal multi-frequency harmonics.")
        .float("scale", "Scale", 8.0, 1.0, 32.0)
        .float("phase", "Phase", 0.0, 0.0, 10.0)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let phase = context.f("phase");

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u * scale;
        let y = v * scale;

        let v1 = (x * PI * 0.5 + phase).sin();
        let v2 = (y * PI * 0.5 + phase * 0.7).sin();
        let v3 = ((x + y) * PI * 0.25 + phase * 1.3).sin();
        let cx = x - scale * 0.5;
        let cy = y - scale * 0.5;
        let dist = (cx * cx + cy * cy).sqrt();
        let v4 = (dist * PI * 0.5 - phase * 2.0).sin();

        let sum = (v1 + v2 + v3 + v4) * 0.25;
        let val = (sum * 0.5 + 0.5).clamp(0.0, 1.0);

        [val, 0.0, 0.0, 1.0]
    })])
}
