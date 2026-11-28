use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub(crate) fn definition_1() -> NodeDefinition {
    node("mesh_1", "Mesh 1", CATEGORY)
        .desc("Square orthogonal wireframe woven wire mesh grid.")
        .int("tiling", "Tiling", 8, 1, 64)
        .float("wire_thickness", "Wire Thickness", 0.15, 0.01, 0.45)
        .float("bevel", "Bevel Width", 0.05, 0.0, 0.2)
        .gray_out()
        .cook(cook_square)
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("mesh_2", "Mesh 2", CATEGORY)
        .desc("Hexagonal honeycomb / chainmail wire mesh.")
        .int("tiling", "Tiling", 8, 1, 64)
        .float("wire_thickness", "Wire Thickness", 0.12, 0.01, 0.45)
        .gray_out()
        .cook(cook_hex)
}

fn cook_square(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let tiling = context.i("tiling").clamp(1, 64) as f32;
    let thickness = context.f("wire_thickness").clamp(0.01, 0.45);
    let bevel = context.f("bevel").clamp(0.001, 0.2);

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let cx = (u * tiling).rem_euclid(1.0);
        let cy = (v * tiling).rem_euclid(1.0);

        let dx = (cx - 0.5).abs();
        let dy = (cy - 0.5).abs();
        let edge_dist = (0.5 - dx.max(dy)).max(0.0);

        let val = if edge_dist <= thickness { ((thickness - edge_dist) / bevel).clamp(0.0, 1.0) } else { 0.0 };

        [val, 0.0, 0.0, 1.0]
    })])
}

fn cook_hex(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let tiling = context.i("tiling").clamp(1, 64) as f32;
    let thickness = context.f("wire_thickness").clamp(0.01, 0.45);

    let sqrt3 = 3.0f32.sqrt();

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u * tiling * sqrt3;
        let y = v * tiling * 1.5;

        let hx = (x / sqrt3).round();
        let hy = y.round();

        let dx = (x - hx * sqrt3).abs();
        let dy = (y - hy).abs();

        let dist_to_hex = dx * 0.5 + dy * (sqrt3 * 0.5);
        let dist = dist_to_hex.max(dx);

        let val = if (dist - 0.5).abs() < thickness { 1.0 - ((dist - 0.5).abs() / thickness) } else { 0.0 };

        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}
