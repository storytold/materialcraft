use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};

pub const ALGORITHMS: &[&str] = &["Reoriented (RNM)", "Whiteout", "Partial Derivative"];

pub(crate) fn definition() -> NodeDefinition {
    node("normal_combine", "Normal Combine", CATEGORY)
        .desc("Blends a base normal map with a detail normal map using Reoriented Normal Mapping (RNM) or alternative algorithms.")
        .color_in("base", "Base Normal")
        .color_in("detail", "Detail Normal")
        .primary_input("base")
        .gray_in("mask", "Mask")
        .float("detail_intensity", "Detail intensity", 1.0, 0.0, 5.0)
        .choice("algorithm", "Algorithm", ALGORITHMS, 0)
        .color_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let base = context.sized(0).map_or_else(|| Image::filled(context.w, context.h, Kind::Color, [0.5, 0.5, 1.0, 1.0]), |img| img.to_color());
    let detail = context.sized(1).map_or_else(|| Image::filled(context.w, context.h, Kind::Color, [0.5, 0.5, 1.0, 1.0]), |img| img.to_color());
    let mask = context.sized(2).map(|img| img.to_gray());

    let intensity = context.f("detail_intensity");
    let intensity = if intensity.is_finite() { intensity.max(0.0) } else { 1.0 };
    let algorithm = context.i("algorithm");

    Ok(vec![Image::generate(context.w, context.h, Kind::Color, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let b = base.get(x, y, false);
        let d = detail.get(x, y, false);
        let m = mask.as_ref().map_or(1.0, |img| img.get_gray(x, y, false));
        let m = if m.is_finite() { m.clamp(0.0, 1.0) } else { 0.0 };

        let eff_intensity = intensity * m;

        // Decode base normal from [0, 1] to [-1, 1]
        let mut x1 = b[0] * 2.0 - 1.0;
        let mut y1 = b[1] * 2.0 - 1.0;
        let mut z1 = b[2] * 2.0 - 1.0;
        let len1 = (x1 * x1 + y1 * y1 + z1 * z1).sqrt();
        if len1 > 1e-6 && len1.is_finite() {
            x1 /= len1;
            y1 /= len1;
            z1 /= len1;
        } else {
            x1 = 0.0;
            y1 = 0.0;
            z1 = 1.0;
        }

        // Decode detail normal and apply effective intensity
        let d_x = (d[0] * 2.0 - 1.0) * eff_intensity;
        let d_y = (d[1] * 2.0 - 1.0) * eff_intensity;
        let d_z = (d[2] * 2.0 - 1.0).max(0.0);
        let len2 = (d_x * d_x + d_y * d_y + d_z * d_z).sqrt();
        let (x2, y2, z2) = if len2 > 1e-6 && len2.is_finite() { (d_x / len2, d_y / len2, d_z / len2) } else { (0.0, 0.0, 1.0) };

        // Combine using selected algorithm
        let (rx, ry, rz) = match algorithm {
            // Whiteout blending
            1 => (x1 + x2, y1 + y2, z1 * z2),
            // Partial Derivative blending
            2 => (x1 * z2 + x2 * z1, y1 * z2 + y2 * z1, z1 * z2),
            // Reoriented Normal Mapping (RNM) - default
            _ => {
                let t_z = (z1 + 1.0).max(1e-4);
                let k = z2 - (x1 * x2 + y1 * y2) / t_z;
                (x1 * k + x2, y1 * k + y2, t_z * k - z2)
            }
        };

        let r_len = (rx * rx + ry * ry + rz * rz).sqrt();
        let (nx, ny, nz) = if r_len > 1e-6 && r_len.is_finite() { (rx / r_len, ry / r_len, (rz / r_len).max(0.0)) } else { (0.0, 0.0, 1.0) };

        [nx * 0.5 + 0.5, ny * 0.5 + 0.5, nz * 0.5 + 0.5, b[3]]
    })])
}
