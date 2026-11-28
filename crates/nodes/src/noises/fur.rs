use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("fur", "Fur", CATEGORY)
        .desc("Hair and fur fibers generator with comb direction, strand length, and curl variation.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .float("length", "Strand Length", 12.0, 1.0, 32.0)
        .angle("angle", "Comb Angle", 0.25)
        .float("curl", "Curl", 0.3, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_1() -> NodeDefinition {
    node("fur_1", "Fur 1", CATEGORY)
        .desc("Straight short coat fur fibers.")
        .float("scale", "Scale", 20.0, 1.0, 64.0)
        .float("length", "Strand Length", 8.0, 1.0, 32.0)
        .angle("angle", "Comb Angle", 0.25)
        .float("curl", "Curl", 0.1, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("fur_2", "Fur 2", CATEGORY)
        .desc("Medium wavy pelt with curved strands.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .float("length", "Strand Length", 14.0, 1.0, 32.0)
        .angle("angle", "Comb Angle", 0.25)
        .float("curl", "Curl", 0.4, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_3() -> NodeDefinition {
    node("fur_3", "Fur 3", CATEGORY)
        .desc("Long tangled shaggy fur fleece.")
        .float("scale", "Scale", 12.0, 1.0, 64.0)
        .float("length", "Strand Length", 20.0, 1.0, 32.0)
        .angle("angle", "Comb Angle", 0.25)
        .float("curl", "Curl", 0.7, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 128.0);
    let length = context.f("length").clamp(1.0, 64.0);
    let angle = context.f("angle") * PI;
    let curl = context.f("curl").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;

        let mut rx = (x * cos_a - y * sin_a) / length + 0.5;
        let mut ry = (x * sin_a + y * cos_a) + 0.5;

        if curl > 1e-4 {
            let curl_offset = (ry * scale * 0.5).sin() * (curl * 0.05);
            rx += curl_offset;
            ry += curl_offset;
        }

        let fbm = super::perlin::sample_perlin_fbm(rx, ry, scale, 4, 0.7, 2.2, seed);
        let val = (fbm * fbm * 1.5).clamp(0.0, 1.0);

        [val, 0.0, 0.0, 1.0]
    })])
}
