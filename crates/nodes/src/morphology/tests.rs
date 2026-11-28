use crate::catalog;
use crate::cook_context::CookContext;
use craft_texture::{Image, Kind};
use materialcraft_core::{OutputFormat, Value};
use std::collections::BTreeMap;

#[test]
fn test_bevel_on_square_mask() {
    let bevel_def = catalog().iter().find(|d| d.id == "bevel").expect("bevel node defined");

    let size = 64u32;
    // Create a 32x32 square in the middle of 64x64
    let mask =
        Image::generate(
            size,
            size,
            Kind::Gray,
            |x, y| {
                if (16..48).contains(&x) && (16..48).contains(&y) { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 0.0, 1.0] }
            },
        );

    let inputs = vec![Some(mask)];
    let mut params = BTreeMap::new();
    params.insert("distance".to_string(), Value::Float(16.0));
    params.insert("curve_type".to_string(), Value::Enum(0)); // Linear

    let resource = |_name: &str| None;
    let vars = BTreeMap::new();
    let ctx =
        CookContext { w: size, h: size, seed: 42, format: OutputFormat::F32, tile: true, inputs: &inputs, params: &params, resource: &resource, vars: &vars };

    let result = (bevel_def.cook)(&ctx).expect("bevel cook succeeds");
    assert_eq!(result.len(), 2);
    let output = &result[0];
    let normal = &result[1];
    assert_eq!(normal.kind(), Kind::Color);

    // Outside the square: 0.0
    assert_eq!(output.get_gray(5, 5, true), 0.0);
    // Center of the square: far enough inside to reach 1.0
    let center_val = output.get_gray(32, 32, true);
    assert!((center_val - 1.0).abs() < 1e-3);
    // Near the boundary: should be between 0.0 and 1.0
    let edge_val = output.get_gray(18, 32, true);
    assert!(edge_val > 0.0 && edge_val < 1.0);
}

#[test]
fn test_edge_detect_on_step() {
    let edge_def = catalog().iter().find(|d| d.id == "edge_detect").expect("edge_detect node defined");

    let size = 64u32;
    // Left half 0.0, right half 1.0
    let step_img = Image::generate(size, size, Kind::Gray, |x, _y| if x >= 32 { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 0.0, 1.0] });

    let inputs = vec![Some(step_img)];
    let mut params = BTreeMap::new();
    params.insert("width".to_string(), Value::Float(4.0));
    params.insert("threshold".to_string(), Value::Float(0.1));
    params.insert("invert".to_string(), Value::Bool(false));

    let resource = |_name: &str| None;
    let vars = BTreeMap::new();
    let ctx =
        CookContext { w: size, h: size, seed: 42, format: OutputFormat::F32, tile: true, inputs: &inputs, params: &params, resource: &resource, vars: &vars };

    let result = (edge_def.cook)(&ctx).expect("edge_detect cook succeeds");
    assert_eq!(result.len(), 1);
    let output = &result[0];

    // Right on the step edge (x = 31 or 32): high edge intensity
    let on_edge = output.get_gray(31, 32, true);
    assert!(on_edge > 0.5);

    // Far from the step edge (x = 10): 0.0
    let far_off = output.get_gray(10, 32, true);
    assert_eq!(far_off, 0.0);
}
