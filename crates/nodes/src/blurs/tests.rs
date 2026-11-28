use crate::{CookContext, find, resolve_params};
use craft_texture::{Image, Kind};
use materialcraft_core::Value;
use std::collections::BTreeMap;

#[test]
fn test_slope_blur_modes() {
    let def = find("slope_blur").unwrap_or_else(|| panic!("slope_blur not found"));

    // Input: white image with black center line
    let input = Image::gray(16, 16, |x, _y| if x == 8 { 0.0 } else { 1.0 });

    // Slope: linear horizontal ramp (dx != 0)
    let slope = Image::gray(16, 16, |x, _y| x as f32 / 16.0);

    let inputs = [Some(input), Some(slope)];

    for mode in 0..=2 {
        let mut overrides = BTreeMap::new();
        overrides.insert("intensity".into(), Value::Float(16.0));
        overrides.insert("samples".into(), Value::Int(8));
        overrides.insert("mode".into(), Value::Enum(mode));

        let params = resolve_params(def, &overrides);
        let no_res = |_: &str| None;
        let vars = BTreeMap::new();

        let context = CookContext {
            format: materialcraft_core::OutputFormat::F32,
            w: 16,
            h: 16,
            seed: 0,
            tile: true,
            inputs: &inputs,
            params: &params,
            resource: &no_res,
            vars: &vars,
        };

        let result = (def.cook)(&context);
        assert!(result.is_ok());
        let images = result.expect("slope blur cook");
        assert_eq!(images.len(), 1);
        let out = &images[0];
        assert_eq!(out.width(), 16);
        assert_eq!(out.height(), 16);
        assert_eq!(out.kind(), Kind::Gray);
    }
}
