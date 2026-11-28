use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition_1() -> NodeDefinition {
    node("fibers_1", "Fibers 1", CATEGORY)
        .desc("Dense parallel longitudinal surface fibers and composite striations.")
        .int("density", "Density", 32, 4, 128)
        .angle("angle", "Angle", 0.0)
        .float("roughness", "Roughness", 0.3, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_fibers(ctx, false))
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("fibers_2", "Fibers 2", CATEGORY)
        .desc("Cross-ply bidirectional interlaced fiber composite pattern.")
        .int("density", "Density", 32, 4, 128)
        .angle("angle", "Angle", 0.0)
        .float("roughness", "Roughness", 0.3, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_fibers(ctx, true))
}

fn cook_fibers(context: &CookContext<'_>, cross: bool) -> CookResult {
    let (w, h) = (context.w, context.h);
    let density = context.i("density").clamp(1, 256) as f32;
    let angle = context.f("angle") * PI;
    let roughness = context.f("roughness").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;

        let proj1 = x * cos_a - y * sin_a;
        let t1 = (proj1 * density).rem_euclid(1.0);
        let fiber1 = (t1 * PI * 2.0).sin() * 0.5 + 0.5;

        let mut val = fiber1;

        if cross {
            let cos_c = (angle + PI * 0.5).cos();
            let sin_c = (angle + PI * 0.5).sin();
            let proj2 = x * cos_c - y * sin_c;
            let t2 = (proj2 * density).rem_euclid(1.0);
            let fiber2 = (t2 * PI * 2.0).sin() * 0.5 + 0.5;
            val = val.max(fiber2);
        }

        if roughness > 1e-4 {
            let h0 = ((u * 1000.0) as u32).wrapping_mul(0x85eb_ca6b) ^ ((v * 1000.0) as u32).wrapping_mul(0xc2b2_ae35) ^ seed;
            let micro = (crate::random::hash(h0) as f32 / 4_294_967_295.0 - 0.5) * roughness * 0.3;
            val = (val + micro).clamp(0.0, 1.0);
        }

        [val, 0.0, 0.0, 1.0]
    })])
}
