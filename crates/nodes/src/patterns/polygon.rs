use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition() -> NodeDefinition {
    node("polygon", "Polygon", CATEGORY)
        .desc("Regular N-sided polygon generator (triangle, pentagon, hexagon, octagon, etc.) with rotation, scale, and corner rounding.")
        .int("sides", "Sides", 5, 3, 32)
        .float("size", "Size", 0.8, 0.05, 1.5)
        .angle("rotation", "Rotation", 0.0)
        .float("roundness", "Roundness", 0.0, 0.0, 1.0)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_1() -> NodeDefinition {
    node("polygon_1", "Polygon 1", CATEGORY)
        .desc("Standard regular sharp N-sided polygon shape.")
        .int("sides", "Sides", 6, 3, 32)
        .float("size", "Size", 0.8, 0.05, 1.5)
        .angle("rotation", "Rotation", 0.0)
        .float("roundness", "Roundness", 0.0, 0.0, 1.0)
        .gray_out()
        .cook(cook)
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("polygon_2", "Polygon 2", CATEGORY)
        .desc("Beveled / gradient height N-sided polygon.")
        .int("sides", "Sides", 5, 3, 32)
        .float("size", "Size", 0.8, 0.05, 1.5)
        .angle("rotation", "Rotation", 0.0)
        .float("bevel", "Bevel Width", 0.2, 0.01, 1.0)
        .gray_out()
        .cook(cook_beveled)
}

fn cook_beveled(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let sides = context.i("sides").clamp(3, 32) as f32;
    let size = context.f("size").clamp(0.01, 2.0) * 0.5;
    let rot = context.f("rotation") * PI;
    let bevel = context.f("bevel").clamp(0.01, 1.0) * size;

    let cos_r = rot.cos();
    let sin_r = rot.sin();
    let sector_angle = 2.0 * PI / sides;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;

        let rx = x * cos_r - y * sin_r;
        let ry = x * sin_r + y * cos_r;

        let r = (rx * rx + ry * ry).sqrt();
        let mut theta = ry.atan2(rx);
        if theta < 0.0 {
            theta += 2.0 * PI;
        }

        let local_theta = (theta + sector_angle * 0.5).rem_euclid(sector_angle) - sector_angle * 0.5;
        let d = r * local_theta.cos();
        let target_r = size * (sector_angle * 0.5).cos();

        let val = if d <= target_r { ((target_r - d) / bevel).clamp(0.0, 1.0) } else { 0.0 };

        [val, 0.0, 0.0, 1.0]
    })])
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let sides = context.i("sides").clamp(3, 32) as f32;
    let size = context.f("size").clamp(0.01, 2.0) * 0.5;
    let rot = context.f("rotation") * PI;
    let roundness = context.f("roundness").clamp(0.0, 1.0);

    let cos_r = rot.cos();
    let sin_r = rot.sin();
    let sector_angle = 2.0 * PI / sides;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;

        let rx = x * cos_r - y * sin_r;
        let ry = x * sin_r + y * cos_r;

        let r = (rx * rx + ry * ry).sqrt();
        let mut theta = ry.atan2(rx);
        if theta < 0.0 {
            theta += 2.0 * PI;
        }

        // Fold into one sector [-sector_angle / 2, sector_angle / 2]
        let local_theta = (theta + sector_angle * 0.5).rem_euclid(sector_angle) - sector_angle * 0.5;

        // Inscribed radius distance to polygon edge: r * cos(local_theta) <= apothem
        let d = r * local_theta.cos();
        let target_r = size * (sector_angle * 0.5).cos();

        // Blend with circle for roundness
        let eff_d = d * (1.0 - roundness) + r * roundness;
        let val = if eff_d <= target_r { 1.0 } else { 0.0 };

        [val, 0.0, 0.0, 1.0]
    })])
}
