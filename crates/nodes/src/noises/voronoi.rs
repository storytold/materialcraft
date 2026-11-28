use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition() -> NodeDefinition {
    node("cells", "Cells", CATEGORY)
        .desc("Cellular / Voronoi noise with distance metrics, cracks, and cell color modes.")
        .generator()
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("disorder", "Disorder", 1.0, 0.0, 1.0)
        .choice("mode", "Mode", &["Distance (F1)", "Second Distance (F2)", "Cracks (F2 - F1)", "Cell Color", "Manhattan (F1)"], 0)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_1() -> NodeDefinition {
    node("cells_1", "Cells 1", CATEGORY)
        .desc("Voronoi F1 closest distance cellular gradient cones.")
        .generator()
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("disorder", "Disorder", 1.0, 0.0, 1.0)
        .gray_out()
        .cook(|ctx| cook_fixed_mode(ctx, 0))
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("cells_2", "Cells 2", CATEGORY)
        .desc("Voronoi F2 - F1 cell boundary ridges and crack network.")
        .generator()
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("disorder", "Disorder", 1.0, 0.0, 1.0)
        .gray_out()
        .cook(|ctx| cook_fixed_mode(ctx, 2))
}

pub(crate) fn definition_3() -> NodeDefinition {
    node("cells_3", "Cells 3", CATEGORY)
        .desc("Voronoi F2 second closest distance rounded pebble shapes.")
        .generator()
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("disorder", "Disorder", 1.0, 0.0, 1.0)
        .gray_out()
        .cook(|ctx| cook_fixed_mode(ctx, 1))
}

pub(crate) fn definition_4() -> NodeDefinition {
    node("cells_4", "Cells 4", CATEGORY)
        .desc("Voronoi random flat luminance value per cell.")
        .generator()
        .float("scale", "Scale", 8.0, 1.0, 64.0)
        .float("disorder", "Disorder", 1.0, 0.0, 1.0)
        .gray_out()
        .cook(|ctx| cook_fixed_mode(ctx, 3))
}

fn cook_fixed_mode(context: &CookContext<'_>, mode: i32) -> CookResult {
    let scale = context.f("scale").clamp(1.0, 64.0);
    let disorder = context.f("disorder").clamp(0.0, 1.0);
    cook_internal(context, scale, disorder, mode)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let scale = context.f("scale").clamp(1.0, 64.0);
    let disorder = context.f("disorder").clamp(0.0, 1.0);
    let mode = context.i("mode");
    cook_internal(context, scale, disorder, mode)
}

fn cook_internal(context: &CookContext<'_>, scale: f32, disorder: f32, mode: i32) -> CookResult {
    let period = scale.round().max(1.0) as i32;

    Ok(vec![Image::generate_uv(context.w, context.h, Kind::Gray, |u, v| {
        let u_scaled = u * period as f32;
        let v_scaled = v * period as f32;

        let gx = u_scaled.floor() as i32;
        let gy = v_scaled.floor() as i32;

        let fx = u_scaled - gx as f32;
        let fy = v_scaled - gy as f32;

        let mut min_d1 = f32::MAX;
        let mut min_d2 = f32::MAX;
        let mut min_manhattan_d1 = f32::MAX;
        let mut closest_cell_hash = 0u32;

        for dy in -1..=1 {
            for dx in -1..=1 {
                let cx = (gx + dx).rem_euclid(period) as u32;
                let cy = (gy + dy).rem_euclid(period) as u32;

                let h1 = crate::random::hash(cx.wrapping_mul(0x85eb_ca6b) ^ cy);
                let h2 = crate::random::hash(cy.wrapping_mul(0xc2b2_ae35) ^ cx);

                let jx = 0.5 + disorder * ((h1 as f32 / 4_294_967_295.0) - 0.5);
                let jy = 0.5 + disorder * ((h2 as f32 / 4_294_967_295.0) - 0.5);

                let px = dx as f32 + jx - fx;
                let py = dy as f32 + jy - fy;

                let dist_sq = px * px + py * py;
                let dist = dist_sq.sqrt();
                let manhattan = px.abs() + py.abs();

                if dist < min_d1 {
                    min_d2 = min_d1;
                    min_d1 = dist;
                    closest_cell_hash = h1 ^ h2;
                } else if dist < min_d2 {
                    min_d2 = dist;
                }

                if manhattan < min_manhattan_d1 {
                    min_manhattan_d1 = manhattan;
                }
            }
        }

        let val = match mode {
            0 => min_d1,
            1 => min_d2,
            2 => (min_d2 - min_d1) * 2.0,
            3 => crate::random::hash(closest_cell_hash) as f32 / 4_294_967_295.0,
            4 => min_manhattan_d1,
            _ => min_d1,
        };

        [val.clamp(0.0, 1.0), 0.0, 0.0, 0.0]
    })])
}
