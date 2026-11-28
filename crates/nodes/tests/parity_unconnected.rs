use craft_texture::Kind;
use materialcraft_core::{OutputFormat, Value};
use materialcraft_nodes::{CookContext, find, resolve_params};
use std::collections::BTreeMap;

#[test]
fn absent_textures_have_zero_pixels_and_expected_kinds() {
    for (id, kind, pixel) in [
        ("channels_shuffle", Kind::Color, [0.0; 4]),
        ("hsl", Kind::Color, [0.0; 4]),
        ("emboss", Kind::Gray, [0.0, 0.0, 0.0, 1.0]),
        ("gradient_dynamic", Kind::Gray, [0.0, 0.0, 0.0, 1.0]),
    ] {
        let definition = find(id).unwrap();
        let params = resolve_params(definition, &BTreeMap::new());
        let resource = |_: &str| None;
        let vars = BTreeMap::new();
        for (w, h) in [(16, 16), (32, 16), (3, 5)] {
            let context = CookContext {
                input_formats: &[],
                w,
                h,
                seed: 0,
                format: OutputFormat::F32,
                tile: true,
                inputs: &[],
                params: &params,
                resource: &resource,
                vars: &vars,
            };

            let outputs = (definition.cook)(&context).unwrap();
            let image = outputs.first().unwrap();

            assert_eq!(image.kind(), kind, "{id}");

            for y in 0..h {
                for x in 0..w {
                    assert_eq!(image.get(x.into(), y.into(), false), pixel, "{id} at {x},{y}");
                }
            }
        }
    }
}

#[test]
fn fresh_catalog_defaults_have_expected_values() {
    for (id, parameter, expected) in [
        ("uniform", "color", Value::Color([0.0, 0.0, 0.0, 1.0])),
        ("blur", "intensity", Value::Float(10.0)),
        ("distance", "combine", Value::Bool(true)),
        ("emboss", "light_angle", Value::Float(0.0)),
        ("emboss", "shadow", Value::Color([0.0; 4])),
        ("fx_map", "roughness", Value::Float(1.0)),
        ("fx_map", "color_mode", Value::Bool(true)),
        ("gradient_dynamic", "position", Value::Float(0.0)),
        ("grayscale_conversion", "matte", Value::Float(1.0)),
        ("normal", "opengl", Value::Bool(false)),
        ("normal", "height_alpha", Value::Bool(true)),
        ("text", "fontsize", Value::Float(0.25)),
        ("text", "text", Value::Text(String::new())),
    ] {
        assert_eq!(find(id).unwrap().param(parameter).unwrap().default, expected, "{id}.{parameter}");
    }
    for (op, expected) in [("const_float1", Value::Float(0.0)), ("const_float4", Value::Float4([0.0; 4])), ("while", Value::Int(32))] {
        assert_eq!(materialcraft_nodes::function::default_value(op), Some(expected), "{op}");
    }
}
