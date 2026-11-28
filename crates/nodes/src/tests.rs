use std::collections::BTreeMap;

use craft_texture::{Image, Kind};
use materialcraft_core::Value;

use super::*;

fn cook(def: &NodeDefinition, inputs: &[Option<Image>], size: u32) -> CookResult {
    let params = resolve_params(def, &BTreeMap::new());
    let no_res = |_: &str| None;
    let vars = BTreeMap::new();
    let ctx = CookContext {
        input_formats: &[],
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
fn pixel_processor_stores_undefined_channels_at_output_precision() {
    use materialcraft_core::{FunctionGraph, OutputFormat};

    let definition = find("pixel_processor").unwrap();
    let mut graph = FunctionGraph::default();
    let pos = graph.add("get_float2", &[], Some(Value::Text("$pos".into())));
    let input = graph.add("samplecol", &[pos], Some(Value::Int2([0, 0])));
    let divisor = graph.add_constant(Value::Float4([0.0, 0.0, 0.0, 2.0]));
    graph.output = Some(graph.add("div", &[input, divisor], None));
    let params = resolve_params(definition, &BTreeMap::from([("function".into(), Value::Function(Box::new(graph)))]));
    let inputs = [Some(Image::filled(2, 2, Kind::Color, [0.0, 1.0, -1.0, 0.5]))];
    let vars = BTreeMap::new();
    for (format, expected) in [
        (OutputFormat::F32, [0x7fff_ffff, 0x7f80_0000, 0xff80_0000, 0x3e80_0000]),
        (OutputFormat::F16, [half::f16::from_bits(0x7fff).to_f32().to_bits(), 0x7f80_0000, 0xff80_0000, 0x3e80_0000]),
        (OutputFormat::U16, [0, 1.0_f32.to_bits(), 0, (16384.0_f32 / 65535.0).to_bits()]),
        (OutputFormat::U8, [0, 1.0_f32.to_bits(), 0, (64.0_f32 / 255.0).to_bits()]),
    ] {
        let context = CookContext {
            w: 2,
            h: 2,
            inputs: &inputs,
            input_formats: &[OutputFormat::F32],
            seed: 0,
            tile: true,
            params: &params,
            vars: &vars,
            resource: &|_| None,
            format,
        };
        let image = (definition.cook)(&context).unwrap().remove(0);
        let stored = output_precision::apply(image, format);
        assert_eq!(stored.get(0, 0, false).map(f32::to_bits), expected, "{format:?}");
    }
}

#[test]
fn bilinear_transform_selects_and_stores_the_requested_mip_level() {
    let definition = find("transformation").unwrap();
    let inputs = [Some(Image::gray(16, 16, |x, y| if x % 4 == 0 && y % 4 == 0 { 1.0 } else { 0.0 }).with_integer_sampling(true))];
    let variables = BTreeMap::new();
    let filtered = 4096.0 / 65535.0;

    for (size, mode, level, first, second) in [(4, 0, 0, filtered, filtered), (4, 1, 0, 0.0, 0.0), (16, 0, 0, 1.0, 0.0), (16, 1, 2, filtered, filtered)] {
        let params = resolve_params(definition, &BTreeMap::from([("mip_mode".into(), Value::Enum(mode)), ("mip_level".into(), Value::Int(level))]));
        let context = CookContext {
            input_formats: &[],
            format: materialcraft_core::OutputFormat::U16,
            w: size,
            h: size,
            seed: 0,
            tile: true,
            inputs: &inputs,
            params: &params,
            resource: &|_| None,
            vars: &variables,
        };
        let result = (definition.cook)(&context).unwrap();
        assert_eq!([result[0].get_gray(0, 0, false), result[0].get_gray(1, 0, false)], [first, second], "size {size}, mode {mode}, level {level}");
    }
}

#[test]
fn grayscale_bitmap_halving_stores_each_level_before_the_next_filter() {
    use materialcraft_core::OutputFormat;

    let definition = find("bitmap").unwrap();
    let params = resolve_params(definition, &BTreeMap::from([("color_mode".into(), Value::Bool(false)), ("resource".into(), Value::Text("field".into()))]));
    let source = Image::gray(2, 4, |x, y| {
        let level = 1000 * x + u32::from(y == 3);
        level as f32 / 65535.0
    })
    .with_integer_sampling(true);
    let vars = BTreeMap::new();
    let context = CookContext {
        input_formats: &[],
        format: OutputFormat::U16,
        w: 2,
        h: 1,
        seed: 0,
        tile: false,
        inputs: &[],
        params: &params,
        resource: &|_| Some(source.clone()),
        vars: &vars,
    };
    let result = (definition.cook)(&context).unwrap();
    let stored = crate::output_precision::apply(result[0].clone(), context.format);
    assert_eq!(stored.gray_data().unwrap(), [1.0 / 65535.0, 1001.0 / 65535.0]);
}

#[test]
fn linear_color_bitmap_filters_each_channel_at_stored_precision() {
    use materialcraft_core::OutputFormat;

    let definition = find("bitmap").unwrap();
    let params = resolve_params(definition, &BTreeMap::from([("colour_space".into(), Value::Enum(1)), ("resource".into(), Value::Text("field".into()))]));
    let level = |x, y| ((31 * x + 13 * y + 97 * x * y + 19) % 256) as f32 / 255.0;
    let source = Image::generate(64, 32, Kind::Color, |x, y| [level(x, y), level(x, (y + 27) % 32), level((x + 55) % 64, y), level((x + 47) % 64, y)])
        .with_integer_sampling(true);
    let context = CookContext {
        input_formats: &[],
        format: OutputFormat::U16,
        w: 64,
        h: 16,
        seed: 497109,
        tile: true,
        inputs: &[],
        params: &params,
        resource: &|_| Some(source.clone()),
        vars: &BTreeMap::new(),
    };
    let result = (definition.cook)(&context).unwrap();
    let stored = crate::output_precision::apply(result[0].clone(), context.format);
    let levels = [(0, 0), (7, 3), (63, 15)].map(|(x, y)| stored.get(x, y, false).map(|v| (v * 65535.0).round() as u16));
    assert_eq!(levels, [[6554, 30969, 44718, 45746], [32382, 31354, 40221, 28913], [21074, 46774, 47417, 19661]]);
}

#[test]
fn bitmap_converts_normalized_rgb_to_gray_on_the_first_filter_pass() {
    use materialcraft_core::OutputFormat;

    let definition = find("bitmap").unwrap();
    let params = resolve_params(
        definition,
        &BTreeMap::from([("colour_space".into(), Value::Enum(1)), ("color_mode".into(), Value::Bool(false)), ("resource".into(), Value::Text("field".into()))]),
    );
    let source = Image::generate(32, 64, Kind::Color, |x, y| {
        [(583, 191, 371, 3229), (47, 313, 59, 4511), (31, 693, 41, 8943), (997, 811, 97, 9043)]
            .map(|(a, b, c, d)| ((a * x + b * y + c * x * y + d) % 65536) as f32 / 65535.0)
    })
    .with_integer_sampling(true);
    for (format, size, positions, expected) in [
        (OutputFormat::U16, [16, 16], [(0, 0), (3, 7), (15, 15)], [6388, 27024, 28209].map(|v| v as f32 / 65535.0)),
        (OutputFormat::F32, [32, 64], [(0, 0), (3, 7), (31, 63)], [0x3dadc8af, 0x3e4060c0, 0x3e80ab2c].map(f32::from_bits)),
    ] {
        let context = CookContext {
            input_formats: &[],
            format,
            w: size[0],
            h: size[1],
            seed: 520885,
            tile: true,
            inputs: &[],
            params: &params,
            resource: &|_| Some(source.clone()),
            vars: &BTreeMap::new(),
        };
        let result = (definition.cook)(&context).unwrap();
        let stored = crate::output_precision::apply(result[0].clone(), format);
        assert_eq!(positions.map(|(x, y)| stored.get_gray(x, y, false)), expected);
    }
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
        input_formats: &[],
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

    assert_eq!(catalog().len(), 29);
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
                    input_formats: &[],
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
        input_formats: &[],
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
    let ctx = CookContext {
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        w,
        h,
        seed: 7,
        tile: false,
        inputs,
        params: &params,
        resource,
        vars: &vars,
    };
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
            input_formats: &[],
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

fn warp_byte_inputs(source: [u32; 2], map: [u32; 2]) -> [Option<Image>; 2] {
    let level = |n: i32, denominator: f32| (n as f32 * 255.0 / denominator).round_ties_even() / 255.0;
    let field = Image::gray(source[0], source[1], |x, y| level(((41 * x + 53 * y + 7 * x * y + 29) % 1021) as i32, 1020.0)).with_integer_sampling(true);
    let mask = Image::gray(map[0], map[1], |x, y| {
        let (x, y) = (x as i32, y as i32);
        level((11 * x - 17 * y + 3 * x * y + 401).rem_euclid(509), 508.0)
    })
    .with_integer_sampling(true);
    [Some(field), Some(mask)]
}

#[test]
fn directional_warp_samples_stored_texels_along_positive_image_angles() {
    use materialcraft_core::OutputFormat;

    let inputs = warp_byte_inputs([64; 2], [64; 2]);
    let definition = find("directional_warp").unwrap();
    for (angle, intensity, tiling, interior, corner) in [
        (0.0, 7.0, 3, 0x3f1e109e, 0x3e741cf4),
        (0.25, 11.0, 3, 0x3f1eaa9f, 0x3ee6a8e7),
        (0.125, 17.0, 3, 0x3f2346a3, 0x3d6d90ee),
        (0.1875, 25.0, 0, 0x3f258da6, 0x3d20a0a1),
        (-0.3125, 21.5, 1, 0x3f134293, 0x3efbecfc),
    ] {
        let params = resolve_params(definition, &BTreeMap::from([("intensity".into(), Value::Float(intensity)), ("angle".into(), Value::Float(angle))]));
        let vars = BTreeMap::from([("$tiling".into(), Value::Int(tiling))]);
        let context = CookContext {
            w: 64,
            h: 64,
            seed: 751,
            tile: tiling != 0,
            format: OutputFormat::F32,
            input_formats: &[OutputFormat::U8; 2],
            inputs: &inputs,
            params: &params,
            vars: &vars,
            resource: &|_| None,
        };
        let image = (definition.cook)(&context).unwrap().remove(0);
        assert_eq!(image.get_gray(7, 13, false).to_bits(), interior, "angle {angle}");
        assert_eq!(image.get_gray(63, 63, false).to_bits(), corner, "tiling {tiling}");
    }
}

#[test]
fn horizontal_warp_at_zero_turns_keeps_the_full_displacement() {
    use materialcraft_core::OutputFormat;

    let definition = find("directional_warp").unwrap();
    let inputs = [Some(Image::gray(16, 16, |x, _| x as f32)), Some(Image::filled(16, 16, Kind::Gray, [1025.0 / 2048.0; 4]))];
    for (angle, expected) in [(0.0, 2.0 + 1.0 / 256.0), (1.0, 2.0)] {
        let params = resolve_params(definition, &BTreeMap::from([("intensity".into(), Value::Float(64.0)), ("angle".into(), Value::Float(angle))]));
        let context = CookContext {
            w: 16,
            h: 16,
            seed: 395191,
            tile: true,
            format: OutputFormat::F32,
            input_formats: &[OutputFormat::F32; 2],
            inputs: &inputs,
            params: &params,
            vars: &BTreeMap::new(),
            resource: &|_| None,
        };
        let image = (definition.cook)(&context).unwrap().remove(0);
        assert_eq!(image.get_gray(0, 0, false), expected, "angle {angle}");
    }
}

#[test]
fn directional_warp_filters_unequal_image_and_mask_sizes() {
    use materialcraft_core::OutputFormat;

    let definition = find("directional_warp").unwrap();
    let params = resolve_params(definition, &BTreeMap::from([("intensity".into(), Value::Float(17.0)), ("angle".into(), Value::Float(0.125))]));
    for (source, map, side, center, edge) in [
        ([64; 2], Some([16, 32]), 64, 0x3deaa0eb, 0x3e5bb8dc),
        ([64; 2], Some([16, 32]), 32, 0x3ed030d0, 0x3e91a092),
        ([16; 2], Some([128; 2]), 32, 0x3f2f3cd9, 0x3d754e55),
        ([8, 32], None, 128, 0x3e92e2d9, 0x3e18104b),
        ([128, 16], None, 16, 0x3ef47d4d, 0x3efe4e5f),
    ] {
        let mut inputs = warp_byte_inputs(source, map.unwrap_or([1, 1]));
        if map.is_none() {
            inputs[1] = Some(Image::filled(side, side, Kind::Gray, [1.0; 4]).with_integer_sampling(true));
        }
        let context = CookContext {
            w: side,
            h: side,
            seed: 863,
            tile: true,
            format: OutputFormat::F32,
            input_formats: &[OutputFormat::U8; 2],
            inputs: &inputs,
            params: &params,
            vars: &BTreeMap::new(),
            resource: &|_| None,
        };
        let image = (definition.cook)(&context).unwrap().remove(0);
        assert_eq!(image.get_gray(7, 13, false).to_bits(), center, "source {source:?}, map {map:?}, output {side}");
        assert_eq!(image.get_gray(0, 0, false).to_bits(), edge);
    }
}

#[test]
fn directional_warp_preserves_the_wider_format_while_resizing_each_input() {
    use materialcraft_core::OutputFormat::{F16, F32, U8, U16};

    let definition = find("directional_warp").unwrap();
    let params = resolve_params(definition, &BTreeMap::from([("intensity".into(), Value::Float(17.0)), ("angle".into(), Value::Float(0.125))]));
    for (source_format, mask_format, output, expected) in [
        (F32, F32, U8, [0, 172, 114, 0, 1]),
        (F16, U16, U16, [0, 44159, 29248, 0, 250]),
        (U16, U8, F16, [12326, 14539, 14209, 12653, 13043]),
        (U8, F32, U16, [8408, 39305, 30722, 11126, 14133]),
    ] {
        let input_value = |v: f64, format| match format {
            U8 => (v * 255.0).round_ties_even() as f32 / 255.0,
            U16 => (v * 65535.0).round_ties_even() as f32 / 65535.0,
            F16 => half::f16::from_f64(v).to_f32(),
            _ => v as f32,
        };
        let image = Image::gray(16, 32, |x, y| {
            let value = f64::from((41 * x + 53 * y + 7 * x * y + 29) % 1021) / 1020.0;
            input_value(if matches!(source_format, F16 | F32) { value * 1.75 - 0.375 } else { value }, source_format)
        })
        .with_integer_sampling(matches!(source_format, U8 | U16));
        let mask = Image::gray(32, 16, |x, y| {
            let (x, y) = (x as i32, y as i32);
            input_value(f64::from((11 * x - 17 * y + 3 * x * y + 401).rem_euclid(509)) / 508.0, mask_format)
        })
        .with_integer_sampling(matches!(mask_format, U8 | U16));
        let context = CookContext {
            w: 64,
            h: 64,
            seed: 1,
            tile: true,
            format: output,
            input_formats: &[source_format, mask_format],
            inputs: &[Some(image), Some(mask)],
            params: &params,
            vars: &BTreeMap::new(),
            resource: &|_| None,
        };
        let result = crate::output_precision::apply((definition.cook)(&context).unwrap().remove(0), output);
        let stored = [(0, 0), (7, 13), (14, 5), (32, 32), (63, 63)].map(|(x, y)| {
            let v = result.get_gray(x, y, false);
            if output == F16 { half::f16::from_f32(v).to_bits() } else { (v * if output == U8 { 255.0 } else { 65535.0 }).round() as u16 }
        });
        assert_eq!(stored, expected, "{source_format:?}/{mask_format:?} into {output:?}");
    }
}

#[test]
fn directional_warp_keeps_large_authored_displacements_and_their_direction() {
    use materialcraft_core::OutputFormat;

    let definition = find("directional_warp").unwrap();
    let inputs = [Some(Image::generate(64, 64, Kind::Color, |x, y| [x as f32, y as f32, 0.0, 1.0])), Some(Image::filled(64, 64, Kind::Gray, [1.0; 4]))];
    for (angle, expected) in [(0.125, [7.59375, 7.59375]), (0.5, [32.03125, 31.953125]), (0.625, [56.53125, 56.34375])] {
        let params = resolve_params(definition, &BTreeMap::from([("intensity".into(), Value::Float(2_097_152.0)), ("angle".into(), Value::Float(angle))]));
        assert_eq!(params["intensity"], Value::Float(2_097_152.0));
        let context = CookContext {
            w: 64,
            h: 64,
            seed: 887,
            tile: true,
            format: OutputFormat::F32,
            input_formats: &[OutputFormat::F32; 2],
            inputs: &inputs,
            params: &params,
            vars: &BTreeMap::new(),
            resource: &|_| None,
        };
        let pixel = (definition.cook)(&context).unwrap().remove(0).get(32, 32, false);
        assert_eq!(pixel[..2], expected, "angle {angle}");
    }
}

#[test]
fn rectangular_directional_warps_scale_both_offsets_by_the_shorter_side() {
    let definition = find("directional_warp").unwrap();
    let params = resolve_params(definition, &BTreeMap::from([("intensity".into(), Value::Float(16.0)), ("angle".into(), Value::Float(0.125))]));
    for (width, height, shift) in [(32, 16, 0.70703125), (16, 32, 0.70703125), (128, 32, 1.4140625), (32, 128, 1.4140625)] {
        let inputs = [
            Some(Image::generate(width, height, Kind::Color, |x, y| [x as f32, y as f32, 0.0, 1.0])),
            Some(Image::filled(width, height, Kind::Gray, [1.0; 4])),
        ];
        let context = CookContext {
            w: width,
            h: height,
            seed: 887,
            tile: true,
            format: materialcraft_core::OutputFormat::F32,
            input_formats: &[],
            inputs: &inputs,
            params: &params,
            vars: &BTreeMap::new(),
            resource: &|_| None,
        };
        let image = (definition.cook)(&context).unwrap().remove(0);
        let (x, y) = (width / 2, height / 2);
        assert_eq!(image.get(i64::from(x), i64::from(y), false), [x as f32 + shift, y as f32 + shift, 0.0, 1.0]);
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
