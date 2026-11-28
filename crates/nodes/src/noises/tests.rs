use crate::{CookContext, find, resolve_params};
use craft_texture::Image;
use materialcraft_core::Value;
use std::collections::BTreeMap;

fn cook(id: &str, param_overrides: &[(&str, Value)], size: u32) -> Vec<Image> {
    let def = find(id).unwrap_or_else(|| panic!("node {id} not found"));
    let mut overrides = BTreeMap::new();
    for (k, v) in param_overrides {
        overrides.insert((*k).to_string(), v.clone());
    }
    let params = resolve_params(def, &overrides);
    let no_res = |_: &str| None;
    let vars = BTreeMap::new();
    let ctx = CookContext {
        format: materialcraft_core::OutputFormat::F32,
        w: size,
        h: size,
        seed: 0,
        tile: true,
        inputs: &[],
        params: &params,
        resource: &no_res,
        vars: &vars,
    };
    (def.cook)(&ctx).unwrap_or_else(|e| panic!("{id} cook error: {e}"))
}

#[test]
fn test_perlin_noise_bounds_and_tiling() {
    let images = cook(
        "perlin_noise",
        &[
            ("scale", Value::Float(8.0)),
            ("octaves", Value::Int(4)),
            ("persistence", Value::Float(0.5)),
            ("lacunarity", Value::Float(2.0)),
            ("disorder", Value::Float(42.0)),
        ],
        64,
    );
    assert_eq!(images.len(), 1);
    let img = &images[0];
    for y in 0..64 {
        for x in 0..64 {
            let p = img.get_gray(x, y, true);
            assert!((0.0..=1.0).contains(&p), "Perlin pixel out of bounds: {p}");
        }
    }
}

#[test]
fn test_cells_modes() {
    for mode in 0..=4 {
        let images = cook("cells", &[("scale", Value::Float(4.0)), ("disorder", Value::Float(0.8)), ("mode", Value::Enum(mode))], 32);
        assert_eq!(images.len(), 1);
        let img = &images[0];
        for y in 0..32 {
            for x in 0..32 {
                let p = img.get_gray(x, y, true);
                assert!((0.0..=1.0).contains(&p), "Cells pixel out of bounds: {p} in mode {mode}");
            }
        }
    }
}

#[test]
fn test_white_noise() {
    let images = cook("white_noise", &[("scale", Value::Int(1))], 32);
    assert_eq!(images.len(), 1);
    let data = images[0].data();
    assert!(data.iter().all(|v| (0.0..=1.0).contains(v)));
    assert!(data.iter().any(|&v| v > 0.8));
    assert!(data.iter().any(|&v| v < 0.2));
}

#[test]
fn test_simplex_noise() {
    let images = cook("simplex_noise", &[("scale", Value::Float(8.0)), ("octaves", Value::Int(3))], 32);
    assert_eq!(images.len(), 1);
    let data = images[0].data();
    assert!(data.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn test_turbulence() {
    let images = cook("turbulence", &[("scale", Value::Float(8.0)), ("octaves", Value::Int(4))], 32);
    assert_eq!(images.len(), 1);
    let data = images[0].data();
    assert!(data.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn test_cracks() {
    let images = cook("cracks", &[("scale", Value::Float(4.0)), ("crack_width", Value::Float(0.1)), ("mode", Value::Enum(0))], 32);
    assert_eq!(images.len(), 1);
    let data = images[0].data();
    assert!(data.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn test_grunge_map() {
    let images = cook("grunge_map", &[("scale", Value::Float(4.0)), ("contrast", Value::Float(1.2)), ("balance", Value::Float(0.5))], 32);
    assert_eq!(images.len(), 1);
    let data = images[0].data();
    assert!(data.iter().all(|v| (0.0..=1.0).contains(v)));
}
