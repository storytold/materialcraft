use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("microscope_view", "Microscope View", CATEGORY)
        .desc("Microscopic biological structure generator featuring cellular membranes, fluid interiors, and cell nuclei.")
        .float("scale", "Scale", 8.0, 1.0, 32.0)
        .float("membrane_thickness", "Membrane Thickness", 0.08, 0.01, 0.3)
        .float("nucleus_size", "Nucleus Size", 0.15, 0.02, 0.4)
        .int("seed", "Random Seed", 1234, 0, 100000)
        .gray_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let scale = context.f("scale").clamp(1.0, 64.0);
    let membrane_thickness = context.f("membrane_thickness").clamp(0.005, 0.5);
    let nucleus_size = context.f("nucleus_size").clamp(0.01, 0.5);
    let seed = context.i("seed").max(0) as u32;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u * scale;
        let y = v * scale;
        let ix = x.floor() as i64;
        let iy = y.floor() as i64;

        let mut d1 = f32::MAX;
        let mut d2 = f32::MAX;

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
                } else if dist < d2 {
                    d2 = dist;
                }
            }
        }

        let edge = d2 - d1;
        let membrane = if edge < membrane_thickness { 1.0 - (edge / membrane_thickness) } else { 0.0 };

        let nucleus = if d1 < nucleus_size { ((1.0 - (d1 / nucleus_size)) * 0.8).clamp(0.0, 1.0) } else { 0.0 };

        let cytoplasm = (1.0 - d1.min(1.0)) * 0.25;

        let val = (membrane * 0.9 + nucleus + cytoplasm).clamp(0.0, 1.0);
        [val, 0.0, 0.0, 1.0]
    })])
}
