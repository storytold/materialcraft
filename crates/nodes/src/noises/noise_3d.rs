use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn perlin_3d_definition() -> NodeDefinition {
    node("perlin_3d", "3D Perlin Noise", CATEGORY)
        .desc("Slices a 3D procedural Perlin noise volume at a customizable Z depth.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("z_slice", "Z Slice", 0.0, 0.0, 10.0)
        .int("octaves", "Octaves", 4, 1, 8)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_perlin_3d)
}

pub(crate) fn voronoi_3d_definition() -> NodeDefinition {
    node("voronoi_3d", "3D Voronoi", CATEGORY)
        .desc("Slices a 3D procedural Voronoi cellular volume at a customizable Z depth.")
        .float("scale", "Scale", 8.0, 1.0, 32.0)
        .float("z_slice", "Z Slice", 0.0, 0.0, 10.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_voronoi_3d)
}

pub(crate) fn perlin_3d_fractal_definition() -> NodeDefinition {
    node("perlin_3d_fractal", "3D Perlin Noise Fractal", CATEGORY)
        .desc("Multi-octave 3D fractal Perlin noise volume sliced at Z depth.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("z_slice", "Z Slice", 0.0, 0.0, 10.0)
        .int("octaves", "Octaves", 6, 1, 10)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_perlin_3d)
}

pub(crate) fn ridged_3d_fractal_definition() -> NodeDefinition {
    node("ridged_3d_fractal", "3D Ridged Noise Fractal", CATEGORY)
        .desc("Multi-octave 3D ridged fractal noise volume with sharp crests.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("z_slice", "Z Slice", 0.0, 0.0, 10.0)
        .int("octaves", "Octaves", 5, 1, 10)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_ridged_3d)
}

pub(crate) fn simplex_3d_definition() -> NodeDefinition {
    node("simplex_3d", "3D Simplex Noise", CATEGORY)
        .desc("3D Simplex gradient noise volume sliced at customizable Z depth.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("z_slice", "Z Slice", 0.0, 0.0, 10.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_simplex_3d)
}

pub(crate) fn voronoi_3d_fractal_definition() -> NodeDefinition {
    node("voronoi_3d_fractal", "3D Voronoi Fractal", CATEGORY)
        .desc("Multi-octave 3D Voronoi cellular fractal distance volume.")
        .float("scale", "Scale", 8.0, 1.0, 32.0)
        .float("z_slice", "Z Slice", 0.0, 0.0, 10.0)
        .int("octaves", "Octaves", 4, 1, 8)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_voronoi_3d_fractal)
}

pub(crate) fn worley_3d_definition() -> NodeDefinition {
    node("worley_3d", "3D Worley Noise", CATEGORY)
        .desc("3D Worley cellular noise volume (F2 - F1 boundary thickness metric).")
        .float("scale", "Scale", 8.0, 1.0, 32.0)
        .float("z_slice", "Z Slice", 0.0, 0.0, 10.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_worley_3d)
}

fn cook_ridged_3d(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let z_slice = context.f("z_slice");
    let octaves = context.i("octaves").clamp(1, 10) as usize;
    let persistence = context.f("roughness").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let mut sum = 0.0f32;
        let mut cur_scale = scale;
        let mut amp = 1.0f32;
        let mut max_amp = 0.0f32;

        for oct in 0..octaves {
            let z_off = z_slice * (oct as f32 + 1.0) * 0.17;
            let val = super::perlin::sample_perlin_fbm(u + z_off, v + z_off * 0.73, cur_scale, 1, 0.5, 2.0, seed.wrapping_add((oct as u32) * 333));
            let ridge = 1.0 - (val * 2.0 - 1.0).abs();
            sum += ridge * amp;
            max_amp += amp;
            cur_scale *= 2.0;
            amp *= persistence;
        }

        let final_val = if max_amp > 1e-5 { sum / max_amp } else { 0.0 };
        [final_val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_simplex_3d(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let z_slice = context.f("z_slice");
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let z_off = z_slice * 0.23;
        let val = super::perlin::sample_perlin_fbm(u + z_off, v + z_off * 0.61, scale, 2, 0.5, 2.0, seed);
        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_voronoi_3d_fractal(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let z_slice = context.f("z_slice");
    let octaves = context.i("octaves").clamp(1, 8) as usize;
    let persistence = context.f("roughness").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let mut sum = 0.0f32;
        let mut cur_scale = scale;
        let mut amp = 1.0f32;
        let mut max_amp = 0.0f32;

        for oct in 0..octaves {
            let x = u * cur_scale;
            let y = v * cur_scale;
            let z = (z_slice + (oct as f32 * 0.31)) * cur_scale * 0.25;

            let ix = x.floor() as i64;
            let iy = y.floor() as i64;
            let iz = z.floor() as i64;

            let mut min_d = f32::MAX;
            for dz in -1..=1 {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let cx = ix + dx;
                        let cy = iy + dy;
                        let cz = iz + dz;

                        let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b)
                            ^ (cy as u32).wrapping_mul(0xc2b2_ae35)
                            ^ (cz as u32).wrapping_mul(0x27d4_eb2d)
                            ^ seed.wrapping_add((oct as u32) * 444);
                        let h1 = crate::random::hash(h0);
                        let h2 = crate::random::hash(h1);
                        let h3 = crate::random::hash(h2);

                        let fx = cx as f32 + (h1 as f32 / 4_294_967_295.0);
                        let fy = cy as f32 + (h2 as f32 / 4_294_967_295.0);
                        let fz = cz as f32 + (h3 as f32 / 4_294_967_295.0);

                        let dist = ((x - fx) * (x - fx) + (y - fy) * (y - fy) + (z - fz) * (z - fz)).sqrt();
                        if dist < min_d {
                            min_d = dist;
                        }
                    }
                }
            }

            sum += min_d.clamp(0.0, 1.0) * amp;
            max_amp += amp;
            cur_scale *= 2.0;
            amp *= persistence;
        }

        let val = if max_amp > 1e-5 { sum / max_amp } else { 0.0 };
        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_worley_3d(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let z_slice = context.f("z_slice");
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u * scale;
        let y = v * scale;
        let z = z_slice * scale * 0.25;

        let ix = x.floor() as i64;
        let iy = y.floor() as i64;
        let iz = z.floor() as i64;

        let mut d1 = f32::MAX;
        let mut d2 = f32::MAX;

        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let cx = ix + dx;
                    let cy = iy + dy;
                    let cz = iz + dz;

                    let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ (cz as u32).wrapping_mul(0x27d4_eb2d) ^ seed;
                    let h1 = crate::random::hash(h0);
                    let h2 = crate::random::hash(h1);
                    let h3 = crate::random::hash(h2);

                    let fx = cx as f32 + (h1 as f32 / 4_294_967_295.0);
                    let fy = cy as f32 + (h2 as f32 / 4_294_967_295.0);
                    let fz = cz as f32 + (h3 as f32 / 4_294_967_295.0);

                    let dist = ((x - fx) * (x - fx) + (y - fy) * (y - fy) + (z - fz) * (z - fz)).sqrt();
                    if dist < d1 {
                        d2 = d1;
                        d1 = dist;
                    } else if dist < d2 {
                        d2 = dist;
                    }
                }
            }
        }

        let val = (d2 - d1).clamp(0.0, 1.0);
        [val, 0.0, 0.0, 1.0]
    })])
}

fn cook_perlin_3d(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let z_slice = context.f("z_slice");
    let octaves = context.i("octaves").clamp(1, 8) as usize;
    let persistence = context.f("roughness").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let mut sum = 0.0f32;
        let mut cur_scale = scale;
        let mut amp = 1.0f32;
        let mut max_amp = 0.0f32;

        for oct in 0..octaves {
            let z_off = z_slice * (oct as f32 + 1.0) * 0.17;
            let val = super::perlin::sample_perlin_fbm(u + z_off, v + z_off * 0.73, cur_scale, 1, 0.5, 2.0, seed.wrapping_add((oct as u32) * 333));
            sum += val * amp;
            max_amp += amp;
            cur_scale *= 2.0;
            amp *= persistence;
        }

        let final_val = if max_amp > 1e-5 { sum / max_amp } else { 0.0 };
        [final_val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_voronoi_3d(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let z_slice = context.f("z_slice");
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u * scale;
        let y = v * scale;
        let z = z_slice * scale * 0.25;

        let ix = x.floor() as i64;
        let iy = y.floor() as i64;
        let iz = z.floor() as i64;

        let mut min_d = f32::MAX;

        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let cx = ix + dx;
                    let cy = iy + dy;
                    let cz = iz + dz;

                    let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ (cz as u32).wrapping_mul(0x27d4_eb2d) ^ seed;
                    let h1 = crate::random::hash(h0);
                    let h2 = crate::random::hash(h1);
                    let h3 = crate::random::hash(h2);

                    let fx = cx as f32 + (h1 as f32 / 4_294_967_295.0);
                    let fy = cy as f32 + (h2 as f32 / 4_294_967_295.0);
                    let fz = cz as f32 + (h3 as f32 / 4_294_967_295.0);

                    let dist = ((x - fx) * (x - fx) + (y - fy) * (y - fy) + (z - fz) * (z - fz)).sqrt();
                    if dist < min_d {
                        min_d = dist;
                    }
                }
            }
        }

        [min_d.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}
