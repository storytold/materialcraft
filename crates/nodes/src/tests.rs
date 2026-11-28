use std::collections::BTreeMap;

use craft_texture::{Image, Kind};
use materialcraft_core::Value;

use super::*;

fn cook(def: &NodeDefinition, inputs: &[Option<Image>], size: u32) -> CookResult {
    let params = resolve_params(def, &BTreeMap::new());
    let no_res = |_: &str| None;
    let vars = BTreeMap::new();
    let ctx = CookContext {
        format: materialcraft_core::OutputFormat::F32,
        w: size,
        h: size,
        seed: 7,
        tile: true,
        inputs,
        params: &params,
        resource: &no_res,
        vars: &vars,
    };

    (def.cook)(&ctx)
}

#[test]
fn nearest_transform_mip_keeps_the_last_column_of_an_odd_width_input() {
    let definition = find("transformation").unwrap();
    let mut params = resolve_params(definition, &BTreeMap::new());
    params.insert("filtering".into(), Value::Enum(1));
    let inputs = [Some(Image::gray(3, 1, |x, _| if x == 2 { 1.0 } else { 0.0 }))];
    let vars = BTreeMap::new();
    let resource = |_: &str| None;
    let context = CookContext {
        format: materialcraft_core::OutputFormat::F32,
        w: 2,
        h: 2,
        seed: 0,
        tile: false,
        inputs: &inputs,
        params: &params,
        resource: &resource,
        vars: &vars,
    };

    let output = (definition.cook)(&context).unwrap();

    assert_eq!(output[0].get(0, 0, false)[0], 0.0);
    assert_eq!(output[0].get(1, 0, false)[0], 1.0);
}

/// Image nodes cook finite outputs at the requested size; an empty processor reports its missing result
#[test]
fn every_node_cooks() {
    let ids: Vec<&str> = catalog().iter().map(|definition| definition.id).collect();
    let mut seen = std::collections::BTreeSet::new();
    for id in &ids {
        assert!(seen.insert(*id), "duplicate node id {id}");
    }

    let odd_size_image = Image::generate(5, 3, Kind::Color, |x, y| [x as f32 / 4.0, y as f32 / 2.0, 0.5, 1.0]);
    for definition in catalog() {
        if matches!(definition.id, "graph_instance" | "value_processor" | "input_value") {
            continue;
        }

        for inputs in [vec![None; definition.inputs.len()], vec![Some(odd_size_image.clone()); definition.inputs.len()]] {
            let result = cook(definition, &inputs, 32);
            if definition.id == "pixel_processor" {
                assert_eq!(result.unwrap_err(), "function has no result node");

                continue;
            }

            let outputs = result.unwrap_or_else(|error| panic!("{}: {error}", definition.id));

            assert_eq!(outputs.len(), definition.outputs.len().max(1), "{}", definition.id);

            for image in &outputs {
                assert_eq!((image.width(), image.height()), (32, 32), "{}", definition.id);
                assert!(image.data().iter().all(|value| value.is_finite()), "{} produced a non-finite value", definition.id);
            }
        }
    }

    assert_eq!(catalog().len(), 312);
}

/// Every node cooks with each numeric parameter at a hostile value (huge, negative huge, NaN): the hard limits keep the cook bounded, finite and the right size
#[test]
fn every_node_survives_hostile_params() {
    use std::time::{Duration, Instant};

    let mut slow: Vec<(String, Duration)> = Vec::new();
    let total = Instant::now();
    for def in catalog() {
        if matches!(def.id, "graph_instance" | "value_processor" | "input_value") {
            continue;
        }

        for p in &def.params {
            for h in [1e30_f32, -1e30, f32::NAN] {
                let v = match p.default {
                    Value::Float(_) => Value::Float(h),
                    Value::Int(_) => Value::Int(h as i32),
                    Value::Float2(_) => Value::Float2([h, h]),
                    Value::Enum(_) => Value::Enum(h as u32),
                    _ => continue,
                };

                let mut params = resolve_params(def, &BTreeMap::from([(p.id.to_string(), v)]));
                clamp_params(def, &mut params);
                let inputs = vec![None; def.inputs.len()];
                let no_res = |_: &str| None;
                let vars = BTreeMap::new();
                let ctx = CookContext {
                    format: materialcraft_core::OutputFormat::F32,
                    w: 16,
                    h: 16,
                    seed: 7,
                    tile: true,
                    inputs: &inputs,
                    params: &params,
                    resource: &no_res,
                    vars: &vars,
                };

                let t = Instant::now();
                let out = (def.cook)(&ctx).unwrap_or_else(|e| panic!("{}.{}={h}: {e}", def.id, p.id));
                let dt = t.elapsed();

                assert!(dt < Duration::from_secs(2), "{}.{}={h} took {dt:?}", def.id, p.id);

                if dt > Duration::from_millis(250) {
                    slow.push((format!("{}.{}={h}", def.id, p.id), dt));
                }

                for img in &out {
                    assert_eq!((img.width(), img.height()), (16, 16), "{}.{}={h}", def.id, p.id);
                    assert!(img.data().iter().all(|x| x.is_finite()), "{}.{}={h} produced a non-finite value", def.id, p.id);
                }
            }
        }
    }

    eprintln!("hostile params: total {:?}; cooks over 250 ms: {slow:?}", total.elapsed());
}

#[test]
fn blend_and_levels_math() {
    let fg = Image::filled(4, 4, Kind::Gray, [0.25; 4]);
    let bg = Image::filled(4, 4, Kind::Gray, [0.5; 4]);
    let def = find("blend").unwrap();
    let mut params = resolve_params(def, &BTreeMap::new());
    params.insert("mode".into(), materialcraft_core::Value::Enum(3));
    let no_res = |_: &str| None;
    let vars = BTreeMap::new();
    let inputs = [Some(fg), Some(bg), None];
    let ctx = CookContext {
        format: materialcraft_core::OutputFormat::F32,
        w: 4,
        h: 4,
        seed: 0,
        tile: true,
        inputs: &inputs,
        params: &params,
        resource: &no_res,
        vars: &vars,
    };

    let out = (def.cook)(&ctx).unwrap();

    assert!((out[0].data()[0] - 0.125).abs() < 1e-6);
    assert!((atomic::levels_fn(0.5, 0.0, 0.5, 1.0, 0.0, 1.0) - 0.5).abs() < 1e-5);
    assert_eq!(atomic::levels_fn(0.2, 0.4, 0.5, 0.8, 0.0, 1.0), 0.0);
}

#[test]
fn dynamic_gradient_preserves_the_gradient_image_type() {
    let definition = find("gradient_dynamic").unwrap();
    for kind in [Kind::Gray, Kind::Color] {
        let source = Image::filled(4, 4, Kind::Gray, [0.5; 4]);
        let gradient = Image::filled(4, 4, kind, [0.25, 0.5, 0.75, 1.0]);
        let result = cook(definition, &[Some(source), Some(gradient.clone())], 16).unwrap();

        assert_eq!(result[0].kind(), kind);
        assert_eq!(result[0].get(0, 0, false), gradient.get(0, 0, false));
    }
}

/// Cooks `def` at `w × h` without tiling, with `resource` as the package's bitmaps.
fn cook_sized(def: &NodeDefinition, inputs: &[Option<Image>], w: u32, h: u32, resource: &(dyn Fn(&str) -> Option<Image> + Sync)) -> CookResult {
    let params = resolve_params(def, &BTreeMap::new());
    let vars = BTreeMap::new();
    let ctx = CookContext { format: materialcraft_core::OutputFormat::F32, w, h, seed: 7, tile: false, inputs, params: &params, resource, vars: &vars };
    (def.cook)(&ctx)
}

/// A request wider than the size limit spreads the normalized coordinates over the clamped width.
#[test]
fn an_oversized_transformation_covers_the_whole_input() {
    let ramp = Image::gray(8192, 1, |x, _| x as f32 / 8191.0);
    let out = cook_sized(find("transformation").unwrap(), &[Some(ramp)], 16384, 1, &|_| None).unwrap();
    let last = out[0].get(8191, 0, false)[0];

    assert!(last > 0.99, "the last pixel reads the ramp at {last}");
}

/// Bilinear resampling between finite values of opposite sign near the float limit stays finite.
#[test]
fn transformation_resamples_extreme_neighbors_to_finite_values() {
    let extremes = Image::gray(2, 1, |x, _| if x == 0 { f32::MAX } else { -f32::MAX });
    let out = cook_sized(find("transformation").unwrap(), &[Some(extremes)], 4, 1, &|_| None).unwrap();

    for x in 0..4 {
        let value = out[0].get(x, 0, false)[0];

        assert!(value.is_finite(), "pixel {x} is {value}");
    }
}

#[test]
fn nearest_transformation_mips_preserve_finite_hdr_constants() {
    let definition = find("transformation").unwrap();
    let parameters = resolve_params(definition, &BTreeMap::from([("filtering".into(), Value::Enum(1))]));
    let variables = BTreeMap::new();
    let value = 0.75 * f32::MAX;

    for kind in [Kind::Gray, Kind::Color] {
        let inputs = [Some(Image::filled(16, 16, kind, [value; 4]))];
        let context = CookContext {
            format: materialcraft_core::OutputFormat::F32,
            w: 4,
            h: 4,
            seed: 7,
            tile: false,
            inputs: &inputs,
            params: &parameters,
            resource: &|_| None,
            vars: &variables,
        };

        let output = (definition.cook)(&context).unwrap().remove(0);

        assert_eq!((output.width(), output.height(), output.kind()), (4, 4, kind));
        assert!(output.data().iter().all(|pixel| *pixel == value), "{:?}", output.data());
    }
}

/// A repeated input offset far past its edge keeps the offset's phase: an offset near 2^31 lands on
/// the same place as an offset of zero, halfway between the last and the first column.
#[test]
fn directional_warp_repeats_a_far_offset_with_its_phase() {
    let stripes = Image::gray(3, 3, |x, _| [0.0, 1.0, 4.0][x as usize]);

    // Intensity 10 scales the map by 10 / 256, moving each pixel by about 2^31
    let far = Image::filled(3, 3, Kind::Gray, [2_147_483_648.0 * 25.6; 4]);
    let out = cook(find("directional_warp").unwrap(), &[Some(stripes), Some(far)], 3).unwrap();

    for x in 0..3 {
        assert_eq!(out[0].get_gray(x, 1, false), 2.0, "pixel {x}");
    }
}

/// Shrinking a bitmap of large HDR values averages them without overflowing.
#[test]
fn bitmap_shrinks_large_hdr_values_without_overflow() {
    let hdr = |_: &str| Some(Image::filled(2, 2, Kind::Color, [0.75 * f32::MAX, 0.75 * f32::MAX, 0.75 * f32::MAX, 1.0]));
    let out = cook_sized(find("bitmap").unwrap(), &[], 1, 1, &hdr).unwrap();
    let pixel = out[0].get(0, 0, false);

    assert!(pixel.iter().all(|value| value.is_finite()), "{pixel:?}");
    assert!((pixel[0] / (0.75 * f32::MAX) - 1.0).abs() < 1e-6, "{pixel:?}");
}
