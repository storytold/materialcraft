use crate::{CookContext, find, resolve_params};
use craft_texture::{Image, Kind};
use materialcraft_core::Value;
use std::collections::BTreeMap;

fn cook(id: &str, inputs: &[Option<Image>], param_overrides: &[(&str, Value)], size: u32) -> Vec<Image> {
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
        inputs,
        params: &params,
        resource: &no_res,
        vars: &vars,
    };
    (def.cook)(&ctx).unwrap_or_else(|e| panic!("{id} cook error: {e}"))
}

#[test]
fn test_height_blend() {
    let top = Image::filled(16, 16, Kind::Gray, [0.8; 4]);
    let bottom = Image::filled(16, 16, Kind::Gray, [0.2; 4]);
    let res = cook("height_blend", &[Some(top), Some(bottom)], &[("height_offset", Value::Float(0.5))], 16);
    assert_eq!(res.len(), 2);
    assert!(res[0].data().iter().all(|v| v.is_finite()));
    assert!(res[1].data().iter().all(|v| v.is_finite()));
}

#[test]
fn test_material_blend() {
    let bc_top = Image::filled(16, 16, Kind::Color, [1.0, 0.0, 0.0, 1.0]);
    let bc_bottom = Image::filled(16, 16, Kind::Color, [0.0, 1.0, 0.0, 1.0]);
    let mask = Image::filled(16, 16, Kind::Gray, [0.5; 4]);
    let inputs = vec![Some(bc_top), Some(bc_bottom), None, None, None, None, None, None, None, None, Some(mask)];
    let res = cook("material_blend", &inputs, &[], 16);
    assert_eq!(res.len(), 5);
    for img in &res {
        assert!(img.data().iter().all(|v| v.is_finite()));
    }
}

#[test]
fn test_water_drops() {
    let res = cook("water_drops", &[], &[("density", Value::Float(8.0))], 16);
    assert_eq!(res.len(), 3);
    for img in &res {
        assert!(img.data().iter().all(|v| v.is_finite()));
    }
}
