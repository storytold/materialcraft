use craft_texture::{Image, Kind};
use materialcraft_core::{FunctionGraph, PatternGraph, PatternNodeKind, Value};
use materialcraft_nodes::{CookContext, find, resolve_params};
use std::collections::BTreeMap;

fn cook(def: &str, mut values: BTreeMap<String, Value>, inputs: &[Option<Image>]) -> Image {
    let definition = find(def).unwrap();

    // Geometry and maximum-blend fixtures use unattenuated grayscale patterns
    if def == "fx_map" {
        values.entry("roughness".into()).or_insert(Value::Float(0.0));
        values.entry("color_mode".into()).or_insert(Value::Bool(false));
    }

    let params = resolve_params(definition, &values);
    let context = CookContext {
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        w: 64,
        h: 64,
        seed: 0,
        tile: true,
        inputs,
        params: &params,
        resource: &|_| None,
        vars: &BTreeMap::new(),
    };

    (definition.cook)(&context).unwrap().remove(0)
}

#[test]
fn hsl_hue_endpoints_and_half_turn_preserve_alpha() {
    let color = [0.7, 0.3, 0.1, 0.6];
    let inputs = [Some(Image::filled(64, 64, Kind::Color, color))];
    for (hue, expected) in [
        (0.0, [0.69990844, 0.29946107, 0.10009158]),
        (0.5, [0.69990844, 0.29946128, 0.10009158]),
        (1.0, [0.69990844, 0.29946128, 0.10009158]),
        (0.75, [0.10009158, 0.5005387, 0.69990844]),
    ] {
        let image = cook("hsl", BTreeMap::from([("hue".into(), Value::Float(hue))]), &inputs);
        let pixel = image.get(0, 0, false);
        for (actual, expected) in pixel.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-6, "hue {hue}: {actual} != {expected}");
        }
        assert_eq!(pixel[3], color[3]);
    }
}

#[test]
fn hsl_offsets_clamp_in_normalized_lightness_and_restore_hdr_gain() {
    let gray = [0.125, 0.125, 0.125, 0.5];
    for (source, slider, value, expected) in [
        (gray, "saturation", 0.75, [0.1875, 0.0625, 0.0625, 0.5]),
        (gray, "lightness", 0.75, [0.625, 0.625, 0.625, 0.5]),
        ([0.25, 0.5, 0.75, 0.5], "saturation", 0.25, [0.5; 4]),
        ([1.7890625, -0.046875, 0.734375, 1.515625], "lightness", 1.0, [1.7890625, 1.7890625, 1.7890625, 1.515625]),
    ] {
        let inputs = [Some(Image::filled(64, 64, Kind::Color, source))];
        let image = cook("hsl", BTreeMap::from([(slider.into(), Value::Float(value))]), &inputs);
        assert_eq!(image.get(0, 0, false), expected);
    }
}

#[test]
fn hsl_wraps_the_hue_before_reconstructing_rgb() {
    let low = 1.0 / 8192.0;
    let high = 1.0 - low;
    let bias = 3.0 / 4096.0;
    for (color, hue, rgb) in [
        ([0.5; 3], 1.0, [1.0, f32::EPSILON, 0.0]),
        ([low, high - 0.5 - bias, high], 1.0 / 3.0, [0.5, 1.0, 0.0]),
        ([low, high - bias, high], 0.375_f32.next_up(), [0.49999946, 1.0, 0.0]),
    ] {
        let [r, g, b] = color;
        let source = Image::filled(64, 64, Kind::Color, [r, g, b, 0.75]);
        let values = BTreeMap::from([("hue".into(), Value::Float(hue)), ("saturation".into(), Value::Float(1.0))]);
        let pixel = cook("hsl", values, &[Some(source)]).get(0, 0, false);
        assert_eq!(pixel, [rgb[0], rgb[1], rgb[2], 0.75], "hue {hue}");
    }
}

#[test]
fn hsl_rounds_saturation_after_adding_the_slider_shift() {
    let source = Image::filled(16, 16, Kind::Color, [0.6109035, 0.6431967, 0.35680333, 0.75]);
    let values = BTreeMap::from([("hue".into(), Value::Float(0.359375)), ("saturation".into(), Value::Float(0.8125))]);
    let pixel = cook("hsl", values, &[Some(source)]).get(0, 0, false);
    assert_eq!(pixel.map(f32::to_bits), [0x3f74_a640, 0x3d35_9c00, 0x3f12_0e45, 0x3f40_0000]);
}

#[test]
fn hsl_rounds_the_channel_bias_before_computing_hue() {
    for (source, rgb) in [
        ([0x3eff_a001, 0x3eff_a001, 0x3eff_a001, 0x3f80_0000], [0x3f7f_a001, 0x38ff_a001, 0]),
        ([0x3ee0_32ed, 0x3f10_f4b6, 0x3ede_1694, 0x3e8a_0000], [0x3d1d_60a0, 0x3f80_0000, 0]),
        ([0x3eff_ffff, 0x3eff_ffff, 0x3eff_ffff, 0x3f80_0000], [0x3f7f_ffff, 0, 0x38ff_ffff]),
        ([0x3eff_fffd, 0x3eff_fffd, 0x3eff_fffd, 0x3f80_0000], [0x3f7f_fffd, 0x38ff_fffd, 0]),
        ([0x3eff_fffc, 0x3eff_ffff, 0x3f00_0001, 0x3ea8_0000], [0x3f7f_ffff, 0x3b1f_dfff, 0]),
        ([0x3eff_fff9, 0x3eff_fff6, 0x3eff_fffb, 0x3ebc_0000], [0, 0x3f7f_fff8, 0x3abf_dffa]),
        ([0x3eff_ffff, 0x3f00_0001, 0x3eff_fffc, 0x3f31_0000], [0x3b1f_dfff, 0, 0x3f7f_ffff]),
    ] {
        let inputs = [Some(Image::filled(64, 64, Kind::Color, source.map(f32::from_bits)))];
        let image = cook("hsl", BTreeMap::from([("saturation".into(), Value::Float(1.0))]), &inputs);
        assert_eq!(image.get(0, 0, false).map(f32::to_bits), [rgb[0], rgb[1], rgb[2], source[3]]);
    }
}

#[test]
fn hsl_expands_chroma_about_the_adjusted_lightness() {
    for (source, [hue, saturation, lightness], rgb) in [
        ([0x3dff_7fff, 0x3dff_7fff, 0x3dff_7fff, 0x3f80_0000], [0.5, 1.0, 0.5], [0x3e7f_8000, 0xb280_0000, 0x36ff_0001]),
        ([0x3cff_53d5, 0x3cff_53d5, 0x3cff_53d5, 0x3e44_0000], [0.5625, 0.75, 0.5625], [0x3e6f_dfb9, 0x3e47_e56a, 0x3d9f_ea7a]),
        ([0x3e3f_ea73, 0x3e3f_ea76, 0x3e3f_ea6c, 0x3f3f_0000], [0.390625, 0.6875, 0.46875], [0x3d9f_e508, 0x3e2f_e25e, 0x3e12_18d9]),
    ] {
        let inputs = [Some(Image::filled(64, 64, Kind::Color, source.map(f32::from_bits)))];
        let values =
            BTreeMap::from([("hue".into(), Value::Float(hue)), ("saturation".into(), Value::Float(saturation)), ("lightness".into(), Value::Float(lightness))]);
        let image = cook("hsl", values, &inputs);
        assert_eq!(image.get(0, 0, false).map(f32::to_bits), [rgb[0], rgb[1], rgb[2], source[3]]);
    }
}

#[test]
fn repeated_quadrants_have_distinct_reproducible_random_values() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let child = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children = vec![Some(child); 4];
    graph.nodes[1].parameters.insert("patterntype".into(), Value::Int(2));
    let mut function = FunctionGraph::default();
    let one = function.add("const_float1", &[], Some(Value::Float(1.0)));
    function.output = Some(function.add("rand", &[one], None));
    graph.nodes[1].parameters.insert("opacity".into(), Value::Function(Box::new(function)));
    let values = BTreeMap::from([("pattern".into(), Value::PatternGraph(Box::new(graph)))]);
    let image = cook("fx_map", values.clone(), &[]);
    let samples: Vec<_> = [(16, 16), (48, 16), (16, 48), (48, 48)].map(|(x, y)| image.get_gray(x, y, false)).into_iter().collect();

    assert!(samples.windows(2).all(|pair| pair[0] != pair[1]), "{samples:?}");
    assert_eq!(image.data(), cook("fx_map", values, &[]).data());
}

#[test]
fn brick_variation_bevels_both_axes_without_narrowing_the_plateau_to_a_strip() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters.insert("patterntype".into(), Value::Int(9));
    graph.nodes[0].parameters.insert("patternsuppl".into(), Value::Float(0.5));
    let image = cook("fx_map", BTreeMap::from([("pattern".into(), Value::PatternGraph(Box::new(graph)))]), &[]);
    let center = image.get_gray(32, 32, false);
    let horizontal_edge = image.get_gray(3, 32, false);
    let vertical_edge = image.get_gray(32, 3, false);

    assert_eq!(center, 1.0);
    assert!(horizontal_edge > 0.0 && horizontal_edge < center);
    assert!((horizontal_edge - vertical_edge).abs() < 1e-5);
}

#[test]
fn subpixel_quadrants_do_not_accumulate_at_shared_edges() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    for index in 0..7 {
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[index].children = vec![Some(child); 4];
    }
    let leaf = graph.nodes.last_mut().unwrap();
    leaf.parameters.insert("patterntype".into(), Value::Int(2));
    leaf.parameters.insert("opacity".into(), Value::Float(0.25));
    let image = cook("fx_map", BTreeMap::from([("pattern".into(), Value::PatternGraph(Box::new(graph)))]), &[]);

    assert!(image.data().iter().all(|value| (*value - 0.25).abs() < 1e-6), "mean {}", image.mean()[0]);
}

#[test]
fn quadrant_offsets_use_canvas_units_at_each_depth() {
    for (depth, origin, side) in [(1, [16_i64, 48], 8), (2, [8, 56], 4), (3, [4, 60], 2)] {
        for property in ["branchoffset", "frameoffset"] {
            let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
            for index in 0..depth {
                let child = graph.add(PatternNodeKind::Quadrant).unwrap();
                graph.nodes[index].children[2] = Some(child);
            }
            let leaf = &mut graph.nodes[depth].parameters;
            leaf.insert("patterntype".into(), Value::Int(2));
            leaf.insert("patternsize".into(), Value::Float2([0.25; 2]));
            leaf.insert(property.into(), Value::Float2([0.25, 0.125]));
            let image = cook("fx_map", BTreeMap::from([("pattern".into(), Value::PatternGraph(Box::new(graph)))]), &[]);
            let [cx, cy] = [origin[0] + 16, (origin[1] + 8) % 64];
            for y in 0..64 {
                for x in 0..64 {
                    let inside = (x - cx + side / 2).rem_euclid(64_i64) < side && (y - cy + side / 2).rem_euclid(64_i64) < side;
                    assert_eq!(image.get_gray(x, y, false), if inside { 1.0 } else { 0.0 }, "{property} at depth {depth}, ({x},{y})");
                }
            }
        }
    }
}

#[test]
fn gaussian_stamp_reaches_zero_at_the_edge_of_the_unit_disk() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters.insert("patterntype".into(), Value::Int(6));
    let image = cook("fx_map", BTreeMap::from([("pattern".into(), Value::PatternGraph(Box::new(graph)))]), &[]);
    assert_eq!(image.get_gray(0, 0, false), 0.0);
    for (x, expected) in [(0, 0.000006), (8, 0.002491), (16, 0.073742), (24, 0.542005), (31, 0.994580)] {
        let value = image.get_gray(x, 31, false);
        assert!((value - expected).abs() < 1.0e-6, "x={x}, {value} vs {expected}");
    }
}

#[test]
fn grayscale_max_patterns_do_not_accumulate_opacity() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let child = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children[1] = Some(child);
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(1));
    graph.nodes[1].parameters.insert("patterntype".into(), Value::Int(4));
    graph.nodes[1].parameters.insert("blendingmode".into(), Value::Int(1));
    let values = |graph| BTreeMap::from([("pattern".into(), Value::PatternGraph(Box::new(graph)))]);
    let single = cook("fx_map", values(graph.clone()), &[]);
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(2));
    let repeated = cook("fx_map", values(graph), &[]);

    assert!(single.data() == repeated.data(), "repeating a maximum blend must not brighten the image");
}
