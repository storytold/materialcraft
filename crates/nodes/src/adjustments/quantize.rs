use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};

pub(crate) fn definition() -> NodeDefinition {
    node("quantize", "Quantize", CATEGORY)
        .desc("Reduces the number of tonal levels in the image to create banding, posterization, or flat plateaus.")
        .any_in("input", "Input")
        .primary_input("input")
        .int("levels", "Levels", 8, 2, 64)
        .float("softness", "Softness", 0.0, 0.0, 1.0)
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let input = context.any_or(0, craft_texture::Kind::Gray, [0.0; 4]);
    let levels = context.i("levels").clamp(2, 64) as f32;
    let softness = context.f("softness").clamp(0.0, 1.0);
    let channels = input.kind().channels();

    let inv_steps = 1.0 / (levels - 1.0);

    Ok(vec![input.map(input.kind(), |pixel| {
        let mut out = [0.0f32; 4];
        for c in 0..channels.min(3) {
            let v = pixel[c].clamp(0.0, 1.0);
            let raw_step = v * (levels - 1.0);
            let base = raw_step.floor();
            let frac = raw_step - base;

            let step_val = if softness > 1e-4 {
                let half_soft = softness * 0.5;
                let s_val = ((frac - (0.5 - half_soft)) / softness.max(1e-4)).clamp(0.0, 1.0);
                let smooth_frac = s_val * s_val * (3.0 - 2.0 * s_val);
                base + smooth_frac
            } else {
                base.round()
            };

            out[c] = (step_val * inv_steps).clamp(0.0, 1.0);
        }
        if channels == 4 {
            out[3] = pixel[3];
        }
        out
    })])
}
