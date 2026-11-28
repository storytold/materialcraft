use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("liquid", "Liquid", CATEGORY)
        .desc("Simulates viscous organic liquid droplets, bubbles, and metaball surface tension.")
        .float("scale", "Scale", 8.0, 1.0, 32.0)
        .float("viscosity", "Viscosity", 0.5, 0.0, 1.0)
        .float("contrast", "Contrast", 2.0, 0.5, 6.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let viscosity = context.f("viscosity").clamp(0.0, 1.0);
    let contrast = context.f("contrast").clamp(0.1, 8.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u * scale;
        let y = v * scale;
        let ix = x.floor() as i64;
        let iy = y.floor() as i64;

        let mut metaball_sum = 0.0f32;

        for dy in -1..=1 {
            for dx in -1..=1 {
                let cx = ix + dx;
                let cy = iy + dy;
                let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ seed;
                let h1 = crate::random::hash(h0);
                let h2 = crate::random::hash(h1);
                let h3 = crate::random::hash(h2);

                let fx = cx as f32 + (h1 as f32 / 4_294_967_295.0);
                let fy = cy as f32 + (h2 as f32 / 4_294_967_295.0);
                let radius = 0.4 + (h3 as f32 / 4_294_967_295.0) * 0.4;

                let d_sq = (x - fx) * (x - fx) + (y - fy) * (y - fy);
                let r_sq = radius * radius;
                if d_sq < r_sq {
                    let falloff = 1.0 - (d_sq / r_sq);
                    metaball_sum += falloff * falloff;
                }
            }
        }

        let threshold = 0.5 - (viscosity * 0.2);
        let t = ((metaball_sum - threshold) * contrast + 0.5).clamp(0.0, 1.0);
        let smooth = t * t * (3.0 - 2.0 * t);
        [smooth, 0.0, 0.0, 1.0]
    })])
}
