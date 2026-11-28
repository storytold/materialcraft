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
    let ctx =
        CookContext { format: materialcraft_core::OutputFormat::F32, w: 4, h: 4, seed: 0, tile: true, inputs, params: &params, resource: &no_res, vars: &vars };
    (def.cook)(&ctx).unwrap_or_else(|e| panic!("{id} cook error: {e}"))
}

#[test]
fn test_invert_grayscale_and_color() {
    let gray_input = Image::filled(4, 4, Kind::Gray, [0.2, 0.0, 0.0, 0.0]);
    let out = cook("invert", &[Some(gray_input)], &[]);
    let p = out[0].get(0, 0, false);
    assert!((p[0] - 0.8).abs() < 1e-5, "got {}", p[0]);

    let color_input = Image::filled(4, 4, Kind::Color, [0.1, 0.4, 0.9, 0.7]);
    let out_no_alpha = cook("invert", &[Some(color_input.clone())], &[("invert_alpha", Value::Bool(false))]);
    let p = out_no_alpha[0].get(0, 0, false);
    assert!((p[0] - 0.9).abs() < 1e-5);
    assert!((p[1] - 0.6).abs() < 1e-5);
    assert!((p[2] - 0.1).abs() < 1e-5);
    assert!((p[3] - 0.7).abs() < 1e-5, "alpha should be preserved");

    let out_alpha = cook("invert", &[Some(color_input)], &[("invert_alpha", Value::Bool(true))]);
    let p = out_alpha[0].get(0, 0, false);
    assert!((p[3] - 0.3).abs() < 1e-5, "alpha should be inverted");
}

#[test]
fn test_clamp_bounds() {
    let input = Image::generate(4, 4, Kind::Gray, |x, _| match x {
        0 => [0.1, 0.0, 0.0, 0.0],
        1 => [0.5, 0.0, 0.0, 0.0],
        _ => [0.9, 0.0, 0.0, 0.0],
    });
    let out = cook("clamp", &[Some(input)], &[("min", Value::Float(0.3)), ("max", Value::Float(0.7))]);
    assert!((out[0].get(0, 0, false)[0] - 0.3).abs() < 1e-5);
    assert!((out[0].get(1, 0, false)[0] - 0.5).abs() < 1e-5);
    assert!((out[0].get(2, 0, false)[0] - 0.7).abs() < 1e-5);
}

#[test]
fn test_contrast_luminosity() {
    let input = Image::filled(4, 4, Kind::Gray, [0.5, 0.0, 0.0, 0.0]);
    let neutral = cook("contrast_luminosity", &[Some(input.clone())], &[("contrast", Value::Float(0.0)), ("luminosity", Value::Float(0.0))]);
    assert!((neutral[0].get(0, 0, false)[0] - 0.5).abs() < 1e-5);

    let brighter = cook("contrast_luminosity", &[Some(input)], &[("contrast", Value::Float(0.0)), ("luminosity", Value::Float(0.2))]);
    assert!((brighter[0].get(0, 0, false)[0] - 0.7).abs() < 1e-5);
}

#[test]
fn test_histogram_scan() {
    let input = Image::generate(4, 4, Kind::Gray, |x, _| match x {
        0 => [0.3, 0.0, 0.0, 0.0],
        _ => [0.7, 0.0, 0.0, 0.0],
    });
    let sharp = cook("histogram_scan", &[Some(input)], &[("position", Value::Float(0.5)), ("contrast", Value::Float(1.0))]);
    assert_eq!(sharp[0].get(0, 0, false)[0], 0.0);
    assert_eq!(sharp[0].get(1, 0, false)[0], 1.0);
}

#[test]
fn test_histogram_range() {
    let input = Image::generate(4, 4, Kind::Gray, |x, _| match x {
        0 => [0.0, 0.0, 0.0, 0.0],
        1 => [0.5, 0.0, 0.0, 0.0],
        _ => [1.0, 0.0, 0.0, 0.0],
    });
    let out = cook("histogram_range", &[Some(input)], &[("range_min", Value::Float(0.2)), ("range_max", Value::Float(0.8))]);
    assert!((out[0].get(0, 0, false)[0] - 0.2).abs() < 1e-5);
    assert!((out[0].get(1, 0, false)[0] - 0.5).abs() < 1e-5);
    assert!((out[0].get(2, 0, false)[0] - 0.8).abs() < 1e-5);
}

#[test]
fn test_threshold() {
    let input = Image::generate(4, 4, Kind::Gray, |x, _| match x {
        0 => [0.39, 0.0, 0.0, 0.0],
        _ => [0.41, 0.0, 0.0, 0.0],
    });
    let out = cook("threshold", &[Some(input)], &[("threshold", Value::Float(0.4)), ("softness", Value::Float(0.0))]);
    assert_eq!(out[0].get(0, 0, false)[0], 0.0);
    assert_eq!(out[0].get(1, 0, false)[0], 1.0);
}

#[test]
fn test_auto_levels_grayscale() {
    let input = Image::generate(4, 4, Kind::Gray, |x, _| match x {
        0 => [0.2, 0.0, 0.0, 0.0],
        1 => [0.5, 0.0, 0.0, 0.0],
        _ => [0.8, 0.0, 0.0, 0.0],
    });
    let out = cook("auto_levels", &[Some(input)], &[]);
    let p0 = out[0].get(0, 0, false);
    let p1 = out[0].get(1, 0, false);
    let p2 = out[0].get(2, 0, false);
    assert!((p0[0] - 0.0).abs() < 1e-5, "expected 0.0, got {}", p0[0]);
    assert!((p1[0] - 0.5).abs() < 1e-5, "expected 0.5, got {}", p1[0]);
    assert!((p2[0] - 1.0).abs() < 1e-5, "expected 1.0, got {}", p2[0]);
}

#[test]
fn test_auto_levels_flat_does_not_panic() {
    let input = Image::filled(4, 4, Kind::Gray, [0.42, 0.0, 0.0, 0.0]);
    let out = cook("auto_levels", &[Some(input)], &[]);
    let p = out[0].get(0, 0, false);
    assert!(p[0].is_finite());
    assert!((p[0] - 0.42).abs() < 1e-5);
}

#[test]
fn test_auto_levels_color_preserves_alpha() {
    let input = Image::generate(4, 4, Kind::Color, |x, _| match x {
        0 => [0.1, 0.2, 0.3, 0.77],
        _ => [0.6, 0.8, 0.9, 0.77],
    });
    let out = cook("auto_levels", &[Some(input)], &[]);
    let p0 = out[0].get(0, 0, false);
    let p1 = out[0].get(2, 0, false);
    assert!((p0[0] - 0.0).abs() < 1e-5);
    assert!((p1[2] - 1.0).abs() < 1e-5);
    assert!((p0[3] - 0.77).abs() < 1e-5);
    assert!((p1[3] - 0.77).abs() < 1e-5);
}

#[test]
fn test_quantize() {
    let input = Image::gray(4, 4, |x, _| x as f32 / 3.0);
    let out = cook("quantize", &[Some(input)], &[("levels", Value::Int(4)), ("softness", Value::Float(0.0))]);
    let data = out[0].data();
    assert!(data.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
}

#[test]
fn test_highpass() {
    let input = Image::gray(16, 16, |x, _| if x > 8 { 1.0 } else { 0.0 });
    let out = cook("highpass", &[Some(input)], &[("radius", Value::Float(4.0)), ("contrast", Value::Float(1.0))]);
    let data = out[0].data();
    assert!(data.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
}
