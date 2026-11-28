use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("waveform", "Waveform", CATEGORY)
        .desc("Mathematical wave generator with multiple harmonic waveforms: Sine, Sawtooth, Triangle, Square.")
        .choice("wave_type", "Wave Type", &["Sine", "Sawtooth", "Triangle", "Square"], 0)
        .int("frequency", "Frequency", 8, 1, 64)
        .angle("angle", "Angle", 0.0)
        .float("phase", "Phase Offset", 0.0, 0.0, 1.0)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_1() -> NodeDefinition {
    node("waveform_1", "Waveform 1", CATEGORY)
        .desc("Primary harmonic sine wave generator with angle and frequency controls.")
        .int("frequency", "Frequency", 8, 1, 64)
        .angle("angle", "Angle", 0.0)
        .float("phase", "Phase Offset", 0.0, 0.0, 1.0)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let wave_type = context.i("wave_type");
    let freq = context.i("frequency").clamp(1, 128) as f32;
    let angle = context.f("angle") * PI;
    let phase = context.f("phase");

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;

        let proj = x * cos_a - y * sin_a + 0.5;
        let t = (proj * freq + phase).rem_euclid(1.0);

        let val = match wave_type {
            // Sine
            0 => (t * PI * 2.0).sin() * 0.5 + 0.5,
            // Sawtooth
            1 => t,
            // Triangle
            2 => {
                let d = (t - 0.5).abs() * 2.0;
                1.0 - d
            }
            // Square
            _ => {
                if t < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
        };

        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}
