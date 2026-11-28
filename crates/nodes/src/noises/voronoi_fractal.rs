use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("voronoi_fractal", "Voronoi Fractal", CATEGORY)
        .desc("Multi-octave fractal Voronoi noise with customizable distance metrics and persistence.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .int("octaves", "Octaves", 4, 1, 8)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .choice("metric", "Distance Metric", &["F1 (Closest)", "F2 - F1 (Edges)", "F2 (Second Closest)"], 0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let octaves = context.i("octaves").clamp(1, 8) as usize;
    let persistence = context.f("roughness").clamp(0.0, 1.0);
    let metric = context.i("metric");
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let mut sum = 0.0f32;
        let mut cur_scale = scale;
        let mut amp = 1.0f32;
        let mut max_amp = 0.0f32;

        for oct in 0..octaves {
            let x = u * cur_scale;
            let y = v * cur_scale;
            let ix = x.floor() as i64;
            let iy = y.floor() as i64;

            let mut d1 = f32::MAX;
            let mut d2 = f32::MAX;

            for dy in -1..=1 {
                for dx in -1..=1 {
                    let cx = ix + dx;
                    let cy = iy + dy;
                    let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ seed.wrapping_add((oct as u32) * 997);
                    let h1 = crate::random::hash(h0);
                    let h2 = crate::random::hash(h1);

                    let fx = cx as f32 + (h1 as f32 / 4_294_967_295.0);
                    let fy = cy as f32 + (h2 as f32 / 4_294_967_295.0);

                    let dist = ((x - fx) * (x - fx) + (y - fy) * (y - fy)).sqrt();
                    if dist < d1 {
                        d2 = d1;
                        d1 = dist;
                    } else if dist < d2 {
                        d2 = dist;
                    }
                }
            }

            let sample = match metric {
                1 => (d2 - d1).clamp(0.0, 1.0),
                2 => d2.clamp(0.0, 1.0),
                _ => d1.clamp(0.0, 1.0),
            };

            sum += sample * amp;
            max_amp += amp;
            cur_scale *= 2.0;
            amp *= persistence;
        }

        let val = if max_amp > 1e-5 { (sum / max_amp).clamp(0.0, 1.0) } else { 0.0 };
        [val, 0.0, 0.0, 1.0]
    })])
}
