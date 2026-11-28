use crate::{CookContext, find, resolve_params};
use craft_texture::{Image, Kind};
use materialcraft_core::Value;
use std::collections::BTreeMap;

fn cook(id: &str, inputs: &[Option<Image>], param_overrides: &[(&str, Value)]) -> Vec<Image> {
    let def = find(id).unwrap_or_else(|| panic!("node {id} not found"));
    let mut overrides = BTreeMap::new();
    for (k, v) in param_overrides {
        overrides.insert((*k).to_string(), v.clone());
    }
    let params = resolve_params(def, &overrides);
    let no_res = |_: &str| None;
    let vars = BTreeMap::new();
    let (w, h) = inputs.first().and_then(|img| img.as_ref()).map_or((4, 4), |img| (img.width(), img.height()));
    let ctx = CookContext { format: materialcraft_core::OutputFormat::F32, w, h, seed: 0, tile: true, inputs, params: &params, resource: &no_res, vars: &vars };
    (def.cook)(&ctx).unwrap_or_else(|e| panic!("{id} cook error: {e}"))
}

#[test]
fn test_normal_combine_with_flat_detail_preserves_base() {
    // Base normal tilted right: (nx=0.5, ny=0, nz=sqrt(0.75)=0.866) -> RGB: [0.75, 0.5, 0.933, 1.0]
    let base = Image::filled(4, 4, Kind::Color, [0.75, 0.5, 0.933, 1.0]);
    // Detail normal is flat: [0.5, 0.5, 1.0, 1.0]
    let flat_detail = Image::filled(4, 4, Kind::Color, [0.5, 0.5, 1.0, 1.0]);

    let out = cook("normal_combine", &[Some(base), Some(flat_detail)], &[]);
    let p = out[0].get(0, 0, false);
    assert!((p[0] - 0.75).abs() < 1e-3, "got rx {}", p[0]);
    assert!((p[1] - 0.5).abs() < 1e-3, "got ry {}", p[1]);
    assert!((p[2] - 0.933).abs() < 1e-3, "got rz {}", p[2]);
}

#[test]
fn test_normal_combine_with_flat_base_preserves_detail() {
    let flat_base = Image::filled(4, 4, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    // Unit normal vector (0, 0.6, 0.8): len = sqrt(0.36 + 0.64) = 1.0 -> encoded [0.5, 0.8, 0.9, 1.0]
    let detail = Image::filled(4, 4, Kind::Color, [0.5, 0.8, 0.9, 1.0]);

    let out = cook("normal_combine", &[Some(flat_base), Some(detail)], &[]);
    let p = out[0].get(0, 0, false);
    assert!((p[0] - 0.5).abs() < 1e-3);
    assert!((p[1] - 0.8).abs() < 1e-3);
    assert!((p[2] - 0.9).abs() < 1e-3);
}

#[test]
fn test_normal_combine_detail_intensity_and_mask() {
    let flat_base = Image::filled(4, 4, Kind::Color, [0.5, 0.5, 1.0, 1.0]);
    let detail = Image::filled(4, 4, Kind::Color, [0.8, 0.5, 0.8, 1.0]);

    // Zero intensity should produce flat normal
    let out = cook("normal_combine", &[Some(flat_base.clone()), Some(detail.clone())], &[("detail_intensity", Value::Float(0.0))]);
    let p = out[0].get(0, 0, false);
    assert!((p[0] - 0.5).abs() < 1e-3);
    assert!((p[1] - 0.5).abs() < 1e-3);
    assert!((p[2] - 1.0).abs() < 1e-3);

    // Zero mask should also produce flat normal
    let mask_zero = Image::filled(4, 4, Kind::Gray, [0.0, 0.0, 0.0, 0.0]);
    let out_masked = cook("normal_combine", &[Some(flat_base), Some(detail), Some(mask_zero)], &[]);
    let p = out_masked[0].get(0, 0, false);
    assert!((p[0] - 0.5).abs() < 1e-3);
    assert!((p[1] - 0.5).abs() < 1e-3);
    assert!((p[2] - 1.0).abs() < 1e-3);
}

#[test]
fn test_normal_invert() {
    // Normal tilted upwards in OpenGL: (x=0, y=0.5, z=0.866) -> RGB [0.5, 0.75, 0.933, 1.0]
    let input = Image::filled(4, 4, Kind::Color, [0.5, 0.75, 0.933, 1.0]);

    // Default: invert_y = true (converts to DirectX: Green flips from 0.75 to 0.25)
    let out = cook("normal_invert", &[Some(input)], &[]);
    let p = out[0].get(0, 0, false);
    assert!((p[0] - 0.5).abs() < 1e-5);
    assert!((p[1] - 0.25).abs() < 1e-5, "Y should invert to 0.25");
    assert!((p[2] - 0.933).abs() < 1e-5);
}

#[test]
fn test_ambient_occlusion_flat_and_crevice() {
    // Flat height should produce 1.0 AO (no occlusion)
    let flat_height = Image::filled(16, 16, Kind::Gray, [0.5, 0.0, 0.0, 1.0]);
    let out_flat = cook("ambient_occlusion", &[Some(flat_height)], &[]);
    let val_flat = out_flat[0].get_gray(8, 8, true);
    assert!((val_flat - 1.0).abs() < 1e-3, "Flat height should have full illumination: {val_flat}");

    // Height with a crevice in the middle
    let crevice = Image::generate(32, 32, Kind::Gray, |x, y| {
        // Deep hole at (16, 16)
        if (15..=17).contains(&x) && (15..=17).contains(&y) { [0.0, 0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0, 1.0] }
    });

    let out_crevice = cook("ambient_occlusion", &[Some(crevice)], &[("radius", Value::Float(32.0))]);
    let center_ao = out_crevice[0].get_gray(16, 16, true);
    let rim_ao = out_crevice[0].get_gray(2, 2, true);

    assert!(center_ao < rim_ao, "Center of hole should be darker ({center_ao}) than rim ({rim_ao})");
}

#[test]
fn test_curvature_modes_and_inputs() {
    // Height with a sharp peak at center
    let peak = Image::generate(32, 32, Kind::Gray, |x, y| {
        let dx = (x as f32 - 16.0).abs();
        let dy = (y as f32 - 16.0).abs();
        let dist = (dx * dx + dy * dy).sqrt();
        let h = (1.0 - dist / 8.0).max(0.0);
        [h, 0.0, 0.0, 1.0]
    });

    // Combined mode: peak should be > 0.5 (convex)
    let out_comb = cook("curvature", &[Some(peak.clone())], &[("mode", Value::Enum(0))]);
    let peak_val = out_comb[0].get_gray(16, 16, true);
    let flat_val = out_comb[0].get_gray(0, 0, true);
    assert!(peak_val > 0.5, "Peak should be convex (>0.5): {peak_val}");
    assert!((flat_val - 0.5).abs() < 1e-3, "Flat area should be 0.5: {flat_val}");

    // Convex only mode
    let out_convex = cook("curvature", &[Some(peak)], &[("mode", Value::Enum(1))]);
    let peak_convex = out_convex[0].get_gray(16, 16, true);
    let flat_convex = out_convex[0].get_gray(0, 0, true);
    assert!(peak_convex > 0.0, "Convex mode on peak should be positive: {peak_convex}");
    assert_eq!(flat_convex, 0.0, "Convex mode on flat should be 0.0");
}
