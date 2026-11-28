use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("gaussian_noise", "Gaussian Noise", CATEGORY)
        .desc("Random Gaussian distribution noise with customizable mean and standard deviation.")
        .float("mean", "Mean", 0.5, 0.0, 1.0)
        .float("deviation", "Standard Deviation", 0.15, 0.01, 0.5)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn spots_1_definition() -> NodeDefinition {
    node("gaussian_spots_1", "Gaussian Spots 1", CATEGORY)
        .desc("Sparse soft circular Gaussian droplets and spots.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .float("spot_size", "Spot Size", 0.3, 0.05, 0.8)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_spots(ctx, false))
}

pub(crate) fn spots_2_definition() -> NodeDefinition {
    node("gaussian_spots_2", "Gaussian Spots 2", CATEGORY)
        .desc("Dense clustered Gaussian splatter spots.")
        .float("scale", "Scale", 24.0, 1.0, 64.0)
        .float("spot_size", "Spot Size", 0.4, 0.05, 0.8)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_spots(ctx, true))
}

fn cook_spots(context: &CookContext<'_>, dense: bool) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 128.0);
    let size = context.f("spot_size").clamp(0.01, 1.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u * scale;
        let y = v * scale;
        let ix = x.floor() as i64;
        let iy = y.floor() as i64;

        let mut sum = 0.0f32;

        for dy in -1..=1 {
            for dx in -1..=1 {
                let cx = ix + dx;
                let cy = iy + dy;
                let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ seed;
                let h1 = crate::random::hash(h0);
                let h2 = crate::random::hash(h1);
                let h3 = crate::random::hash(h2);

                if !dense && (h3 % 100) > 60 {
                    continue;
                }

                let fx = cx as f32 + (h1 as f32 / 4_294_967_295.0);
                let fy = cy as f32 + (h2 as f32 / 4_294_967_295.0);

                let d_sq = (x - fx) * (x - fx) + (y - fy) * (y - fy);
                let spot = (-d_sq / (2.0 * size * size)).exp();
                sum += spot;
            }
        }

        [sum.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let mean = context.f("mean").clamp(0.0, 1.0);
    let dev = context.f("deviation").clamp(0.001, 1.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let px = (u * w as f32).floor() as u32;
        let py = (v * h as f32).floor() as u32;

        let h0 = px.wrapping_mul(0x85eb_ca6b) ^ py.wrapping_mul(0xc2b2_ae35) ^ seed;
        let h1 = crate::random::hash(h0);
        let h2 = crate::random::hash(h1);

        // Box-Muller transform
        let u1 = ((h1 as f32 / 4_294_967_295.0) + 1e-7).clamp(1e-7, 1.0);
        let u2 = h2 as f32 / 4_294_967_295.0;

        let z0 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos();
        let val = (mean + z0 * dev).clamp(0.0, 1.0);

        [val, 0.0, 0.0, 1.0]
    })])
}
