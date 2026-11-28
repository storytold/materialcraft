use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use std::f32::consts::PI;

pub(crate) fn definition_1() -> NodeDefinition {
    node("gaussian_1", "Gaussian 1", CATEGORY)
        .desc("Symmetric radial 2D Gaussian bell distribution.")
        .float("radius", "Radius", 0.4, 0.05, 1.0)
        .gray_out()
        .cook(cook_radial)
}

pub(crate) fn definition_2() -> NodeDefinition {
    node("gaussian_2", "Gaussian 2", CATEGORY)
        .desc("Anisotropic / elliptical 2D Gaussian profile with independent X and Y radii and orientation.")
        .float("radius_x", "Radius X", 0.4, 0.05, 1.0)
        .float("radius_y", "Radius Y", 0.2, 0.05, 1.0)
        .angle("angle", "Angle", 0.0)
        .gray_out()
        .cook(cook_anisotropic)
}

fn cook_radial(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let r = context.f("radius").clamp(0.01, 2.0);
    let two_sigma_sq = 2.0 * r * r;

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let dx = u - 0.5;
        let dy = v - 0.5;
        let d_sq = dx * dx + dy * dy;
        let val = (-d_sq / two_sigma_sq).exp();
        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}

fn cook_anisotropic(context: &CookContext<'_>) -> CookResult {
    let (w, h) = (context.w, context.h);
    let rx = context.f("radius_x").clamp(0.01, 2.0);
    let ry = context.f("radius_y").clamp(0.01, 2.0);
    let angle = context.f("angle") * PI;

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    Ok(vec![Image::generate_uv(w, h, Kind::Gray, |u, v| {
        let dx = u - 0.5;
        let dy = v - 0.5;

        let rot_x = dx * cos_a - dy * sin_a;
        let rot_y = dx * sin_a + dy * cos_a;

        let d = (rot_x * rot_x) / (2.0 * rx * rx) + (rot_y * rot_y) / (2.0 * ry * ry);
        let val = (-d).exp();
        [val.clamp(0.0, 1.0), 0.0, 0.0, 1.0]
    })])
}
