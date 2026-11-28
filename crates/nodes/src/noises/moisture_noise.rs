use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("moisture_noise", "Moisture Noise", CATEGORY)
        .desc("Moisture, humidity, and condensation noise generator simulating liquid spots and wet blotches.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("humidity", "Humidity Level", 0.5, 0.0, 1.0)
        .float("contrast", "Contrast", 2.0, 0.5, 5.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 128.0);
    let humidity = context.f("humidity").clamp(0.0, 1.0);
    let contrast = context.f("contrast").clamp(0.1, 10.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        // Multi-layered blotches with porous droplet clusters
        let base = super::perlin::sample_perlin_fbm(u, v, scale, 3, 0.5, 2.0, seed);
        let micro = super::perlin::sample_perlin_fbm(u, v, scale * 4.0, 4, 0.6, 2.2, seed.wrapping_add(888));

        // Porous clustering
        let wet_factor = base * 0.6 + micro * 0.4;
        let threshold = 1.0 - humidity;
        let t = ((wet_factor - threshold) * contrast + 0.5).clamp(0.0, 1.0);
        let val = t * t * (3.0 - 2.0 * t);

        [val, 0.0, 0.0, 1.0]
    })])
}
