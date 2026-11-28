use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("cracks", "Cracks", CATEGORY)
        .desc("Procedural branching stone and concrete cracks based on second-order cellular distances and organic noise warp.")
        .float("tiling", "Tiling", 4.0, 1.0, 32.0)
        .float("crack_width", "Crack Width", 0.02, 0.001, 0.2)
        .float("crack_depth", "Crack Depth", 0.8, 0.1, 1.0)
        .float("roughness", "Roughness", 0.3, 0.0, 1.0)
        .int("seed", "Random Seed", 0, 0, 10000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let tiling = context.f("tiling").clamp(1.0, 32.0);
    let crack_width = context.f("crack_width").clamp(0.001, 0.2);
    let crack_depth = context.f("crack_depth").clamp(0.1, 1.0);
    let roughness = context.f("roughness").clamp(0.0, 1.0);
    let user_seed = context.i("seed").max(0) as u32;

    let n = tiling.round().max(1.0) as usize;
    let inv_n = 1.0 / n as f32;

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        // High frequency micro warp for roughness
        let warp_val = if roughness > 1e-4 {
            let wx = (u * 32.0 * tiling).sin() * 0.5 + (v * 32.0 * tiling).cos() * 0.5;
            wx * roughness * 0.02 * inv_n
        } else {
            0.0
        };

        let su = u + warp_val;
        let sv = v + warp_val;

        let gx = (su * n as f32).floor() as i64;
        let gy = (sv * n as f32).floor() as i64;

        let mut f1 = f32::INFINITY;
        let mut f2 = f32::INFINITY;

        for dy in -1..=1 {
            let cy_raw = gy + dy;
            let cy = cy_raw.rem_euclid(n as i64) as usize;

            for dx in -1..=1 {
                let cx_raw = gx + dx;
                let cx = cx_raw.rem_euclid(n as i64) as usize;

                let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ user_seed.wrapping_mul(0x27d4_eb2d);
                let h1 = crate::random::hash(h0);
                let h2 = crate::random::hash(h1);

                let jx = (h1 as f32 / 4_294_967_295.0) * 0.8 + 0.1;
                let jy = (h2 as f32 / 4_294_967_295.0) * 0.8 + 0.1;

                let feature_u = (cx_raw as f32 + jx) * inv_n;
                let feature_v = (cy_raw as f32 + jy) * inv_n;

                let du = ((su - feature_u + 0.5).rem_euclid(1.0)) - 0.5;
                let dv = ((sv - feature_v + 0.5).rem_euclid(1.0)) - 0.5;

                let d = (du * du + dv * dv).sqrt();
                if d < f1 {
                    f2 = f1;
                    f1 = d;
                } else if d < f2 {
                    f2 = d;
                }
            }
        }

        // F2 - F1 produces cellular boundary cracks
        let crack_dist = (f2 - f1) * n as f32;
        let crack_val = ((crack_dist / crack_width).clamp(0.0, 1.0) * crack_depth + (1.0 - crack_depth)).clamp(0.0, 1.0);

        [crack_val, crack_val, crack_val, 1.0]
    })])
}
