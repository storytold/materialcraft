use crate::{CookContext, find, resolve_params};
use craft_texture::Image;
use materialcraft_core::Value;
use std::collections::BTreeMap;

fn cook(id: &str, param_overrides: &[(&str, Value)], size: u32) -> Vec<Image> {
    cook_with_inputs(id, &[], param_overrides, size)
}

fn cook_with_inputs(id: &str, inputs: &[Option<Image>], param_overrides: &[(&str, Value)], size: u32) -> Vec<Image> {
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
fn test_shape_disc_and_square() {
    // Disc: center at (16, 16) in a 32x32 image should be 1.0; corner (0, 0) should be 0.0
    let disc = cook("shape", &[("pattern", Value::Enum(0)), ("scale", Value::Float(1.0))], 32);
    assert!((disc[0].get(16, 16, false)[0] - 1.0).abs() < 1e-3);
    assert_eq!(disc[0].get(0, 0, false)[0], 0.0);

    // Square: center (16, 16) is 1.0, corner (0, 0) is 0.0
    let sq = cook("shape", &[("pattern", Value::Enum(1)), ("scale", Value::Float(0.5))], 32);
    assert!((sq[0].get(16, 16, false)[0] - 1.0).abs() < 1e-3);
    assert_eq!(sq[0].get(0, 0, false)[0], 0.0);
}

#[test]
fn test_gradient_linear() {
    let grad = cook("gradient_linear", &[("tiling", Value::Int(1)), ("cycle", Value::Enum(0))], 32);
    // At x=0 (left) value should be near 0; at x=31 (right) value should be near 1.0
    let left = grad[0].get(0, 16, false)[0];
    let right = grad[0].get(31, 16, false)[0];
    assert!(left < 0.1, "left was {left}");
    assert!(right > 0.9, "right was {right}");
}

#[test]
fn test_gradient_axial() {
    let grad = cook("gradient_axial", &[("tiling", Value::Int(1))], 32);
    // Center column x=16 has minimum distance to center line -> near 0.0
    let center = grad[0].get(16, 16, false)[0];
    // Edges have maximum distance -> near 1.0
    let edge = grad[0].get(0, 16, false)[0];
    assert!(center < 0.1, "center was {center}");
    assert!(edge > 0.9, "edge was {edge}");
}

#[test]
fn test_gradient_circular() {
    let grad = cook("gradient_circular", &[("radius", Value::Float(0.5))], 32);
    // Center (16, 16) is at distance 0 -> near 0.0
    let center = grad[0].get(16, 16, false)[0];
    assert!(center < 0.1, "center was {center}");
}

#[test]
fn test_checker() {
    let chk = cook("checker", &[("tiling_x", Value::Int(2)), ("tiling_y", Value::Int(2))], 32);
    // Cell (0, 0): top-left -> (0+0)%2 == 0 -> 1.0
    assert_eq!(chk[0].get(4, 4, false)[0], 1.0);
    // Cell (1, 0): top-right -> (1+0)%2 == 1 -> 0.0
    assert_eq!(chk[0].get(20, 4, false)[0], 0.0);
    // Cell (0, 1): bottom-left -> (0+1)%2 == 1 -> 0.0
    assert_eq!(chk[0].get(4, 20, false)[0], 0.0);
    // Cell (1, 1): bottom-right -> (1+1)%2 == 0 -> 1.0
    assert_eq!(chk[0].get(20, 20, false)[0], 1.0);
}

#[test]
fn test_stripes() {
    let st = cook("stripes", &[("tiling", Value::Int(2)), ("width", Value::Float(0.5)), ("softness", Value::Float(0.0))], 32);
    // Values alternate between 1.0 and 0.0 across the width
    let p0 = st[0].get(0, 16, false)[0];
    assert!(p0 == 0.0 || p0 == 1.0);
}

#[test]
fn test_tile_generator_grid_and_offset() {
    let tiles = cook(
        "tile_generator",
        &[
            ("x_amount", Value::Int(2)),
            ("y_amount", Value::Int(2)),
            ("offset", Value::Float(0.5)),
            ("size_x", Value::Float(0.8)),
            ("size_y", Value::Float(0.8)),
        ],
        32,
    );
    assert_eq!(tiles.len(), 1);
    let img = &tiles[0];

    // Inside row 0, cell 0: center (8, 8) is inside the tile (1.0)
    let center_val = img.get_gray(8, 8, true);
    assert_eq!(center_val, 1.0);

    // In the interstice gap between cells: (0, 8) or (16, 8) gap is 0.0
    let gap_val = img.get_gray(0, 8, true);
    assert_eq!(gap_val, 0.0);
}

#[test]
fn test_hexagon_grid() {
    let hex = cook("hexagon", &[("tiling", Value::Float(4.0)), ("gap", Value::Float(0.1))], 32);
    assert_eq!(hex.len(), 1);
    let img = &hex[0];
    let data = img.data();
    assert!(data.iter().all(|v| v.is_finite()));
    assert!(data.iter().any(|&v| v > 0.5));
    assert!(data.iter().any(|&v| v < 0.5));
}

#[test]
fn test_tile_sampler_basic() {
    let tiles = cook(
        "tile_sampler",
        &[
            ("x_amount", Value::Int(2)),
            ("y_amount", Value::Int(2)),
            ("offset", Value::Float(0.5)),
            ("size_x", Value::Float(0.8)),
            ("size_y", Value::Float(0.8)),
        ],
        32,
    );
    assert_eq!(tiles.len(), 1);
    let img = &tiles[0];
    let center_val = img.get_gray(8, 8, true);
    assert_eq!(center_val, 1.0);
    let gap_val = img.get_gray(0, 8, true);
    assert_eq!(gap_val, 0.0);
}

#[test]
fn test_tile_sampler_mask_culling() {
    use craft_texture::Kind;
    // With 1 pattern input, mask is at index 1 + 6 = 7
    let mask = Image::filled(32, 32, Kind::Gray, [0.0; 4]);
    let inputs = vec![None, None, None, None, None, None, None, Some(mask)];
    let tiles = cook_with_inputs(
        "tile_sampler",
        &inputs,
        &[("x_amount", Value::Int(2)), ("y_amount", Value::Int(2)), ("pattern_input_number", Value::Int(1)), ("mask_map_threshold", Value::Float(0.5))],
        32,
    );
    assert_eq!(tiles.len(), 1);
    assert!(tiles[0].data().iter().all(|&v| v == 0.0));
}

#[test]
fn test_tile_sampler_multi_pattern() {
    use craft_texture::Kind;
    // Pattern 1: filled 0.2
    let p1 = Image::filled(16, 16, Kind::Gray, [0.2; 4]);
    // Pattern 2: filled 0.9
    let p2 = Image::filled(16, 16, Kind::Gray, [0.9; 4]);
    let inputs = vec![Some(p1), Some(p2)];
    let tiles = cook_with_inputs(
        "tile_sampler",
        &inputs,
        &[
            ("pattern", Value::Enum(0)), // Image Input
            ("pattern_input_number", Value::Int(2)),
            ("x_amount", Value::Int(2)),
            ("y_amount", Value::Int(1)),
            ("scale", Value::Float(1.0)),
        ],
        32,
    );
    assert_eq!(tiles.len(), 1);
    let data = tiles[0].data();
    assert!(data.iter().all(|v| v.is_finite()));
    assert!(data.iter().any(|&v| (v - 0.2).abs() < 0.05 || (v - 0.9).abs() < 0.05));
}

#[test]
fn test_tile_sampler_pattern_specific() {
    // Cook with Gaussian (index 5) and Capsule (index 11)
    let g = cook(
        "tile_sampler",
        &[
            ("pattern", Value::Enum(5)), // Gaussian
            ("pattern_specific", Value::Float(0.5)),
            ("x_amount", Value::Int(2)),
            ("y_amount", Value::Int(2)),
        ],
        32,
    );
    assert!(g[0].data().iter().all(|v| v.is_finite()));

    let c = cook(
        "tile_sampler",
        &[
            ("pattern", Value::Enum(11)), // Capsule
            ("pattern_specific", Value::Float(0.2)),
            ("x_amount", Value::Int(2)),
            ("y_amount", Value::Int(2)),
        ],
        32,
    );
    assert!(c[0].data().iter().all(|v| v.is_finite()));
}

#[test]
fn test_weave_pattern() {
    let images =
        cook("weave", &[("x_amount", Value::Int(4)), ("y_amount", Value::Int(4)), ("weave_type", Value::Enum(0)), ("thread_width", Value::Float(0.8))], 32);
    assert_eq!(images.len(), 3);
    for img in &images {
        assert_eq!((img.width(), img.height()), (32, 32));
        assert!(img.data().iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
    }
}

#[test]
fn test_splatter_circular() {
    let images = cook(
        "splatter_circular",
        &[("rings", Value::Int(3)), ("pattern_amount", Value::Int(8)), ("radius", Value::Float(0.4)), ("pattern_size", Value::Float(0.08))],
        32,
    );
    assert_eq!(images.len(), 1);
    let img = &images[0];
    assert_eq!((img.width(), img.height()), (32, 32));
    assert!(img.data().iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
}
