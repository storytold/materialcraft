use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn concrete_definition() -> NodeDefinition {
    node("grunge_concrete", "Grunge Concrete", CATEGORY)
        .desc("Porous concrete weathering with micro-cavities and aggregate blotches.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("roughness", "Roughness", 0.6, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_grunge(ctx, 1.2, 0.5, 4, 101))
}

pub(crate) fn damas_definition() -> NodeDefinition {
    node("grunge_damas", "Grunge Damas", CATEGORY)
        .desc("Faded ornamental damask weathering and aged patina.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("contrast", "Contrast", 1.5, 0.2, 5.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_grunge(ctx, 1.5, 0.45, 5, 202))
}

pub(crate) fn galvanic_large_definition() -> NodeDefinition {
    node("grunge_galvanic_large", "Grunge Galvanic Large", CATEGORY)
        .desc("Large crystalline zinc spangle galvanic weathering.")
        .float("scale", "Scale", 6.0, 1.0, 32.0)
        .float("contrast", "Contrast", 1.8, 0.5, 5.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_galvanic(ctx, 1.0))
}

pub(crate) fn galvanic_small_definition() -> NodeDefinition {
    node("grunge_galvanic_small", "Grunge Galvanic Small", CATEGORY)
        .desc("Fine crystalline zinc spangle galvanic weathering.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .float("contrast", "Contrast", 2.0, 0.5, 5.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_galvanic(ctx, 2.5))
}

pub(crate) fn leaks_definition() -> NodeDefinition {
    node("grunge_leaks", "Grunge Leaks", CATEGORY)
        .desc("Vertical runoff drip trails, water streaks, and washouts.")
        .float("scale", "Scale", 12.0, 1.0, 64.0)
        .float("intensity", "Intensity", 1.0, 0.1, 4.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_leaks)
}

pub(crate) fn leaky_paint_definition() -> NodeDefinition {
    node("grunge_leaky_paint", "Grunge Leaky Paint", CATEGORY)
        .desc("Dripping viscous paint runs and dissolving paint streaks.")
        .float("scale", "Scale", 10.0, 1.0, 64.0)
        .float("intensity", "Intensity", 1.2, 0.1, 4.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_leaks)
}

pub(crate) fn rough_dirty_definition() -> NodeDefinition {
    node("grunge_rough_dirty", "Grunge Rough Dirty", CATEGORY)
        .desc("Heavy coarse dirt accumulation with grit.")
        .float("scale", "Scale", 10.0, 1.0, 64.0)
        .float("contrast", "Contrast", 1.6, 0.2, 5.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_grunge(ctx, 1.6, 0.5, 5, 303))
}

pub(crate) fn rust_fine_definition() -> NodeDefinition {
    node("grunge_rust_fine", "Grunge Rust Fine", CATEGORY)
        .desc("Fine powdery iron rust oxidation flecks.")
        .float("scale", "Scale", 20.0, 1.0, 64.0)
        .float("contrast", "Contrast", 2.2, 0.5, 6.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_grunge(ctx, 2.2, 0.4, 6, 404))
}

pub(crate) fn scratches_dirty_definition() -> NodeDefinition {
    node("grunge_scratches_dirty", "Grunge Scratches Dirty", CATEGORY)
        .desc("Entangled surface scratches filled with dark grime.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .float("density", "Density", 0.5, 0.1, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_scratches(ctx, 1.5))
}

pub(crate) fn scratches_fine_definition() -> NodeDefinition {
    node("grunge_scratches_fine", "Grunge Scratches Fine", CATEGORY)
        .desc("Fine shallow hairline surface abrasions.")
        .float("scale", "Scale", 24.0, 1.0, 64.0)
        .float("density", "Density", 0.4, 0.1, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_scratches(ctx, 0.8))
}

pub(crate) fn scratches_rough_definition() -> NodeDefinition {
    node("grunge_scratches_rough", "Grunge Scratches Rough", CATEGORY)
        .desc("Deep gouges and rough cut abrasions.")
        .float("scale", "Scale", 12.0, 1.0, 64.0)
        .float("density", "Density", 0.6, 0.1, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_scratches(ctx, 2.5))
}

pub(crate) fn shavings_definition() -> NodeDefinition {
    node("grunge_shavings", "Grunge Shavings", CATEGORY)
        .desc("Curled metallic or wooden shavings and ribbon-like debris.")
        .float("scale", "Scale", 14.0, 1.0, 64.0)
        .float("density", "Density", 0.5, 0.1, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_scratches(ctx, 1.8))
}

pub(crate) fn splashes_dusty_definition() -> NodeDefinition {
    node("grunge_splashes_dusty", "Grunge Splashes Dusty", CATEGORY)
        .desc("Fine dusty spray splashes and powder droplets.")
        .float("scale", "Scale", 16.0, 1.0, 64.0)
        .float("density", "Density", 0.5, 0.1, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_splashes(ctx, true))
}

pub(crate) fn spots_definition() -> NodeDefinition {
    node("grunge_spots", "Grunge Spots", CATEGORY)
        .desc("Circular liquid droplet drying spots and water rings.")
        .float("scale", "Scale", 12.0, 1.0, 64.0)
        .float("contrast", "Contrast", 2.0, 0.5, 5.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_splashes(ctx, false))
}

pub(crate) fn spots_dirty_definition() -> NodeDefinition {
    node("grunge_spots_dirty", "Grunge Spots Dirty", CATEGORY)
        .desc("Muddy irregular spatter spots with textured centers.")
        .float("scale", "Scale", 14.0, 1.0, 64.0)
        .float("contrast", "Contrast", 2.5, 0.5, 6.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(|ctx| cook_splashes(ctx, true))
}

fn cook_grunge(context: &CookContext<'_>, contrast: f32, balance: f32, octaves: usize, seed_off: u32) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let n1 = super::perlin::sample_perlin_fbm(u, v, scale, octaves, 0.6, 2.0, seed.wrapping_add(seed_off));
        let n2 = super::perlin::sample_perlin_fbm(u + 0.3, v + 0.7, scale * 2.5, 3, 0.7, 2.2, seed.wrapping_add(seed_off * 2));
        let raw = n1 * 0.7 + n2 * 0.3;
        let val = ((raw - balance) * contrast + 0.5).clamp(0.0, 1.0);
        [val, 0.0, 0.0, 1.0]
    })])
}

fn cook_galvanic(context: &CookContext<'_>, scale_mult: f32) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0) * scale_mult;
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u * scale;
        let y = v * scale;
        let ix = x.floor() as i64;
        let iy = y.floor() as i64;

        let mut d1 = f32::MAX;
        let mut d2 = f32::MAX;
        let mut f_val = 0.0f32;

        for dy in -1..=1 {
            for dx in -1..=1 {
                let cx = ix + dx;
                let cy = iy + dy;
                let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ seed;
                let h1 = crate::random::hash(h0);
                let h2 = crate::random::hash(h1);

                let fx = cx as f32 + (h1 as f32 / 4_294_967_295.0);
                let fy = cy as f32 + (h2 as f32 / 4_294_967_295.0);

                let dist = ((x - fx) * (x - fx) + (y - fy) * (y - fy)).sqrt();
                if dist < d1 {
                    d2 = d1;
                    d1 = dist;
                    f_val = h2 as f32 / 4_294_967_295.0;
                } else if dist < d2 {
                    d2 = dist;
                }
            }
        }

        let edge = (d2 - d1).clamp(0.0, 1.0);
        let val = (edge * 0.5 + f_val * 0.5).clamp(0.0, 1.0);
        [val, 0.0, 0.0, 1.0]
    })])
}

fn cook_leaks(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let intensity = context.f("intensity").clamp(0.1, 4.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        // Vertical stretched flow
        let x = u * scale;
        let y = v * (scale * 0.1);
        let fbm = super::perlin::sample_perlin_fbm(x, y, 1.0, 4, 0.6, 2.0, seed);
        // Vertical attenuation
        let falloff = (1.0 - (fbm * 2.0 - 1.0).abs()).powf(2.0);
        let val = (falloff * intensity * (v * 0.7 + 0.3)).clamp(0.0, 1.0);
        [val, 0.0, 0.0, 1.0]
    })])
}

fn cook_scratches(context: &CookContext<'_>, strength: f32) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let density = context.f("density").clamp(0.05, 1.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let mut accum = 0.0f32;
        for i in 0..3 {
            let angle = (i as f32 * (PI / 3.0)) + (seed as f32 * 0.1);
            let rx = (u * angle.cos() - v * angle.sin()) / 12.0;
            let ry = u * angle.sin() + v * angle.cos();
            let n = super::perlin::sample_perlin_fbm(rx, ry, scale, 3, 0.7, 2.0, seed.wrapping_add((i as u32) * 555));
            let th = 1.0 - density * 0.3;
            if n > th {
                accum = accum.max(((n - th) / (1.0 - th)) * strength);
            }
        }
        [accum.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_splashes(context: &CookContext<'_>, dirty: bool) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
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

                if (h3 % 100) > 40 {
                    continue;
                }

                let fx = cx as f32 + (h1 as f32 / 4_294_967_295.0);
                let fy = cy as f32 + (h2 as f32 / 4_294_967_295.0);
                let r = 0.2 + (h3 as f32 / 4_294_967_295.0) * 0.3;

                let d = ((x - fx) * (x - fx) + (y - fy) * (y - fy)).sqrt();
                if d < r {
                    let spot = if dirty {
                        (1.0 - d / r).powf(1.5)
                    } else {
                        // Ring / droplet edge
                        1.0 - ((d - r * 0.8).abs() / (r * 0.2)).min(1.0)
                    };
                    sum += spot;
                }
            }
        }
        [sum.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}
