use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("crystal", "Crystal", CATEGORY)
        .desc("Faceted crystalline cellular noise generator based on 3D distance field intersections.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_1() -> NodeDefinition {
    node("crystal_1", "Crystal 1", CATEGORY)
        .desc("Faceted crystalline geometric crystal planes.")
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("roughness", "Roughness", 0.5, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("crystal_2", "Crystal 2", CATEGORY)
        .desc("Needle and clustered quartz crystalline shards.")
        .float("scale", "Scale", 12.0, 1.0, 64.0)
        .float("roughness", "Roughness", 0.6, 0.0, 1.0)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook_crystal_2)
}

fn cook_crystal_2(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let roughness = context.f("roughness").clamp(0.0, 1.0);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let mut accum = 0.0f32;
        let mut cur_scale = scale;
        let mut weight = 0.6f32;

        for oct in 0..2 {
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
                    let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ seed.wrapping_add((oct as u32) * 555);
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

            let shard = (1.0 - (d2 - d1) * 3.0).clamp(0.0, 1.0);
            accum += shard * weight;
            cur_scale *= 2.0;
            weight *= 0.5;
        }

        let val = (accum * (1.0 + roughness * 0.5)).clamp(0.0, 1.0);
        [val, 0.0, 0.0, 1.0]
    })])
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let roughness = context.f("roughness").clamp(0.0, 1.0);
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

                let h0 = (cx as u32).wrapping_mul(0x85eb_ca6b) ^ (cy as u32).wrapping_mul(0xc2b2_ae35) ^ seed.wrapping_mul(0x27d4_eb2d);
                let h1 = crate::random::hash(h0);
                let h2 = crate::random::hash(h1);
                let h3 = crate::random::hash(h2);

                let fx = cx as f32 + (h1 as f32 / 4_294_967_295.0);
                let fy = cy as f32 + (h2 as f32 / 4_294_967_295.0);
                let fz = h3 as f32 / 4_294_967_295.0;

                let dist = ((x - fx) * (x - fx) + (y - fy) * (y - fy)).sqrt();

                if dist < d1 {
                    d2 = d1;
                    d1 = dist;
                    f_val = fz;
                } else if dist < d2 {
                    d2 = dist;
                }
            }
        }

        // Crystal facet height is a mix of cell height and slope to edge
        let edge_slope = (d2 - d1).clamp(0.0, 1.0);
        let val = (edge_slope * (1.0 - roughness * 0.5) + f_val * (roughness * 0.5)).clamp(0.0, 1.0);

        [val, 0.0, 0.0, 1.0]
    })])
}
