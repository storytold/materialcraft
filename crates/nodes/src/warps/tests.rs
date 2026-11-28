use crate::{CookContext, find, resolve_params};
use craft_texture::{Image, Kind};
use materialcraft_core::Value;
use std::collections::BTreeMap;

#[test]
fn test_multi_directional_warp() {
    let def = find("multi_directional_warp").expect("multi_directional_warp defined");

    let input = Image::filled(32, 32, Kind::Gray, [0.5, 0.0, 0.0, 1.0]);
    let warp_map = Image::filled(32, 32, Kind::Gray, [1.0, 0.0, 0.0, 1.0]);

    let mut overrides = BTreeMap::new();
    overrides.insert("intensity".to_string(), Value::Float(8.0));
    overrides.insert("directions".to_string(), Value::Enum(1)); // 4 directions
    overrides.insert("mode".to_string(), Value::Enum(0)); // Average

    let params = resolve_params(def, &overrides);
    let inputs = vec![Some(input), Some(warp_map)];
    let no_res = |_: &str| None;
    let vars = BTreeMap::new();
    let ctx = CookContext {
        format: materialcraft_core::OutputFormat::F32,
        w: 32,
        h: 32,
        seed: 0,
        tile: true,
        inputs: &inputs,
        params: &params,
        resource: &no_res,
        vars: &vars,
    };

    let result = (def.cook)(&ctx).expect("multi_directional_warp cook succeeds");
    assert_eq!(result.len(), 1);
    let out = &result[0];
    assert!(out.data().iter().all(|v| v.is_finite()));
    assert_eq!(out.get_gray(16, 16, true), 0.5);
}
