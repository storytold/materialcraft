use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("fluid", "Fluid", CATEGORY)
        .desc("Simulates fluid advection and vortex swirl using curl-distorted multi-frequency noise.")
        .float("scale", "Scale", 4.0, 1.0, 32.0)
        .float("swirl", "Swirl Intensity", 2.0, 0.0, 8.0)
        .int("octaves", "Octaves", 4, 1, 8)
        .int("iterations", "Advection Iterations", 3, 1, 6)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let swirl = context.f("swirl").clamp(0.0, 16.0);
    let octaves = context.i("octaves").clamp(1, 8) as usize;
    let iterations = context.i("iterations").clamp(1, 6) as usize;
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let mut cur_u = u;
        let mut cur_v = v;
        let delta = 0.005f32;

        for it in 0..iterations {
            let it_seed = seed.wrapping_add((it as u32) * 513);
            let n_r = super::perlin::sample_perlin_fbm(cur_u + delta, cur_v, scale, 2, 0.5, 2.0, it_seed);
            let n_l = super::perlin::sample_perlin_fbm(cur_u - delta, cur_v, scale, 2, 0.5, 2.0, it_seed);
            let n_u = super::perlin::sample_perlin_fbm(cur_u, cur_v + delta, scale, 2, 0.5, 2.0, it_seed);
            let n_d = super::perlin::sample_perlin_fbm(cur_u, cur_v - delta, scale, 2, 0.5, 2.0, it_seed);

            let grad_x = (n_r - n_l) / (2.0 * delta);
            let grad_y = (n_u - n_d) / (2.0 * delta);

            let step = (swirl * 0.05) / (it as f32 + 1.0);
            cur_u -= grad_y * step;
            cur_v += grad_x * step;
        }

        let val = super::perlin::sample_perlin_fbm(cur_u, cur_v, scale, octaves, 0.5, 2.0, seed.wrapping_add(9999));
        [val, 0.0, 0.0, 1.0]
    })])
}
