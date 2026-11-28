use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("stripes", "Stripes", CATEGORY)
        .desc("Generates alternating stripes with angle, duty cycle, shift, and transition softness.")
        .generator()
        .int("tiling", "Tiling", 4, 1, 64)
        .angle("angle", "Angle", 0.0)
        .float("width", "Stripe width", 0.5, 0.0, 1.0)
        .float("shift", "Shift", 0.0, 0.0, 1.0)
        .float("softness", "Softness", 0.0, 0.0, 1.0)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let tiling = context.i("tiling").max(1) as f32;
    let angle = context.f("angle") * PI;
    let width = context.f("width").clamp(0.0, 1.0);
    let shift = context.f("shift");
    let softness = context.f("softness").clamp(0.0, 1.0);

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    let (w_f, h_f) = (context.w as f32, context.h as f32);

    Ok(vec![Image::generate(context.w, context.h, Kind::Gray, |x, y| {
        let u = (x as f32 + 0.5) / w_f;
        let v = (y as f32 + 0.5) / h_f;

        let dx = u - 0.5;
        let dy = v - 0.5;

        let proj = dx * cos_a - dy * sin_a + 0.5 + shift;
        let phase = (proj * tiling).rem_euclid(1.0);

        let val = if width <= 1e-6 {
            0.0
        } else if width >= 1.0 - 1e-6 {
            1.0
        } else if softness <= 1e-4 {
            if phase < width { 1.0 } else { 0.0 }
        } else {
            let half = softness * 0.5;
            // Rising edge at 0: wrap around 1.0 to ensure smoothness on both sides of 0
            let phase_wrap = if phase > 1.0 - half { phase - 1.0 } else { phase };
            let rise = ((phase_wrap + half) / softness).clamp(0.0, 1.0);
            let fall = 1.0 - ((phase - width + half) / softness).clamp(0.0, 1.0);
            let combined = rise.min(fall);
            combined * combined * (3.0 - 2.0 * combined)
        };

        [val.clamp(0.0, 1.0), 0.0, 0.0, 0.0]
    })])
}
