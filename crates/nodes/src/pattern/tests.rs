use crate::{CookContext, find, resolve_params};
use materialcraft_core::{FunctionGraph, PatternGraph, PatternNodeKind, Value};
use std::collections::BTreeMap;

mod paired_batches;
mod repeated_quadrants;
mod sparse_quadrants;
mod terminal_quadrants;
mod underfilled_quadrants;

fn coverage(shape: i32, x: f32, y: f32, variation: f32) -> f32 {
    let sample = super::shape::prepare(shape, variation);
    sample(x, y).unwrap().map_or(0.0, |(base, offset)| base + offset)
}

#[test]
fn gaussian_stamps_stop_at_the_disc_boundary() {
    let x = 0.6;
    let inside = 0.79999;
    let outside = 0.80001;
    assert_eq!(coverage(3, x, inside, 0.0), 1.0);
    assert!(coverage(6, x, inside, 0.0) > 0.0);
    assert_eq!(coverage(3, x, outside, 0.0), 0.0);
    assert_eq!(coverage(6, x, outside, 0.0), 0.0);
}

#[test]
fn radial_stamps_keep_the_measured_center_and_skirt() {
    for (x, y, expected) in [
        (0xbf7c0000, 0x3c7fffbf, [0x3cfc0000, 0x35762800, 0x36cecb1c]),
        (0xbeb80001, 0xbef80001, [0x3f22e000, 0x3e27c967, 0x3c90589e]),
        (0x3c7fffbf, 0x3c7fffbf, [0x3f7fe000, 0x3f7f8018, 0x3f7e9cc6]),
    ] {
        for (shape, bits) in (4..=6).zip(expected) {
            let value = coverage(shape, f32::from_bits(x), f32::from_bits(y), 0.0);
            assert_eq!(value.to_bits(), bits, "shape {shape} at ({x:08x}, {y:08x})");
        }
    }
}

#[test]
fn bell_stamps_keep_signed_samples_until_blending() {
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    for (mode, expected) in [(0, [0xb2f2_8000, 0xb363_46e0]), (1, [0; 2])] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters =
            BTreeMap::from([("patterntype".into(), Value::Int(5)), ("blendingmode".into(), Value::Int(mode)), ("patternsize".into(), Value::Float2([1.0; 2]))]);
        let image = cook_at_size(graph, &variables, 316163, 64).unwrap().remove(0);
        let samples = [(10, 8), (54, 9)].map(|(x, y)| image.get_gray(x, y, false).to_bits());
        assert_eq!(samples, expected, "blend mode {mode}");
    }
}

#[test]
fn radial_stamp_draws_its_boundary_and_leaves_the_corners_of_its_quad_clear() {
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    for (shape, variation, corner, edge) in
        [(3, 0.25, -0.25, 1.0), (4, 0.25, -0.25, 0.0), (5, 0.25, -0.25, 0.0), (10, 1.0, 0.0, 0.0), (15, 1.0, 0.0, 0.0), (18, 0.25, -0.25, 0.25)]
    {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(shape)),
            ("patternsuppl".into(), Value::Float(variation)),
            ("blendingmode".into(), Value::Int(1)),
            ("frameoffset".into(), Value::Float2([1.0 / 128.0; 2])),
            ("patternsize".into(), Value::Float2([0.5; 2])),
        ]);
        let image = cook_in_regions(graph, &variables, 322224, [64; 2], [[0.0, 1.0, 0.0, 1.0]; 2], -0.25).unwrap().remove(0);
        assert_eq!([image.get_gray(20, 20, false), image.get_gray(48, 32, false)], [corner, edge], "shape {shape}");
    }
}

#[test]
fn beveled_square_multiplies_the_two_edge_profiles() {
    for (x, y, variation, expected) in [
        (-0.984375, 0.0, 0.0625, 0x3e200000),
        (-0.984375, -0.984375, 0.0625, 0x3cc80000),
        (-0.984375, -0.984375, 0.125, 0x3af20000),
        (-0.984375, -0.984375, -0.25, 0x3f800000),
    ] {
        assert_eq!(coverage(9, x, y, variation).to_bits(), expected);
    }
}

#[test]
fn beveled_square_uses_the_pixel_reciprocal_for_its_edge_width() {
    // Pixel (1, 31) in a centered 64 by 64 stamp lies 3/64 inside the left edge.
    for (width, expected) in [(0.072_187_5, 0x3f37_a51f), (0.718_97, 0x3c47_d97b)] {
        assert_eq!(coverage(9, -0.953125, -0.015625, width).to_bits(), expected);
    }
}

#[test]
fn horizontal_stamp_fades_use_the_pixel_reciprocal_for_the_remaining_width() {
    let coordinate = f32::from_bits(0x3f03_fffe);
    let variation = 86.0 / 1021.0;
    assert_eq!(coverage(12, coordinate, coordinate, variation).to_bits(), 0x3c9f_e0f0);
    assert_eq!(coverage(14, -0.265625, 0.109375, variation).to_bits(), 0x3e9b_3a66);
}

#[test]
fn gradient_variation_attenuates_the_horizontal_ramp() {
    for (variation, expected) in [(-0.25, 0.2578125), (0.0625, 0.241_699_22), (0.5, 0.12890625), (1.0, 0.0), (1.25, -0.064453125)] {
        assert_eq!(coverage(10, -0.484375, 0.75, variation), expected);
    }
}

#[test]
fn wave_frequency_and_radial_falloff_follow_stamp_coordinates() {
    let x = f32::from_bits(0x3d5fffef);
    let y = f32::from_bits(0xbe080003);
    for (variation, expected) in [(-0.25, 0x3f6a8659), (0.0, 0x3f6a8659), (0.1875, 0x3f4b2e24), (0.25, 0x3f3dd911), (1.0, 0x3d9ac833)] {
        assert_eq!(coverage(11, x, y, variation).to_bits(), expected);
    }
    assert_eq!(coverage(11, 0.0, 0.5, 0.25), 0.75);
}

#[test]
fn wave_profile_rounds_the_folded_phase_before_shaping_it() {
    for (x, y, variation, expected) in [
        (0xbecc_0001, 0x3bff_ff7f, 0.0, 0x3d9c_60ac),
        (0x3e17_fffb, 0x3bff_ff7f, 0.1875, 0x3de6_497a),
        (0xbebc_0001, 0x3bff_ff7f, 0.5, 0x3e3b_a2a0),
        (0x3ee3_fffd, 0xbc00_003f, 1.0, 0x3e3e_af6e),
    ] {
        assert_eq!(coverage(11, f32::from_bits(x), f32::from_bits(y), variation).to_bits(), expected);
    }
}

#[test]
fn wave_frequency_rounds_once_before_multiplying_the_coordinate() {
    let y = f32::from_bits(0x3bff_ff7f);
    for (x, variation, expected) in [(0x3d8f_fff7, 0.45037964, 0x3eb7_1c16), (0x3d1f_ffef, 0.44281647, 0x3f42_b347)] {
        assert_eq!(coverage(11, f32::from_bits(x), y, variation).to_bits(), expected);
    }
    assert_eq!(coverage(11, 0.0, 0.5, f32::MAX), 0.75);
}

#[test]
fn stamp_profiles_follow_the_radial_and_horizontal_falloffs() {
    // Two samples on the low-coverage skirt distinguish the signed quartic from a clamped falloff.
    for (shape, x, y, variation, bits) in [
        (7, 0xbf0c0000, 0xbd40000f, 0.0, 0x3d29a3c2),
        (12, 0xbeb80001, 0xbef80001, 0.25, 0x3e27c967),
        (12, 0x3ec7fffd, 0xbec80001, 0.25, 0x3ddfc34c),
        (12, 0x3f3bfffe, 0x3f2bfffe, 0.25, 0xadf38c4f),
        (13, 0xbf3c0000, 0x3f2bfffe, 0.25, 0xb1cb84a4),
        (13, 0x3ec7fffd, 0xbec80001, 0.25, 0x3db1415d),
        (14, 0xbeb80001, 0xbef80001, 0.0, 0x3f40da6b),
        (14, 0x3ec7fffd, 0xbec80001, 0.25, 0x3c774318),
        (15, 0x3e4ffffb, 0x3c7fffbf, 0.0, 0x3e2e002d),
        (15, 0x3e4ffffb, 0x3c7fffbf, 0.25, 0x3f7e2c09),
        (16, 0xbe880001, 0xbf740000, 0.0, 0x3c2ce9c0),
        (17, 0x3ec7fffd, 0xbec80001, 0.5, 0x3f990000),
        (18, 0x3ec7fffd, 0xbec80001, 0.25, 0x3f8ab222),
        (18, 0xbf7c0000, 0xbf7c0000, 0.25, 0x00000000),
    ] {
        let value = coverage(shape, f32::from_bits(x), f32::from_bits(y), variation);
        assert_eq!(value.to_bits(), bits, "shape {shape} at ({x:08x}, {y:08x})");
    }
}

fn cook(graph: PatternGraph) -> crate::CookResult {
    cook_with_variables(graph, &BTreeMap::new(), 7)
}

#[test]
fn stamp_tint_preserves_the_authored_type_and_scales_before_coverage() {
    let rgba = Value::Float4([0.9, 0.11, 0.57, 0.732]);
    let constant = |value| Value::Function(Box::new(FunctionGraph::constant(value)));
    for (shape, color, tint, gain, expected) in [
        (2, false, rgba.clone(), 1.0, [0x3f06_d3a0; 4]),
        (2, false, constant(rgba.clone()), 1.0, [0x3f3b_645a; 4]),
        (5, true, constant(Value::Float(0.732)), 0.6137, [0x3ee4_3699; 4]),
        (5, true, rgba.clone(), 0.6137, [0x3ecd_6456, 0x3d48_d3e4, 0x3e82_14e2, 0x3ee4_3699]),
        (17, true, constant(rgba), 327.125, [0x42de_3ea0, 0x4159_4e4d, 0x428c_c144, 0x42f6_f041]),
    ] {
        assert_stamp_pixel(shape, color, tint, gain, 0.0, expected);
    }
}

#[test]
fn pattern_opacity_flushes_subnormals_at_their_arithmetic_boundaries() {
    let normal = f32::MIN_POSITIVE;
    let tiny = f32::from_bits(0x0037_1245);
    let large = f32::from_bits(0x5d80_0000);
    for (tint, gain, background, expected) in [
        ([large, large, large, 0.25], normal, 0.0, [0; 4]),
        ([1.0, 1.0, 1.0, tiny], large, 0.0, [0; 4]),
        ([tiny, tiny, tiny, 1.0], large, 0.0, [0, 0, 0, large.to_bits()]),
        ([1.0, 1.0, 1.0, -1.0], f32::from_bits(0x0080_0001), normal, [0x8000_0000; 4]),
        ([1.0; 4], normal, 0.0, [normal.to_bits(); 4]),
    ] {
        assert_stamp_pixel(2, true, Value::Float4(tint), gain, background, expected);
    }
    for color in [false, true] {
        for shape in [0, 2] {
            assert_stamp_pixel(shape, color, Value::Float4([1.0; 4]), 0.0, -tiny, [0x8000_0000; 4]);
        }
    }
}

#[test]
fn solid_stamps_round_the_alpha_blend_at_accumulator_precision() {
    for (background, expected) in
        [(0x393f_a080, [0x3e81_77f4, 0x3e0f_efe8, 0x3eba_f7f4, 0x3ee6_0d32]), (0xbb89_bb22, [0x3e7e_7227, 0x3e0b_7227, 0x3eb8_b913, 0x3ee4_d090])]
    {
        assert_stamp_pixel(2, true, Value::Float4([0.5625, 0.3125, 0.8125, 0.71875]), 0.625, f32::from_bits(background), expected);
    }
}

fn stamp_on_background(graph: PatternGraph, color: bool, gain: f32, background: [f32; 4], seed: u32) -> craft_texture::Image {
    let definition = find("fx_map").unwrap();
    let params = resolve_params(
        definition,
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph))),
            ("roughness".into(), Value::Float(0.0)),
            ("color_mode".into(), Value::Bool(color)),
            ("opacity".into(), Value::Float(gain)),
            ("background".into(), Value::Color(background)),
        ]),
    );
    let context = CookContext {
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        w: 32,
        h: 32,
        seed,
        tile: true,
        inputs: &[],
        params: &params,
        resource: &|_| None,
        vars: &BTreeMap::new(),
    };
    (definition.cook)(&context).unwrap().remove(0)
}

fn assert_stamp_pixel(shape: i32, color: bool, tint: Value, gain: f32, background: f32, expected: [u32; 4]) {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([("patterntype".into(), Value::Int(shape)), ("opacity".into(), tint), ("blendingmode".into(), Value::Int(0))]);
    let image = stamp_on_background(graph, color, gain, [background; 4], 7);
    let pixel = image.get(16, 16, false).map(f32::to_bits);
    assert_eq!(&pixel[..image.channels()], &expected[..image.channels()], "shape {shape}, color {color}, opacity {gain}");
}

#[test]
fn quadrant_tint_blends_undefined_components_individually() {
    let mut tint = FunctionGraph::default();
    let depth = tint.add("get_float1", &[], Some(Value::Text("$depth".into())));
    let zero = tint.add("sub", &[depth, depth], None);
    let undefined = tint.add("div", &[zero, zero], None);
    let eighth = tint.add("const_float1", &[], Some(Value::Float(0.125)));
    let quarter = tint.add("const_float1", &[], Some(Value::Float(0.25)));
    let half = tint.add("const_float1", &[], Some(Value::Float(0.5)));
    let red_nan = tint.add("vector2", &[undefined, quarter], None);
    let blue_alpha = tint.add("vector2", &[eighth, half], None);
    let red_tint = tint.add("vector4", &[red_nan, blue_alpha], None);
    let red_green = tint.add("vector2", &[eighth, quarter], None);
    let alpha_nan = tint.add("vector2", &[eighth, undefined], None);
    let alpha_tint = tint.add("vector4", &[red_green, alpha_nan], None);

    let background = [0.25_f32, 0.375, 0.5, 0.75];
    for (shape, color, output, expected) in [
        (2, true, red_tint, [0x7fff_ffff, 0.5_f32.to_bits(), 0.5625_f32.to_bits(), 0.875_f32.to_bits()]),
        (2, true, alpha_tint, background.map(f32::to_bits)),
        (2, true, undefined, background.map(f32::to_bits)),
        (2, false, red_tint, [0.875_f32.to_bits(); 4]),
        (2, false, alpha_tint, [0.375_f32.to_bits(); 4]),
        (2, false, undefined, [0.375_f32.to_bits(); 4]),
        (17, true, alpha_tint, [0x7fff_ffff; 4]),
        (17, false, undefined, [0x7fff_ffff; 4]),
        (18, true, alpha_tint, background.map(f32::to_bits)),
    ] {
        tint.output = Some(output);
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(shape)),
            ("patternsuppl".into(), Value::Float(0.375)),
            ("opacity".into(), Value::Function(Box::new(tint.clone()))),
        ]);
        let image = stamp_on_background(graph, color, 1.0, background, 40991);
        let pixel = image.get(16, 16, false).map(f32::to_bits);
        assert_eq!(&pixel[..image.channels()], &expected[..image.channels()]);
    }
}

#[test]
fn quadrant_fields_reject_nonfinite_geometry_and_infinite_tints() {
    for (field, value) in [("frameoffset", Value::Float2([f32::NAN, 0.0])), ("opacity", Value::Float(f32::INFINITY))] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters =
            BTreeMap::from([("patterntype".into(), Value::Int(2)), (field.into(), Value::Function(Box::new(FunctionGraph::constant(value))))]);
        assert!(cook_with_variables(graph, &BTreeMap::new(), 40991).is_err());
    }
}

#[test]
fn zero_authored_stamp_opacity_is_distinct_from_zero_graph_opacity() {
    let linked = |value| Value::Function(Box::new(FunctionGraph::constant(Value::Float(value))));
    let tiny = f32::from_bits(1 << 19);
    for (shape, tint, gain, gray, red) in [
        (2, Value::Float4([0.0; 4]), 1.0, -0.25, -0.25),
        (2, Value::Float4([1.0; 4]), 0.0, 0.0, -0.25),
        (17, Value::Float4([0.0; 4]), 1.0, 0.25, 0.0625),
        (18, Value::Float4([0.0; 4]), 1.0, -0.25, -0.25),
        (18, Value::Float4([1.0; 4]), 0.0, 0.25, 0.0625),
        (18, linked(tiny), 1.0, -0.25, -0.25),
        (18, linked(-tiny), 1.0, -0.25, -0.25),
        (18, linked(f32::MIN_POSITIVE), 1.0, 0.25, 0.0625),
    ] {
        for (color, expected) in [(false, gray), (true, red)] {
            let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
            graph.nodes[0].parameters = BTreeMap::from([
                ("patterntype".into(), Value::Int(shape)),
                ("opacity".into(), tint.clone()),
                ("blendingmode".into(), Value::Int(1)),
                ("patternsuppl".into(), Value::Float(0.25)),
            ]);
            let image = stamp_on_background(graph, color, gain, [-0.25; 4], 328019);
            assert_eq!(image.get(16, 16, false)[0], expected, "shape {shape}, color {color}, gain {gain}");
        }
    }
}

#[test]
fn gradient_stamp_offsets_apply_to_the_tinted_channels() {
    let background = [0.5, 0.375, 0.75, 0.875];
    for (shape, variation, blend, background, position, expected) in [
        (17, 0.25, 0, [0.0; 4], 16, [0x3ebde000, 0x3e9ef000, 0x3ed28000, 0x3ed28000]),
        (17, -0.125, 0, [0.0; 4], 16, [0xbb880000, 0xbd844000, 0x3d140000, 0x3d140000]),
        (2, 0.0, 0, background, 16, [0x3f3c0000, 0x3efc0000, 0x3f880000, 0x3f6a0000]),
        (2, 0.0, 2, background, 16, [0x3f140000, 0x3ec00000, 0x3f540000, 0x3f6a0000]),
        (18, -0.125, 1, background, 0, background.map(f32::to_bits)),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(shape)),
            ("patternsuppl".into(), Value::Float(variation)),
            ("opacity".into(), Value::Float4([0.75, 0.375, 1.0, 0.625])),
            ("blendingmode".into(), Value::Int(blend)),
        ]);
        let image = stamp_on_background(graph, true, 0.5, background, 468);
        assert_eq!(image.get(position, position, false).map(f32::to_bits), expected);
    }
}

fn cook_with_variables(graph: PatternGraph, variables: &BTreeMap<String, Value>, seed: u32) -> crate::CookResult {
    cook_at_size(graph, variables, seed, 16)
}

fn cook_at_size(graph: PatternGraph, variables: &BTreeMap<String, Value>, seed: u32, side: u32) -> crate::CookResult {
    cook_at_dimensions(graph, variables, seed, side, side)
}

fn stamp_turn_input() -> Value {
    Value::Function(Box::new(FunctionGraph::input("turn", materialcraft_core::ValueType::Float)))
}

fn cook_at_dimensions(graph: PatternGraph, variables: &BTreeMap<String, Value>, seed: u32, width: u32, height: u32) -> crate::CookResult {
    cook_in_regions(graph, variables, seed, [width, height], [[0.0, 1.0, 0.0, 1.0]; 2], 0.0)
}

#[test]
fn grayscale_stamps_draw_in_shape_then_blend_order() {
    let tiny = 2.0_f32.powi(-27);
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    for (shapes, modes, tints, siblings, background, expected) in [
        ([2, 2, 2], [1, 0, 0], [0.75, 0.25, 0.5], true, 0.1875, [0.9375, 0.75]),
        ([2, 3, 2], [1, 0, 0], [0.875, -0.375, 0.5], false, 0.1875, [0.5, 0.875]),
        ([2, 3, 2], [0, 0, 0], [0.75, tiny, -0.75], false, 0.0, [tiny, 0.75]),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        let first = graph.add(PatternNodeKind::Quadrant).unwrap();
        let second = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].children[0] = Some(first);
        let parent = if siblings { 0 } else { 1 };
        graph.nodes[parent].children[1] = Some(second);
        for (i, (node, size)) in graph.nodes.iter_mut().zip([1.0, 2.0, 4.0]).enumerate() {
            let tint = tints[i];
            node.parameters = BTreeMap::from([
                ("patterntype".into(), Value::Int(shapes[i])),
                ("blendingmode".into(), Value::Int(modes[i])),
                ("opacity".into(), Value::Float4([tint, tint, tint, 1.0])),
                ("patternsize".into(), Value::Float2([size; 2])),
            ]);
        }
        let image = cook_in_regions(graph, &variables, 0, [32, 16], [[0.0, 1.0, 0.0, 1.0]; 2], background).unwrap().remove(0);
        assert_eq!([image.get_gray(0, 0, false), image.get_gray(31, 15, false)], expected, "shapes {shapes:?}, modes {modes:?}");
    }
}

fn three_bitmap_stamps(slots: [i32; 3], filters: [i32; 3], modes: [i32; 3], colors: [[f32; 4]; 3]) -> PatternGraph {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let first = graph.add(PatternNodeKind::Quadrant).unwrap();
    let second = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children[0] = Some(first);
    graph.nodes[1].children[1] = Some(second);
    for (i, (node, size)) in graph.nodes.iter_mut().zip([1.0, 2.0, 4.0]).enumerate() {
        node.parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(1)),
            ("imageindex".into(), Value::Int(slots[i])),
            ("imagefiltering".into(), Value::Int(filters[i])),
            ("blendingmode".into(), Value::Int(modes[i])),
            ("opacity".into(), Value::Float4(colors[i])),
            ("patternsize".into(), Value::Float2([size; 2])),
        ]);
    }
    graph
}

#[test]
fn grayscale_bitmap_stamps_group_repeated_slots_in_first_draw_order() {
    use craft_texture::{Image, Kind};
    use materialcraft_core::OutputFormat;

    let definition = find("fx_map").unwrap();
    let level = 128.0 / 255.0;
    let tiny = 2.0_f32.powi(-27);
    // The input bridges have already resized both constant fields to the graph dimensions.
    let inputs = [None, Some(Image::filled(32, 16, Kind::Gray, [level; 4])), Some(Image::filled(32, 16, Kind::Gray, [1.0; 4]))];
    for (slots, filters, modes, tints, expected) in [
        ([1, 0, 1], [2; 3], [0, 0, 0], [0.75, tiny, -0.75], [tiny * level, 0.75]),
        ([1, 0, 1], [2; 3], [1, 0, 1], [0.125, 0.25, 0.875], [0.875 + 0.25 * level, 0.125]),
        ([1, 1, 1], [2; 3], [1, 0, 1], [0.875, 0.25, 0.75], [0.875, 0.875]),
        ([1; 3], [0, 1, 0], [1, 0, 1], [0.75, 0.3125, 0.1875], [1.0625, 0.75]),
        ([1; 3], [1, 2, 1], [0; 3], [0.75, tiny, -0.75], [tiny, 0.75]),
        ([1; 3], [2, 1, 2], [0; 3], [0.75, tiny, -0.75], [0.0, 0.75]),
    ] {
        let graph = three_bitmap_stamps(slots, filters, modes, tints.map(|tint| [tint, tint, tint, 1.0]));
        let params = resolve_params(
            definition,
            &BTreeMap::from([
                ("pattern".into(), Value::PatternGraph(Box::new(graph))),
                ("roughness".into(), Value::Float(0.0)),
                ("color_mode".into(), Value::Bool(false)),
            ]),
        );
        let context = CookContext {
            w: 32,
            h: 16,
            seed: 471817,
            format: OutputFormat::F32,
            tile: false,
            input_formats: &[OutputFormat::U8; 3],
            inputs: &inputs,
            params: &params,
            resource: &|_| None,
            vars: &BTreeMap::from([("$tiling".into(), Value::Int(0))]),
        };
        let image = (definition.cook)(&context).unwrap().remove(0);
        assert_eq!([image.get_gray(0, 0, false), image.get_gray(31, 15, false)], expected);
    }
}

#[test]
fn bitmap_draw_order_uses_the_wired_slots_for_a_runtime_selector() {
    use craft_texture::{Image, Kind};
    use materialcraft_core::{OutputFormat, ValueType};

    let definition = find("fx_map").unwrap();
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0)), ("chosen".into(), Value::Int(0))]);
    for (kind, gray, alpha, fixed, runtime) in [
        (Kind::Gray, [0.125, 0.25, 0.875], [1.0; 3], 1.0004902_f32, 0.875_f32),
        (Kind::Color, [0.75, 0.25, 0.5], [0.375, 0.59375, 0.71875], 0.51298636, 0.451574),
    ] {
        let colors = std::array::from_fn(|i| [gray[i], gray[i], gray[i], alpha[i]]);
        let inputs = [
            None,
            Some(Image::filled(32, 16, kind, [128.0 / 255.0, 181.0 / 255.0, 79.0 / 255.0, 1.0])),
            Some(Image::filled(32, 16, kind, [1.0, 205.0 / 255.0, 111.0 / 255.0, 1.0])),
        ];
        for (selector, expected) in [(FunctionGraph::constant(Value::Int(0)), fixed), (FunctionGraph::input("chosen", ValueType::Integer), runtime)] {
            let mut graph = three_bitmap_stamps([1, 0, 1], [2; 3], [1, 0, 1], colors);
            graph.nodes[1].parameters.insert("imageindex".into(), Value::Function(Box::new(selector)));
            let params = resolve_params(
                definition,
                &BTreeMap::from([
                    ("pattern".into(), Value::PatternGraph(Box::new(graph))),
                    ("roughness".into(), Value::Float(0.0)),
                    ("color_mode".into(), Value::Bool(kind == Kind::Color)),
                ]),
            );
            let context = CookContext {
                w: 32,
                h: 16,
                seed: 471817,
                format: OutputFormat::F32,
                tile: false,
                input_formats: &[OutputFormat::U8; 3],
                inputs: &inputs,
                params: &params,
                resource: &|_| None,
                vars: &variables,
            };
            let image = (definition.cook)(&context).unwrap().remove(0);
            assert_eq!(image.get(0, 0, false)[0].to_bits(), expected.to_bits());
        }
    }
}

fn draw_on_fourth_repetition(graph: &mut PatternGraph) {
    // Only the fourth outer repetition submits a quadrant tree.
    let select = graph.add(PatternNodeKind::Switch).unwrap();
    let mut condition = FunctionGraph::default();
    let number = condition.add("get_float1", &[], Some(Value::Text("$number".into())));
    let three = condition.add_constant(Value::Float(3.0));
    condition.output = Some(condition.add("eq", &[number, three], None));
    graph.nodes[(select - 1) as usize].children[1] = graph.root;
    graph.nodes[(select - 1) as usize].parameters.insert("switch".into(), Value::Function(Box::new(condition)));
    let outer = graph.add(PatternNodeKind::Iterate).unwrap();
    graph.nodes[(outer - 1) as usize].children[1] = Some(select);
    graph.nodes[(outer - 1) as usize].parameters.insert("numberadded".into(), Value::Int(9));
    graph.root = Some(outer);
}

#[test]
fn a_single_quadrant_tree_retains_its_compositing_order_through_a_gated_repetition() {
    let small = f32::EPSILON * 0.5;
    for (depth, expected) in [(5, 0.0_f32), (6, small), (7, small)] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        for (slot, weight) in [1.0, small, -1.0, 0.0].into_iter().enumerate() {
            let mut leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
            graph.nodes[0].children[slot] = Some(leaf);
            for _ in 1..depth {
                let child = graph.add(PatternNodeKind::Quadrant).unwrap();
                graph.nodes[(leaf - 1) as usize].children.fill(Some(child));
                leaf = child;
            }
            // Center the first leaf of each subtree on the same pixel. All other leaves miss it.
            let first = [slot & 1, slot >> 1].map(|half| half as f32 * 0.5 + 0.5 / (1 << depth) as f32);
            graph.nodes[(leaf - 1) as usize].parameters = BTreeMap::from([
                ("patterntype".into(), Value::Int(4)),
                ("patternsize".into(), Value::Float2([0.75; 2])),
                ("frameoffset".into(), Value::Float2(first.map(|v| 15.5 / 32.0 - v))),
                ("opacity".into(), Value::Float(weight)),
                ("blendingmode".into(), Value::Int(0)),
            ]);
        }
        let image = cook_at_size(graph.clone(), &BTreeMap::new(), 377014, 32).unwrap().remove(0);
        assert_eq!(image.get_gray(15, 15, false).to_bits(), expected.to_bits(), "{depth} levels");

        draw_on_fourth_repetition(&mut graph);
        let image = cook_at_size(graph, &BTreeMap::new(), 306344, 32).unwrap().remove(0);
        assert_eq!(image.get_gray(15, 15, false).to_bits(), expected.to_bits(), "{depth} levels through a gated repetition");
    }
}

struct QuadrantProbeGroup {
    last_three: [usize; 3],
    offset: [f32; 2],
}

fn seven_level_tree() -> (PatternGraph, Vec<QuadrantProbeGroup>) {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let mut groups = Vec::new();
    for slot in 0..4 {
        let parent = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].children[slot] = Some(parent);
        for child_slot in 0..4 {
            let mut ancestor = graph.add(PatternNodeKind::Quadrant).unwrap();
            graph.nodes[(parent - 1) as usize].children[child_slot] = Some(ancestor);
            for _ in 2..5 {
                let child = graph.add(PatternNodeKind::Quadrant).unwrap();
                graph.nodes[(ancestor - 1) as usize].children.fill(Some(child));
                ancestor = child;
            }
            let terminal = graph.add(PatternNodeKind::Quadrant).unwrap();
            graph.nodes[(ancestor - 1) as usize].children.fill(Some(terminal));
            let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
            graph.nodes[(terminal - 1) as usize].children.fill(Some(leaf));
            let offset = [(slot & 1) * 2 + (child_slot & 1), (slot >> 1) * 2 + (child_slot >> 1)].map(|quarter| -0.25 * quarter as f32);
            groups.push(QuadrantProbeGroup { last_three: [ancestor, terminal, leaf].map(|id| (id - 1) as usize), offset });
        }
    }
    (graph, groups)
}

fn additive_square(weight: f32, offset: [f32; 2], frame: [f32; 2]) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("patterntype".into(), Value::Int(2)),
        ("patternsize".into(), Value::Float2([0.875; 2])),
        ("branchoffset".into(), Value::Float2(offset)),
        ("frameoffset".into(), Value::Float2(frame)),
        ("opacity".into(), Value::Float(weight)),
        ("blendingmode".into(), Value::Int(0)),
    ])
}

fn square_fork(parent_gain: f32) -> PatternGraph {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let mut parent = graph.root.unwrap();
    for depth in 1..=9 {
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        let slots = &mut graph.nodes[(parent - 1) as usize].children;
        if let Some(&slot) = [0, 3, 1, 0].get(depth - 1) {
            slots[slot] = Some(child);
        } else {
            slots.fill(Some(child));
        }
        if depth == 1 || depth == 9 {
            let (gain, size) = if depth == 1 { (parent_gain, 2.0) } else { (f32::EPSILON * 0.25, 1.375) };
            let mut parameters = additive_square(gain, [0.0; 2], [0.0; 2]);
            parameters.insert("patternsize".into(), Value::Float2([size; 2]));
            graph.nodes[(child - 1) as usize].parameters = parameters;
        }
        parent = child;
    }
    graph
}

#[test]
fn an_empty_later_quadrant_changes_when_its_sibling_draws() {
    let mut graph = square_fork(-1.0);
    let empty = graph.add(PatternNodeKind::Quadrant).unwrap();
    let active = graph.nodes[3].children[0];
    for (children, surviving_pixel) in [
        ([active, None, None, None], Some((6, 4))),
        ([active, Some(empty), None, None], None),
        ([active, active, None, None], Some((7, 4))),
        ([active, active, active, None], Some((6, 5))),
        ([active; 4], Some((7, 5))),
    ] {
        graph.nodes[3].children.copy_from_slice(&children);
        let image = cook_in_regions(graph.clone(), &BTreeMap::new(), 0, [16; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
        for y in 0..16 {
            for x in 0..16 {
                let value = if Some((x, y)) == surviving_pixel { f32::EPSILON } else { 0.0 };
                assert_eq!(image.get_gray(x, y, false).to_bits(), value.to_bits(), "pixel ({x}, {y}), children {children:?}");
            }
        }
    }
}

#[test]
fn two_repetitions_share_the_parent_and_last_child_draws() {
    let mut graph = square_fork(-0.5);
    let empty = graph.add(PatternNodeKind::Quadrant).unwrap();
    let iterate = graph.add(PatternNodeKind::Iterate).unwrap();
    graph.nodes[(iterate - 1) as usize].children[1] = graph.root;
    graph.nodes[(iterate - 1) as usize].parameters.insert("numberadded".into(), Value::Int(2));
    graph.root = Some(iterate);
    let active = graph.nodes[3].children[0];
    for (sibling, expected) in [(None, [f32::EPSILON, 0.0]), (Some(empty), [0.0; 2]), (active, [0.0, f32::EPSILON])] {
        graph.nodes[3].children[1] = sibling;
        let image = cook_in_regions(graph.clone(), &BTreeMap::new(), 0, [16; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
        assert_eq!([6, 7].map(|x| image.get_gray(x, 4, false).to_bits()), expected.map(f32::to_bits), "sibling {sibling:?}");
    }
}

#[test]
fn seven_quadrant_levels_order_leaf_runs_before_discarding_missed_stamps() {
    let tiny = f32::EPSILON * 0.25;
    for (negative, positive, miss_other_stamps, missing_slot, expected) in
        [(12, 0, false, None, tiny), (12, 0, true, None, tiny), (0, 14, true, None, 0.0), (12, 0, false, Some(2), tiny), (0, 14, true, Some(2), 0.0)]
    {
        let (mut graph, groups) = seven_level_tree();
        for (group, QuadrantProbeGroup { last_three: [_, terminal, leaf], offset }) in groups.into_iter().enumerate() {
            if let Some(slot) = missing_slot {
                graph.nodes[terminal].children[slot] = None;
            }
            let weight = if group == negative {
                -1.0
            } else if group == positive {
                tiny
            } else {
                0.0
            };
            // Move neutral leaves halfway between samples to exercise geometric culling.
            let shift = if miss_other_stamps && weight == 0.0 { 0.5 / 128.0 } else { 0.0 };
            graph.nodes[leaf].parameters = additive_square(weight, offset, [shift; 2]);
        }
        for detached_dot in [false, true] {
            if detached_dot {
                graph.add(PatternNodeKind::Dot).unwrap();
            }
            let image = cook_in_regions(graph.clone(), &BTreeMap::new(), 136404, [128; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
            for y in 0..128 {
                for x in 0..128 {
                    let slot = ((x & 1) + 2 * (y & 1)) as usize;
                    let value = if x < 32 && y < 32 && missing_slot != Some(slot) { expected } else { 1.0 };
                    assert_eq!(
                        image.get_gray(x, y, false).to_bits(),
                        value.to_bits(),
                        "groups {negative} and {positive} at ({x},{y}), missing {missing_slot:?}, detached {detached_dot}"
                    );
                }
            }
        }
    }
}

#[test]
fn depth_six_stamps_and_their_four_leaves_draw_in_the_same_grandchild_run() {
    let tiny = f32::EPSILON * 0.25;
    let (mut graph, groups) = seven_level_tree();
    for (group, QuadrantProbeGroup { last_three: [_, terminal, leaf], offset }) in groups.into_iter().enumerate() {
        let weight = match group {
            12 => -1.0,
            0 => tiny,
            _ => 0.0,
        };
        graph.nodes[terminal].parameters = additive_square(weight, offset, [0.0; 2]);
        graph.nodes[leaf].parameters = additive_square(0.25, [0.0; 2], [0.0, 0.5]);
    }
    let image = cook_in_regions(graph, &BTreeMap::new(), 137143, [128; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
    for y in 0..32 {
        for x in 0..32 {
            assert_eq!(image.get_gray(x, y, false).to_bits(), tiny.to_bits(), "overlapping parents at ({x},{y})");
            assert_eq!(image.get_gray(x, y + 64, false), 5.0, "overlapping leaves at ({x},{y})");
        }
    }
}

fn gated_mixed_depth_tree(grandchildren: bool) -> PatternGraph {
    let (mut graph, groups) = seven_level_tree();
    let middle = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[(middle - 1) as usize].parameters = additive_square(0.25, [0.0; 2], [0.0, 0.5]);
    if grandchildren {
        let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[(leaf - 1) as usize].parameters = additive_square(0.25, [0.0; 2], [0.0, 0.5]);
        graph.nodes[(middle - 1) as usize].children.fill(Some(leaf));
    }
    let tiny = f32::EPSILON * 0.25;
    for (group, QuadrantProbeGroup { last_three: [_, prior, parent], offset }) in groups.into_iter().enumerate() {
        let (weight, frame) = match group {
            12 if grandchildren => (-1.0, [0.0; 2]),
            5 if !grandchildren => (-1.0, [0.0; 2]),
            0 => (tiny, [0.0; 2]),
            _ => (0.25, [0.5, 0.0]),
        };
        graph.nodes[parent].parameters = additive_square(weight, offset, frame);
        graph.nodes[parent].children.fill(Some(middle));
        let pass = graph.add(PatternNodeKind::Iterate).unwrap();
        graph.nodes[(pass - 1) as usize].children[1] = Some(graph.nodes[parent].id);
        graph.nodes[(pass - 1) as usize].parameters.insert("numberadded".into(), Value::Int(1));
        graph.nodes[prior].children.fill(Some(pass));
    }

    draw_on_fourth_repetition(&mut graph);
    graph
}

#[test]
fn single_branch_controls_keep_the_order_of_a_complete_nine_level_tree() {
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    let image = cook_in_regions(gated_mixed_depth_tree(true), &variables, 188119, [128; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
    let samples = [(0, 0), (8, 0), (0, 16), (16, 0)].map(|(x, y)| image.get_gray(x, y, false));
    let tiny = f32::EPSILON * 0.25;
    assert_eq!(samples, [0.0, tiny, tiny, 0.0], "one active tree through both iterators");
}

#[test]
fn a_larger_tiled_window_keeps_the_draw_order_when_each_stamp_has_one_copy() {
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(3))]);
    let window = [-0.5, 1.5, -0.5, 1.5];
    let image = cook_in_regions(gated_mixed_depth_tree(true), &variables, 188119, [256; 2], [window; 2], 1.0).unwrap().remove(0);
    let samples = [(64, 64), (72, 64), (64, 80), (80, 64)].map(|(x, y)| image.get_gray(x, y, false));
    let tiny = f32::EPSILON * 0.25;
    assert_eq!(samples, [0.0, tiny, tiny, 0.0]);
}

fn draw_white_image(mut graph: PatternGraph, filter: i32, axes: i32) -> craft_texture::Image {
    use craft_texture::Image;

    for node in &mut graph.nodes {
        if node.parameters.get("patterntype").and_then(Value::as_i32) == Some(2) {
            node.parameters.insert("patterntype".into(), Value::Int(1));
            node.parameters.insert("imagefiltering".into(), Value::Int(filter));
        }
    }
    cook_probe_on_white(graph, axes, &[None, Some(Image::gray(8, 8, |_, _| 1.0))])
}

fn cook_probe_on_white(graph: PatternGraph, axes: i32, inputs: &[Option<craft_texture::Image>]) -> craft_texture::Image {
    use materialcraft_core::OutputFormat;

    let definition = find("fx_map").unwrap();
    let params = resolve_params(
        definition,
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph))),
            ("roughness".into(), Value::Float(0.0)),
            ("color_mode".into(), Value::Bool(false)),
            ("background".into(), Value::Color([1.0; 4])),
        ]),
    );
    let context = CookContext {
        w: 128,
        h: 128,
        seed: 188119,
        format: OutputFormat::F32,
        tile: axes != 0,
        input_formats: &[OutputFormat::F32; 2],
        inputs,
        params: &params,
        resource: &|_| None,
        vars: &BTreeMap::from([("$tiling".into(), Value::Int(axes))]),
    };
    (definition.cook)(&context).unwrap().remove(0)
}

#[test]
fn fixed_image_stamps_keep_the_subtree_order_of_an_untiled_quadrant_tree() {
    let tiny = f32::EPSILON * 0.25;
    for filter in [2, 1, 0] {
        let image = draw_white_image(gated_mixed_depth_tree(true), filter, 0);
        let samples = [(0, 0), (8, 0), (0, 16), (16, 0)].map(|(x, y)| image.get_gray(x, y, false));
        assert_eq!(samples, [0.0, tiny, tiny, 0.0], "filter {filter}");
    }
}

fn two_tree_parent_probe(negative: (i32, [i32; 2]), small: (i32, [i32; 2]), wrap_leaves: bool) -> PatternGraph {
    fn at_parent(f: &mut FunctionGraph, (iteration, pixel): (i32, [i32; 2])) -> u32 {
        let number = f.add("get_float1", &[], Some(Value::Text("$number".into())));
        let phase = f.add_constant(Value::Float(iteration as f32));
        let active = f.add("eq", &[number, phase], None);
        let pos = f.add("get_float2", &[], Some(Value::Text("$pos".into())));
        let center = f.add_constant(Value::Float2(pixel.map(|axis| (axis as f32 + 0.5) / 128.0)));
        let delta = f.add("sub", &[pos, center], None);
        let squared = f.add("dot", &[delta, delta], None);
        let zero = f.add_constant(Value::Float(0.0));
        let selected = f.add("eq", &[squared, zero], None);
        f.add("and", &[active, selected], None)
    }

    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(2));
    let mut parent = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children[1] = Some(parent);
    let tiny = f32::EPSILON * 0.25;
    for depth in 1..=9 {
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[(parent - 1) as usize].children.fill(Some(child));
        parent = child;
        if depth < 7 {
            continue;
        }
        let mut params = additive_square(0.25, [0.0; 2], [0.0; 2]);
        let mut frame = FunctionGraph::default();
        let pos = frame.add("get_float2", &[], Some(Value::Text("$pos".into())));
        let far = frame.add_constant(Value::Float2([0.875; 2]));
        let center = if depth == 7 {
            let a = at_parent(&mut frame, negative);
            let b = at_parent(&mut frame, small);
            let hit = frame.add("or", &[a, b], None);
            let readout = frame.add_constant(Value::Float2([32.5 / 128.0, 16.5 / 128.0]));
            let mut tint = FunctionGraph::default();
            let selected = at_parent(&mut tint, negative);
            let minus_one = tint.add_constant(Value::Float(-1.0));
            let increment = tint.add_constant(Value::Float(tiny));
            tint.output = Some(tint.add("ifelse", &[selected, minus_one, increment], None));
            params.insert("opacity".into(), Value::Function(Box::new(tint)));
            frame.add("ifelse", &[hit, readout, far], None)
        } else if wrap_leaves && depth == 9 {
            // Eight by eight leaves in the first tree produce one extra tile copy each.
            let number = frame.add("get_float1", &[], Some(Value::Text("$number".into())));
            let zero = frame.add_constant(Value::Float(0.0));
            let mut inside = frame.add("eq", &[number, zero], None);
            let limit = frame.add_constant(Value::Float(8.0 / 512.0));
            for axis in 0..2 {
                let component = frame.add("swizzle1", &[pos], Some(Value::Int(axis)));
                let below = frame.add("lr", &[component, limit], None);
                inside = frame.add("and", &[inside, below], None);
            }
            let edge = frame.add_constant(Value::Float2([0.0, 0.875]));
            frame.add("ifelse", &[inside, edge, far], None)
        } else {
            far
        };
        frame.output = Some(frame.add("sub", &[center, pos], None));
        params.insert("frameoffset".into(), Value::Function(Box::new(frame)));
        graph.nodes[(child - 1) as usize].parameters = params;
    }
    graph
}

#[test]
fn unfinished_quadrant_draws_can_follow_a_draw_from_the_next_tree() {
    let graph = two_tree_parent_probe((1, [118, 112]), (0, [79, 69]), false);
    let vars = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    let square = cook_in_regions(graph.clone(), &vars, 188119, [128; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
    let image = draw_white_image(graph, 2, 0);
    assert_eq!([square.get_gray(32, 16, false), image.get_gray(32, 16, false)], [f32::EPSILON * 0.25; 2], "order of overlapping parents");
}

#[test]
fn extra_copies_of_leaves_can_move_a_parent_past_another_subtree() {
    for (wrap_leaves, expected) in [(false, f32::EPSILON * 0.25), (true, 0.0)] {
        let graph = two_tree_parent_probe((0, [49, 29]), (0, [54, 50]), wrap_leaves);
        let image = draw_white_image(graph, 2, 3);
        assert_eq!(image.get_gray(32, 16, false), expected, "parent order with wrapping leaves {wrap_leaves}");
    }
}

fn two_tree_copy_field_readout(negative: [i32; 2], small: [i32; 2], copies_in_group: impl Fn(u32) -> u32 + Sync) -> f32 {
    use craft_texture::Image;

    // The two banks hold one texel per terminal quadrant. Move the selected
    // leaves to the tile boundary. All remaining leaves stay in the interior.
    let mut frame = FunctionGraph::default();
    let pos = frame.add("get_float2", &[], Some(Value::Text("$pos".into())));
    let number = frame.add("get_float1", &[], Some(Value::Text("$number".into())));
    let half = frame.add_constant(Value::Float(0.5));
    let bank = frame.add("mul", &[number, half], None);
    let zero = frame.add_constant(Value::Float(0.0));
    let origin = frame.add("vector2", &[bank, zero], None);
    let scale = frame.add_constant(Value::Float2([0.5, 1.0]));
    let scaled = frame.add("mul", &[pos, scale], None);
    let uv = frame.add("add", &[scaled, origin], None);
    let x = frame.add("samplelum", &[uv], Some(Value::Int2([0, 0])));
    let y = frame.add_constant(Value::Float(0.875));
    let center = frame.add("vector2", &[x, y], None);
    frame.output = Some(frame.add("sub", &[center, pos], None));

    let mut graph = two_tree_parent_probe((1, negative), (1, small), false);
    graph.nodes.last_mut().unwrap().parameters.insert("frameoffset".into(), Value::Function(Box::new(frame)));
    let field = Image::gray(1024, 512, |x, y| {
        let parent_x = x % 512 / 4;
        let parent_y = y / 4;
        let mut ordinal = (x / 512) * 16384;
        for bit in 0..7 {
            ordinal |= ((parent_x >> bit) & 1) << (2 * bit);
            ordinal |= ((parent_y >> bit) & 1) << (2 * bit + 1);
        }
        let copies = copies_in_group(ordinal / 64);
        let leaves = copies / 64 + u32::from(ordinal % 64 < copies % 64);
        if (y % 4) * 4 + x % 4 < leaves { 0.0 } else { 0.875 }
    });
    let image = cook_probe_on_white(graph.clone(), 3, &[None, Some(field)]);
    image.get_gray(32, 16, false)
}

#[test]
fn the_last_tile_copy_in_a_batch_moves_the_following_two_tree_draws() {
    // Parent 29,819 and parent 22,269 both belong to the second iteration.
    for (extra, expected) in [(167, f32::EPSILON * 0.25), (168, 0.0), (169, 0.0)] {
        let value = two_tree_copy_field_readout([109, 71], [111, 30], |group| match group {
            213 => 46,
            217 => 67,
            312 | 419 => 85,
            332 => extra,
            372 => 171,
            384 => 154,
            464 => 36,
            472 => 32,
            _ => 0,
        });
        assert_eq!(value, expected, "two-tree order with {extra} extra copies in subtree 332");
    }
}

#[test]
fn a_receiving_tile_batch_can_cross_the_middle_of_its_quadrant() {
    // Parents 28,996 and 24,640 exchange order when the next copy fills
    // the first half of the receiving 256-parent quadrant.
    // Parents 29,824 and 32,596 check the order once that half is full.
    for (extra, negative, small, expected) in [(167, [90, 64], [8, 64], f32::EPSILON * 0.25), (168, [90, 64], [8, 64], 0.0), (169, [96, 72], [126, 112], 0.0)] {
        let value = two_tree_copy_field_readout(negative, small, |group| match group {
            256 => 189,
            309 => 85,
            320 | 428 => 169,
            464 => extra,
            488 => 148,
            _ => 0,
        });
        assert_eq!(value, expected, "receiving quadrant with {extra} copies on its first subtree");
    }
}

#[test]
fn tiled_receiving_order_survives_an_earlier_cut_in_the_preceding_quadrant() {
    // Parents 32,139 and 29,680 overlap after the eight-batch cut moves
    // into the 64-parent subtree preceding parent 24,448.
    let value = two_tree_copy_field_readout([113, 107], [92, 92], |group| match group {
        236 => 76,
        256 => 156,
        308 | 352 => 85,
        320 => 169,
        439 => 132,
        480 => 37,
        488 => 105,
        _ => 0,
    });
    assert_eq!(value, f32::EPSILON * 0.25);
}

#[test]
fn tiled_receiving_order_defers_two_subtrees_until_the_batch_fits() {
    // At 83 the pending pair returns after parent 21,055. At 84 the
    // batch fits exactly, putting parent 31,935 before parent 24,452.
    for (extra, negative, small) in [(83, [71, 23], [48, 56]), (84, [103, 111], [114, 56]), (85, [103, 111], [114, 56])] {
        let value = two_tree_copy_field_readout(negative, small, |group| match group {
            234 => 70,
            260 => 141,
            320 => 191,
            440 => 145,
            450 => 62,
            480 => 24,
            488 => 126,
            497 => extra,
            _ => 0,
        });
        assert_eq!(value, f32::EPSILON * 0.25, "receiving batch with {extra} extra copies in subtree 497");
    }
}

#[test]
fn fixed_image_draws_follow_the_quadrant_runs_when_both_axes_tile() {
    for (grandchildren, axes) in [(false, 0), (false, 3), (true, 3)] {
        let mut graph = gated_mixed_depth_tree(grandchildren);
        // Place one row of child draws over the tile edge.
        for node in &mut graph.nodes {
            if node.parameters.get("frameoffset") == Some(&Value::Float2([0.0, 0.5])) {
                node.parameters.insert("frameoffset".into(), Value::Float2([-0.5 / 256.0, 0.5]));
            }
        }
        let vars = BTreeMap::from([("$tiling".into(), Value::Int(axes))]);
        let squares = cook_in_regions(graph.clone(), &vars, 188119, [128; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
        let image = draw_white_image(graph, 2, axes);
        assert!(squares.data().contains(&(f32::EPSILON * 0.25)));
        let exact = squares.data().iter().zip(image.data()).filter(|(a, b)| a.to_bits() == b.to_bits()).count();
        assert_eq!(exact, squares.data().len(), "matching fixed-image pixels, grandchildren {grandchildren}, tiling {axes}");
    }
}

#[test]
fn depth_eight_parent_order_counts_children_that_cross_the_tile_edge() {
    let tiny = f32::EPSILON * 0.25;
    let unit = [0.0, 1.0, 0.0, 1.0];
    for (child_count, tiling, child_x, width, early, extent, window, origin) in [
        (2, 0, 0.0, 0.875, 0..284, [128; 2], unit, [0; 2]),
        (2, 3, -1.0 / 512.0, 0.875, 0..274, [128; 2], unit, [0; 2]),
        (2, 3, 0.4995, 0.875, 0..252, [128; 2], unit, [0; 2]),
        (2, 3, -0.125, 4.75, 0..271, [128; 2], unit, [0; 2]),
        (2, 3, 0.4995, 4.75, 0..220, [128; 2], unit, [0; 2]),
        (2, 3, -1.0 / 512.0, 8.875, 0..252, [128; 2], unit, [0; 2]),
        (2, 3, 0.4995, 8.875, 0..188, [128; 2], unit, [0; 2]),
        (4, 0, 0.0, 0.875, 580..648, [128; 2], unit, [0; 2]),
        (4, 3, -1.0 / 512.0, 0.875, 571..644, [128; 2], unit, [0; 2]),
        (4, 3, 0.4995, 0.875, 574..623, [128; 2], unit, [0; 2]),
        (2, 3, 0.0, 0.875, 0..284, [256; 2], [-0.5, 1.5, -0.5, 1.5], [64; 2]),
        (2, 3, -1.0 / 512.0, 0.875, 0..274, [128, 256], [0.0, 1.0, -0.5, 1.5], [0, 64]),
        (2, 3, 0.4995, 0.875, 0..252, [128, 256], [0.0, 1.0, -0.5, 1.5], [0, 64]),
        (4, 3, 0.0, 0.875, 580..648, [256; 2], [-0.5, 1.5, -0.5, 1.5], [64; 2]),
        (4, 3, -1.0 / 512.0, 0.875, 571..644, [128, 256], [0.0, 1.0, -0.5, 1.5], [0, 64]),
        (4, 3, 0.4995, 0.875, 574..623, [128, 256], [0.0, 1.0, -0.5, 1.5], [0, 64]),
    ] {
        let variables = BTreeMap::from([("$tiling".into(), Value::Int(tiling))]);
        let (mut graph, groups) = seven_level_tree();
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[(child - 1) as usize].parameters = additive_square(0.25, [0.0; 2], [child_x, 0.5]);
        graph.nodes[(child - 1) as usize].parameters.insert("patternsize".into(), Value::Float2([width, 0.875]));
        for (group, QuadrantProbeGroup { last_three: [_, _, parent], mut offset }) in groups.into_iter().enumerate() {
            let weight = match group {
                0 => tiny,
                5 => -1.0,
                _ => {
                    // Keep the other parent draws active, outside the readout.
                    offset[0] += 0.5;
                    0.25
                }
            };
            graph.nodes[parent].parameters = additive_square(weight, offset, [0.0; 2]);
            for (slot, link) in graph.nodes[parent].children.iter_mut().enumerate() {
                *link = (child_count == 4 || slot % 2 == 0).then_some(child);
            }
        }
        let image = cook_in_regions(graph, &variables, 158729, extent, [window; 2], 1.0).unwrap().remove(0);
        for y in 0..32 {
            for x in 0..32 {
                let mut parent = 0;
                for bit in 0..5 {
                    parent |= (((x >> bit) & 1) | (((y >> bit) & 1) << 1)) << (2 * bit);
                }
                let negative_first = early.contains(&parent) || (child_count == 4 && parent >= 768);
                let expected = if negative_first { tiny } else { 0.0 };
                assert_eq!(
                    image.get_gray(origin[0] + x, origin[1] + y, false).to_bits(),
                    expected.to_bits(),
                    "{child_count} children, X offset {child_x}, window {window:?} at ({x},{y})"
                );
            }
        }
    }
}

#[test]
fn three_active_quadrant_levels_split_on_physical_tile_copies() {
    fn top_pair_frame(subdivisions: f32) -> Value {
        let mut frame = FunctionGraph::default();
        let pos = frame.add("get_float2", &[], Some(Value::Text("$pos".into())));
        let x = frame.add("swizzle1", &[pos], Some(Value::Int(0)));
        let y = frame.add("swizzle1", &[pos], Some(Value::Int(1)));
        let eighth = frame.add_constant(Value::Float(0.125));
        let left = frame.add("lr", &[x, eighth], None);
        let top = frame.add("lr", &[y, eighth], None);
        let edge = -0.5 / subdivisions;
        let first = frame.add_constant(Value::Float2([edge, 0.5]));
        let second = frame.add_constant(Value::Float2([edge - 0.125, 0.5]));
        let interior = frame.add_constant(Value::Float2([0.25, 0.5]));
        let seam = frame.add("ifelse", &[left, first, second], None);
        frame.output = Some(frame.add("ifelse", &[top, seam, interior], None));
        Value::Function(Box::new(frame))
    }

    let tiny = f32::EPSILON * 0.25;
    let interior = [0.25, 0.5];
    let horizontal_seam = [[-0.5 / 256.0, 0.5], [-0.5 / 512.0, 0.5]];
    for (tiling, negative, positive, frames, group_frames, first, second) in [
        (0, 12, 0, [[0.0, 0.5]; 2], None, 64..128, 512..1024),
        (0, 9, 11, [[0.0, 0.5], horizontal_seam[1]], None, 796..932, 0..0),
        (1, 9, 11, [[0.0, 0.5], horizontal_seam[1]], None, 784..919, 0..0),
        (2, 9, 11, [[0.0, 0.5], [0.5, -0.5 / 512.0]], None, 784..922, 0..0),
        // Two levels cross the seam in the first grandchild, filling a batch in an early subtree.
        (1, 12, 0, [interior; 2], Some((0, horizontal_seam.map(Value::Float2))), 64..128, 512..1024),
        // The first two subtrees of the final quadrant together submit 10,944 tile copies.
        (1, 12, 0, [interior; 2], Some((12, [256.0, 512.0].map(top_pair_frame))), 64..128, 512..1024),
    ] {
        let (mut graph, groups) = seven_level_tree();
        let mut add_descendants = |frames: [Value; 2]| {
            let middle = graph.add(PatternNodeKind::Quadrant).unwrap();
            let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
            for (id, frame) in [middle, leaf].into_iter().zip(frames) {
                let parameters = &mut graph.nodes[(id - 1) as usize].parameters;
                *parameters = additive_square(0.25, [0.0; 2], [0.0; 2]);
                parameters.insert("frameoffset".into(), frame);
            }
            graph.nodes[(middle - 1) as usize].children.fill(Some(leaf));
            middle
        };
        let middle = add_descendants(frames.map(Value::Float2));
        let selected = group_frames.map(|(group, frames)| (group, add_descendants(frames)));
        for (group, QuadrantProbeGroup { last_three: [_, _, parent], offset }) in groups.into_iter().enumerate() {
            let (gain, frame) = if group == negative {
                (-1.0, [0.0; 2])
            } else if group == positive {
                (tiny, [0.0; 2])
            } else {
                (0.25, [0.5, 0.0])
            };
            graph.nodes[parent].parameters = additive_square(gain, offset, frame);
            let child = selected.filter(|(index, _)| *index == group).map_or(middle, |(_, child)| child);
            graph.nodes[parent].children.fill(Some(child));
        }
        let variables = BTreeMap::from([("$tiling".into(), Value::Int(tiling))]);
        let image = cook_in_regions(graph, &variables, 188119, [128; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
        for y in 0..32 {
            for x in 0..32 {
                let mut parent = 0;
                for bit in 0..5 {
                    parent |= (((x >> bit) & 1) | (((y >> bit) & 1) << 1)) << (2 * bit);
                }
                let expected = if first.contains(&parent) || second.contains(&parent) { tiny } else { 0.0 };
                assert_eq!(image.get_gray(x, y, false), expected, "tiling {tiling}, groups {negative} and {positive}, parent {parent}");
            }
        }
    }
}

#[test]
fn wrapped_copies_of_one_stamp_can_draw_on_opposite_sides_of_another_stamp() {
    fn square(gain: f32, center: [f32; 2], size: f32) -> BTreeMap<String, Value> {
        let mut frame = FunctionGraph::default();
        let target = frame.add_constant(Value::Float2(center));
        let position = frame.add("get_float2", &[], Some(Value::Text("$pos".into())));
        frame.output = Some(frame.add("sub", &[target, position], None));
        let mut parameters = additive_square(gain, [0.0; 2], [0.0; 2]);
        parameters.insert("frameoffset".into(), Value::Function(Box::new(frame)));
        parameters.insert("patternsize".into(), Value::Float2([size; 2]));
        parameters
    }

    let mut tree = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let mut parent = tree.root.unwrap();
    for depth in 1..=8 {
        let child = tree.add(PatternNodeKind::Quadrant).unwrap();
        tree.nodes[(parent - 1) as usize].children.fill(Some(child));
        if depth >= 7 {
            tree.nodes[(child - 1) as usize].parameters = square(0.25, [0.5, 0.75], 0.875);
        }
        parent = child;
    }

    let tiny = f32::EPSILON * 0.25;
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(3))]);
    // Each parent draws once, followed by four children. Only the two selected squares cross the corner.
    for (negative, positive, early_copies) in [(15_699, 65_280, 1), (23_378, 75_520, 2), (2_897, 70_400, 3)] {
        let mut graph = tree.clone();
        for (draw, gain) in [(negative, -1.0), (positive, tiny)] {
            let parent_visit = draw / 5;
            let mut path: Vec<_> = (0..7).rev().map(|level| (parent_visit >> (level * 2)) & 3).collect();
            if draw % 5 != 0 {
                path.push(draw % 5 - 1);
            }
            let size = (1 << path.len()) as f32 / 16.0;
            let mut parent = graph.root.unwrap();
            for slot in path {
                let original = graph.nodes[(parent - 1) as usize].children[slot].unwrap();
                let node = graph.nodes[(original - 1) as usize].clone();
                let child = graph.add(PatternNodeKind::Quadrant).unwrap();
                graph.nodes[(child - 1) as usize] = materialcraft_core::PatternNode { id: child, ..node };
                graph.nodes[(parent - 1) as usize].children[slot] = Some(child);
                parent = child;
            }
            graph.nodes[(parent - 1) as usize].parameters = square(gain, [0.0; 2], size);
        }
        let image = cook_in_regions(graph, &variables, 173_519, [128, 64], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
        let corners = [(127, 63), (0, 63), (127, 0), (0, 0)];
        for (copy, (x, y)) in corners.into_iter().enumerate() {
            assert_eq!(image.get_gray(x, y, false), if copy < early_copies { tiny } else { 0.0 }, "draw {negative}, copy {copy}");
        }
    }
}

#[test]
fn missing_slots_at_the_last_two_subdivisions_keep_the_grandchild_boundaries() {
    let tiny = f32::EPSILON * 0.25;
    for (subdivision, mask) in [(5, 0b1011), (6, 0b1000), (6, 0b1010)] {
        let (mut graph, groups) = seven_level_tree();
        for (group, QuadrantProbeGroup { last_three, offset }) in groups.into_iter().enumerate() {
            for (slot, child) in graph.nodes[last_three[subdivision - 5]].children.iter_mut().enumerate() {
                if mask & (1 << slot) == 0 {
                    *child = None;
                }
            }
            let weight = match group {
                12 => -1.0,
                0 => tiny,
                _ => 0.0,
            };
            graph.nodes[last_three[2]].parameters = additive_square(weight, offset, [0.0; 2]);
        }
        let image = cook_in_regions(graph, &BTreeMap::new(), 136404, [128; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
        for y in 0..128 {
            for x in 0..128 {
                let place = 6 - subdivision;
                let slot = ((x >> place) & 1) + 2 * ((y >> place) & 1);
                let expected = if x < 32 && y < 32 && mask & (1 << slot) != 0 { tiny } else { 1.0 };
                assert_eq!(image.get_gray(x, y, false).to_bits(), expected.to_bits(), "subdivision {subdivision}, mask {mask} at ({x},{y})");
            }
        }
    }
}

#[test]
fn sparse_quadrant_streams_preserve_the_order_of_their_own_draws() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let weights = [-1.409_263_6, 0.452_831_5, f32::EPSILON * 0.5, 0.124_022_64_f32];
    for (group, slot) in [0, 2].into_iter().enumerate() {
        let parent = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].children[slot] = Some(parent);
        for subslot in 0..2 {
            let mut leaf = parent;
            for child_slot in [subslot, 0, 0, 0, 0] {
                let child = graph.add(PatternNodeKind::Quadrant).unwrap();
                graph.nodes[(leaf - 1) as usize].children[child_slot] = Some(child);
                leaf = child;
            }
            let center = [subslot as f32 * 0.25 + 0.5 / 64.0, group as f32 * 0.5 + 0.5 / 64.0];
            graph.nodes[(leaf - 1) as usize].parameters = BTreeMap::from([
                ("patterntype".into(), Value::Int(4)),
                ("patternsize".into(), Value::Float2([0.75; 2])),
                ("frameoffset".into(), Value::Float2(center.map(|v| 15.5 / 32.0 - v))),
                ("opacity".into(), Value::Float(weights[group * 2 + subslot])),
                ("blendingmode".into(), Value::Int(0)),
            ]);
        }
    }
    let image = cook_in_regions(graph, &BTreeMap::new(), 379595, [32; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 1.0).unwrap().remove(0);
    assert_eq!(image.get_gray(15, 15, false).to_bits(), 0x3e2b_9cd8);
}

fn cook_in_regions(
    graph: PatternGraph,
    variables: &BTreeMap<String, Value>,
    seed: u32,
    [width, height]: [u32; 2],
    [render, tiling]: [[f32; 4]; 2],
    background: f32,
) -> crate::CookResult {
    let definition = find("fx_map").unwrap();

    // These traversal fixtures measure unattenuated grayscale stamps
    let parameters = resolve_params(
        definition,
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph))),
            ("roughness".into(), Value::Float(0.0)),
            ("color_mode".into(), Value::Bool(false)),
            ("background".into(), Value::Color([background; 4])),
            ("render_region".into(), Value::Float4(render)),
            ("tiling_region".into(), Value::Float4(tiling)),
        ]),
    );
    let context = CookContext {
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        w: width,
        h: height,
        seed,
        tile: true,
        inputs: &[],
        params: &parameters,
        resource: &|_| None,
        vars: variables,
    };

    (definition.cook)(&context)
}

#[test]
fn stamp_gradients_keep_the_phase_of_extended_turns() {
    for (turns, x, y, bits) in [(1.5, 30, 6, 0x3f05_ffff), (1.75, 6, 30, 0x3f05_ffff), (-3.0, 19, 26, 0x3e9b_fffe)] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([("patterntype".into(), Value::Int(10)), ("patternrotation".into(), Value::Float(turns))]);
        let image = cook_at_size(graph, &BTreeMap::new(), 43, 64).unwrap().remove(0);
        assert_eq!(image.get_gray(x, y, false).to_bits(), bits, "{turns} turns at ({x},{y})");
    }
}

#[test]
fn rotated_stamps_use_the_pixel_center_when_snapping_the_near_vertex() {
    for (dimensions, center, size, turns, shape, pixel, expected) in [
        ([64, 32], [0.5009855, 0.012967646], [0.024458896, 0.015625], 136.94586, 10, [31, 0], 0x3dff_4c1c),
        ([32, 32], [0.06045103, 0.2529377], [0.055972442, 0.03125], 152.8708, 4, [1, 8], 0x3f0e_3e7c),
        ([64, 64], [-0.0138504505, 0.07075971], [0.0723335, 0.0625], 189.57106, 4, [0, 5], 0x3eeb_a3c2),
        ([256, 256], [-0.010898113, 0.7042948], [0.02902194, 0.03125], 325.4251, 10, [0, 179], 0x3d67_8fd8),
    ] {
        let variables = BTreeMap::from([("$tiling".into(), Value::Int(0)), ("turn".into(), Value::Float(turns))]);
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(shape)),
            ("branchoffset".into(), Value::Float2([-0.5; 2])),
            ("frameoffset".into(), Value::Float2(center)),
            ("patternsize".into(), Value::Float2(size)),
            ("patternrotation".into(), stamp_turn_input()),
        ]);
        let image = cook_at_dimensions(graph, &variables, 389119, dimensions[0], dimensions[1]).unwrap().remove(0);
        assert_eq!(image.get_gray(pixel[0], pixel[1], false).to_bits(), expected, "{center:?} at {dimensions:?}");
    }
}

#[test]
fn narrow_rotated_stamps_align_the_plane_origin_before_interpolation() {
    for (canvas, center, size, turns, shape, pixel, expected) in [
        ([32, 64], [0.59865606, 0.84110206], [0.004831543, 0.28550416], -0.13097546, 10, [18, 52], 0x3f43_ca7a),
        ([64, 64], [0.28058177, 0.822639], [0.006552735, 0.31079665], 1.4040037, 6, [18, 53], 0x3f25_4208),
        ([128, 64], [0.2890158, 0.7890536], [0.0022128907, 0.43932068], 0.8263619, 8, [29, 48], 0x3ec1_6c44),
        ([128, 128], [0.8203125, 0.8671875], [0.00625, 0.23773901], 1.489372, 10, [105, 116], 0x3eab_9835),
    ] {
        let variables = BTreeMap::from([("turn".into(), Value::Float(turns))]);
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(shape)),
            ("branchoffset".into(), Value::Float2([-0.5; 2])),
            ("frameoffset".into(), Value::Float2(center)),
            ("patternsize".into(), Value::Float2(size)),
            ("patternrotation".into(), stamp_turn_input()),
        ]);
        let image = cook_at_dimensions(graph, &variables, 0, canvas[0], canvas[1]).unwrap().remove(0);
        assert_eq!(image.get_gray(pixel[0], pixel[1], false).to_bits(), expected, "{canvas:?} at {pixel:?}");
    }
}

#[test]
fn stamp_projection_rounds_at_the_pixel_center_in_scaled_regions() {
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    for (span, center, size, dimensions, samples) in [
        ([7.0; 2], [3.5; 2], [7.0; 2], [64, 64], [(12, 0xbf1c_0001), (19, 0xbec8_0004)]),
        ([7.0; 2], [3.5; 2], [7.0; 2], [64, 32], [(0, 0xbf7c_0000), (63, 0x3f7b_fffc)]),
        ([7.0; 2], [3.5; 2], [7.0; 2], [32, 64], [(0, 0xbf78_0000), (31, 0x3f77_fffc)]),
        ([3.0; 2], [1.5 + 3.0 / 4096.0, 1.5], [3.0; 2], [32, 32], [(12, 0xbe60_8008), (19, 0x3e5f_7ff8)]),
        ([13.0; 2], [6.5; 2], [13.0; 2], [32, 64], [(12, 0xbe60_0008), (19, 0x3e5f_fff8)]),
        ([13.0, 1.0], [0.5; 2], [1.0; 2], [16, 32], [(0, 0xbe40_0004), (1, 0x0000_0000)]),
        ([15.0, 1.0], [0.5; 2], [1.0; 2], [16, 32], [(0, 0xbd80_0008), (1, 0x0000_0000)]),
        ([13.0, 1.0], [0.5; 2], [1.0; 2], [32, 64], [(0, 0xbf18_0001), (1, 0x3e5f_fff0)]),
        ([7.0, 1.0], [0.109_375_f32.next_down(), 0.5], [0.4375, 1.0], [32, 32], [(0, 0xb380_0000), (1, 0x3f7f_fffc)]),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(17)),
            ("patternsuppl".into(), Value::Float(-1.0)),
            ("blendingmode".into(), Value::Int(0)),
            ("opacity".into(), Value::Float(2.0)),
            ("branchoffset".into(), Value::Float2([-0.5; 2])),
            ("frameoffset".into(), Value::Float2(center)),
            ("patternsize".into(), Value::Float2(size)),
        ]);
        let region = [0.0, span[0], 0.0, span[1]];
        let image = cook_in_regions(graph, &variables, 170235, dimensions, [region; 2], 0.0).unwrap().remove(0);
        for (x, bits) in samples {
            assert_eq!(image.get_gray(x, 0, false).to_bits(), bits, "span {span:?}, column {x}");
        }
    }
}

#[test]
fn fading_stamp_uses_the_pixel_center_in_an_offset_rectangular_window() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(12)),
        ("patternsuppl".into(), Value::Float(0.78)),
        ("blendingmode".into(), Value::Int(1)),
        ("branchoffset".into(), Value::Float2([-0.5; 2])),
        ("frameoffset".into(), Value::Float2([0.6015625, 0.4375])),
        ("patternsize".into(), Value::Float2([1.8125, 0.96875])),
        ("patternrotation".into(), stamp_turn_input()),
    ]);
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0)), ("turn".into(), Value::Float(0.25))]);
    let region = [-1.0, 2.0, -2.5, 3.5];
    let image = cook_in_regions(graph, &variables, 290178, [16, 32], [region; 2], 0.0).unwrap().remove(0);
    assert_eq!(image.get_gray(9, 15, false).to_bits(), 0x3f0c_d037);
}

#[test]
fn clipped_square_stamp_triangles_use_the_far_vertex_for_both_slope_terms() {
    for (dimensions, center, size, turns, shape, pixel, expected) in [
        ([256, 256], [-0.08850241, 2.9797058], [0.06013941, 0.0625], 161.3187, 4, [236, 1], 0x3dec_9e10),
        ([128, 256], [-0.01251943, 0.8291415], [0.0390625, 0.0078125], 0.94446576, 8, [0, 211], 0x3e21_7c84),
    ] {
        let variables = BTreeMap::from([("turn".into(), Value::Float(turns))]);
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(shape)),
            ("branchoffset".into(), Value::Float2([-0.5; 2])),
            ("frameoffset".into(), Value::Float2(center)),
            ("patternsize".into(), Value::Float2(size)),
            ("patternrotation".into(), stamp_turn_input()),
        ]);
        let image = cook_at_dimensions(graph, &variables, 395323, dimensions[0], dimensions[1]).unwrap().remove(0);
        assert_eq!(image.get_gray(pixel[0], pixel[1], false).to_bits(), expected, "{dimensions:?} at {pixel:?}");
    }
}

#[test]
fn clipped_quarter_turn_aligns_the_axis_offset_before_adding_the_vertex() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(4)),
        ("branchoffset".into(), Value::Float2([-0.5; 2])),
        ("frameoffset".into(), Value::Float2([-0.8652914, -0.36846057])),
        ("patternsize".into(), Value::Float2([0.09375, 0.27446696])),
        ("patternrotation".into(), Value::Float(0.25)),
    ]);
    let image = cook_at_dimensions(graph, &BTreeMap::new(), 399341, 256, 512).unwrap().remove(0);
    assert_eq!(image.get_gray(18, 341, false).to_bits(), 0x3e62_3d20);
}

#[test]
fn clipped_rotated_ramp_aligns_the_plane_origin_before_product_normalization() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(10)),
        ("branchoffset".into(), Value::Float2([-0.5; 2])),
        ("frameoffset".into(), Value::Float2([0.113407135, 0.3964901])),
        ("patternsize".into(), Value::Float2([0.23880348, 0.90636885])),
        ("patternrotation".into(), Value::Float(0.94554406)),
    ]);
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    let image = cook_at_dimensions(graph, &variables, 396907, 128, 256).unwrap().remove(0);
    assert_eq!(image.get_gray(11, 163, false).to_bits(), 0x3d88_c460);
}

#[test]
fn quarter_turn_square_snaps_its_bounds_before_the_residual_phase_affects_sampling() {
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    for (turn, size, x) in [(1.5, [0.5175781, 0.27148438], 37), (-17.75, [0.27148438, 0.5175781], 27), (4096.25, [0.27148438, 0.5175781], 42)] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(2)),
            ("branchoffset".into(), Value::Float2([-0.5; 2])),
            ("frameoffset".into(), Value::Float2([0.5332031, 0.3779297])),
            ("patternsize".into(), Value::Float2(size)),
            ("patternrotation".into(), Value::Float(turn)),
        ]);
        let image = cook_at_size(graph, &variables, 323355, 64).unwrap().remove(0);
        assert_eq!(image.get_gray(x, 15, false), 0.0, "{turn} turns");
        assert_eq!(image.get_gray(x, 16, false), 1.0, "{turn} turns");
    }
}

#[test]
fn subpixel_axis_aligned_ramps_and_paraboloids_use_interpolated_coordinates() {
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    let unit = [0.0, 1.0, 0.0, 1.0];
    for (canvas, [x, y], center, size, seed, expected) in [
        ([64, 32], [37, 18], [0.586_013_8, 0.577_758_8], [1.0 / 2048.0, 1.0 / 512.0], 332208, [0x3eb0_0002, 0x3f43_0001]),
        ([16, 32], [6, 19], [0.405_578_6, 0.609_878_54], [-1.0 / 512.0, 1.0 / 512.0], 332239, [0x3e20_0004, 0x3e85_e008]),
    ] {
        for (shape, bits) in [10, 4].into_iter().zip(expected) {
            let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
            graph.nodes[0].parameters = BTreeMap::from([
                ("patterntype".into(), Value::Int(shape)),
                ("branchoffset".into(), Value::Float2([-0.5; 2])),
                ("frameoffset".into(), Value::Float2(center)),
                ("patternsize".into(), Value::Float2(size)),
            ]);
            let image = cook_in_regions(graph, &variables, seed, canvas, [unit; 2], 0.0).unwrap().remove(0);
            assert_eq!(image.get_gray(x, y, false).to_bits(), bits, "shape {shape} at {center:?}");
        }
    }
}

#[test]
fn subpixel_square_keeps_its_interpolated_edge_on_repeated_draws() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let child = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children[1] = Some(child);
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(2));
    graph.nodes[1].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(2)),
        ("blendingmode".into(), Value::Int(0)),
        ("branchoffset".into(), Value::Float2([-0.5; 2])),
        ("frameoffset".into(), Value::Float2([0.003_742_665_5, 0.523_437_5])),
        ("patternsize".into(), Value::Float2([0.008_139_668, 0.011_718_75])),
    ]);
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    let image = cook_at_size(graph, &variables, 307023, 64).unwrap().remove(0);
    assert_eq!(image.get_gray(0, 33, false), 2.0);
}

#[test]
fn oblique_stamp_interpolates_both_coordinates_of_a_paraboloid() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(4)),
        ("patternsize".into(), Value::Float2([0.25; 2])),
        ("patternrotation".into(), Value::Float(0.0625)),
    ]);
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    let image = cook_at_size(graph, &variables, 301067, 64).unwrap().remove(0);
    assert_eq!(image.get_gray(31, 24, false).to_bits(), 0x3df0_0010);
}

#[test]
fn rotated_stamp_uses_its_stored_bounds_in_a_displaced_render_window() {
    let unit = [0.0, 1.0, 0.0, 1.0];
    for (region, center, turn, shape, [x, y], bits) in [
        ([128.0, 129.0, 0.0, 1.0], [128.4375, 0.4375], 0.125, 10, [17, 27], 0x3c62_f8c0),
        ([1024.0, 1025.0, 1024.0, 1025.0], [1024.4375; 2], 0.0625, 4, [23, 21], 0x3cc0_e9a0),
    ] {
        let variables = BTreeMap::from([("$tiling".into(), Value::Int(0)), ("turn".into(), Value::Float(turn))]);
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(shape)),
            ("branchoffset".into(), Value::Float2([-0.5; 2])),
            ("frameoffset".into(), Value::Float2(center)),
            ("patternsize".into(), Value::Float2([0.25; 2])),
            ("patternrotation".into(), stamp_turn_input()),
        ]);
        let rendered = cook_in_regions(graph, &variables, 337435, [64, 64], [region, unit], 0.0).unwrap().remove(0);
        assert_eq!(rendered.get_gray(x, y, false).to_bits(), bits);
    }
}

#[test]
fn subpixel_rotated_stamp_interpolates_its_ramp_and_paraboloid() {
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0)), ("turn".into(), Value::Float(0.125))]);
    let unit = [0.0, 1.0, 0.0, 1.0];
    for (shape, bits) in [(10, 0x3f6b_7e10), (4, 0x3e44_b5b0)] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(shape)),
            ("branchoffset".into(), Value::Float2([-0.5; 2])),
            ("frameoffset".into(), Value::Float2([0.586_013_8, 0.577_758_8])),
            ("patternsize".into(), Value::Float2([1.0 / 2048.0, 1.0 / 512.0])),
            ("patternrotation".into(), stamp_turn_input()),
        ]);
        let rendered = cook_in_regions(graph, &variables, 332332, [64, 32], [unit; 2], 0.0).unwrap().remove(0);
        assert_eq!(rendered.get_gray(37, 18, false).to_bits(), bits);
    }
}

#[test]
fn sparse_quadrants_report_their_accumulated_child_positions() {
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    for (slot, center, axis, bits) in [
        (1, [-0.953_548_5, -0.769_640_9], 0, 0xbf54_1bc0),
        (2, [0.400_343_98, 0.941_799_6], 0, 0x3e8c_f9e4),
        (2, [0.111_769_885, -0.181_289_99], 1, 0xbd66_9050),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        let parent = graph.add(PatternNodeKind::Quadrant).unwrap();
        let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].parameters.insert("branchoffset".into(), Value::Float2([-0.25; 2]));
        graph.nodes[0].children[0] = Some(parent);
        graph.nodes[1].parameters.insert("branchoffset".into(), Value::Float2(center));
        graph.nodes[1].children[slot] = Some(leaf);

        let mut color = FunctionGraph::default();
        let position = color.add("get_float2", &[], Some(Value::Text("$pos".into())));
        color.output = Some(color.add("swizzle1", &[position], Some(Value::Int(axis))));
        graph.nodes[2].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(2)),
            ("patternsize".into(), Value::Float2([4.0; 2])),
            ("frameoffset".into(), Value::Float2(center.map(|v| 0.5 - v))),
            ("opacity".into(), Value::Function(Box::new(color))),
        ]);
        let image = cook_at_dimensions(graph, &variables, 365809, 16, 16).unwrap().remove(0);
        assert_eq!(image.get_gray(7, 7, false).to_bits(), bits);
    }
}

#[test]
fn rotated_paraboloid_interpolates_across_neighboring_pixel_pairs() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(4)),
        ("branchoffset".into(), Value::Float2([-0.5; 2])),
        ("frameoffset".into(), Value::Float2([0.046_721_816, 0.413_024_96])),
        ("patternsize".into(), Value::Float2([0.106_382_97; 2])),
        ("patternrotation".into(), stamp_turn_input()),
    ]);
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(3)), ("turn".into(), Value::Float(0.430_651_3))]);
    let image = cook_at_dimensions(graph, &variables, 350314, 64, 128).unwrap().remove(0);
    for (x, y, bits) in [(1, 48, 0x3ecb_3c42), (1, 52, 0x3f4e_334f), (5, 52, 0x3ee8_375a)] {
        assert_eq!(image.get_gray(x, y, false).to_bits(), bits);
    }
}

#[test]
fn shallow_rotated_ramp_stores_the_interpolated_coordinate_before_shading() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(10)),
        ("branchoffset".into(), Value::Float2([-0.5; 2])),
        ("frameoffset".into(), Value::Float2([0.415_625, 0.480_468_75])),
        ("patternsize".into(), Value::Float2([0.386_718_75, 0.25])),
        ("patternrotation".into(), Value::Float(0.000_273_437_5)),
    ]);
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    let image = cook_at_dimensions(graph, &variables, 360680, 32, 64).unwrap().remove(0);
    assert_eq!(image.get_gray(8, 31, false).to_bits(), 0x3de5_bb68);
}

#[test]
fn oblique_stamp_retains_the_small_cross_axis_step_on_a_thin_ramp() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([("patterntype".into(), Value::Int(10)), ("patternrotation".into(), Value::Float(1.5))]);
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    let image = cook_at_dimensions(graph, &variables, 189775, 256, 16).unwrap().remove(0);
    assert_eq!(image.get_gray(128, 6, false).to_bits(), 0x3eff_0000);
}

#[test]
fn fractional_stamp_edges_set_both_coordinate_gradients() {
    for (size, shift, turn, x, y, expected) in [
        ([0.375, 0.23046875], [0.0, 0.0], 0.0, 21, 32, 0x3d80_0004),
        ([0.375, 0.23828125], [0.0, 0.00390625], 0.0, 35, 32, 0x3f25_5554),
        ([0.625, 0.234375], [0.0078125, 0.0], 0.0, 37, 32, 0x3f1f_ffff),
        ([0.23828125, 0.375], [0.0, 0.001953125], 0.25, 20, 34, 0x3f27_de6c),
        ([0.2578125, 0.375], [0.0, 0.001953125], 0.25, 20, 33, 0x3f15_5554),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(10)),
            ("patternsize".into(), Value::Float2(size)),
            ("frameoffset".into(), Value::Float2(shift)),
            ("patternrotation".into(), Value::Float(turn)),
        ]);
        let image = cook_at_size(graph, &BTreeMap::new(), 428, 64).unwrap().remove(0);
        assert_eq!(image.get_gray(x, y, false).to_bits(), expected, "size {size:?}, turn {turn}, pixel ({x},{y})");
    }
}

#[test]
fn wrapped_beveled_stamp_retains_its_coordinate_at_the_crop() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(9)),
        ("patternsuppl".into(), Value::Float(0.7265625)),
        ("patternsize".into(), Value::Float2([0.984375, 0.484375])),
        ("frameoffset".into(), Value::Float2([0.5, 0.25])),
    ]);
    let image = cook_at_size(graph, &BTreeMap::new(), 261312, 128).unwrap().remove(0);
    assert_eq!(image.get_gray(48, 88, false).to_bits(), 0x3e73_2b04);
}

#[test]
fn clipped_stamp_gradients_use_one_triangle_vertex_for_both_axes() {
    let region = super::region::Region::new([0.0, 1.0, 0.0, 1.0]).unwrap();
    for (canvas, center, size, expected) in [
        ([128, 64], [-0.2734375, 0.3125], [0.734375_f32, 0.734375], 0x3f60_ae4e),
        ([128, 64], [-0.2734375, 0.3125], [-0.734375, 0.734375], 0x3dfa_8d8c),
        ([128, 128], [-0.3046875, -0.265625], [0.7578125, 0.6953125], 0x3f68_3f56),
    ] {
        let frame = super::footprint::Footprint { sine: 0.0, cosine: 1.0, radius: size.map(|s| s.abs() * 0.5) };
        let planes = super::coordinates::PixelPlanes::new(&frame, &region, center, size, canvas).unwrap();
        let [x, _] = planes.row(0)(0).unwrap();
        assert_eq!(((x + 1.0) * 0.5).to_bits(), expected, "center {center:?}, size {size:?}");
    }
}

#[test]
fn snapped_stamp_vertices_preserve_fractional_positions() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(10)),
        ("patternsize".into(), Value::Float2([28.625 / 64.0, 12.625 / 64.0])),
        ("frameoffset".into(), Value::Float2([-0.075, -0.125])),
    ]);
    let image = cook_at_size(graph, &BTreeMap::new(), 265259, 64).unwrap().remove(0);
    for (x, bits) in [(16, 0x3e01_3ace), (33, 0x3f38_57a4)] {
        assert_eq!(image.get_gray(x, 18, false).to_bits(), bits, "column {x}");
    }
}

#[test]
fn oblique_stamp_bounds_depend_on_whether_size_and_rotation_are_literal() {
    let size = Value::Float2([0.75, 1.5]);
    let turn = Value::Float(0.125);
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0)), ("size".into(), size.clone()), ("turn".into(), turn.clone())]);
    let link = |name, ty| Value::Function(Box::new(FunctionGraph::input(name, ty)));
    for (size, rotation, expected) in [
        (size.clone(), turn.clone(), 0x3f59_911e),
        (size.clone(), Value::Function(Box::new(FunctionGraph::constant(turn.clone()))), 0x3f59_911e),
        (link("size", materialcraft_core::ValueType::Float2), turn, 0x3f59_9122),
        (size, link("turn", materialcraft_core::ValueType::Float), 0x3f59_9122),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(17)),
            ("patternsize".into(), size),
            ("patternrotation".into(), rotation),
            ("frameoffset".into(), Value::Float2([0.0, -1.0])),
        ]);
        let image = cook_at_dimensions(graph, &variables, 422233, 128, 64).unwrap().remove(0);
        assert_eq!(image.get_gray(46, 0, false).to_bits(), expected);
    }
}

#[test]
fn stamp_precision_at_the_far_vertex_precedes_the_coordinate_scale() {
    let region = super::region::Region::new([0.0, 1.0, 0.0, 1.0]).unwrap();
    let size = [0.166_012_8_f32, 0.652_076_1];
    let center = [0.029_354_721, 0.792_399_17];
    for (cosine, expected) in [(1.0, 0x3ed5_a7fc), (-1.0, 0x3f15_2c02)] {
        let frame = super::footprint::Footprint { sine: 0.0, cosine, radius: size.map(|s| s * 0.5) };
        let planes = super::coordinates::PixelPlanes::new(&frame, &region, center, size, [32, 64]).unwrap();
        let [x, _] = planes.row(30)(0).unwrap();
        assert_eq!(((x + 1.0) * 0.5).to_bits(), expected, "cosine {cosine}");
    }
}

#[test]
fn stamp_vertices_snap_after_storage_at_pixel_precision() {
    let region = super::region::Region::new([0.0, 1.0, 0.0, 1.0]).unwrap();
    let size = [0.1875; 2];
    let frame = super::footprint::Footprint { sine: 0.0, cosine: 1.0, radius: [0.09375; 2] };
    let planes = super::coordinates::PixelPlanes::new(&frame, &region, [0.295_898_4, -0.031_547_427], size, [512, 128]).unwrap();
    let [x, y] = planes.row(0)(138).unwrap();
    assert_eq!((coverage(4, x, y, 0.0) * 0.91).to_bits(), 0x3f36_8e3f);
}

#[test]
fn shifted_region_preserves_the_vertices_before_projection() {
    let region = super::region::Region::new([0.0, 1.0, -0.5, 1.5]).unwrap();
    let size = [0.270_833_34, 0.162_500_01];
    let frame = super::footprint::Footprint { sine: 0.0, cosine: 1.0, radius: size.map(|s| s * 0.5) };
    let planes = super::coordinates::PixelPlanes::new(&frame, &region, [0.541_666_7, 0.437_499_97], size, [16, 32]).unwrap();
    let [u, v] = planes.row(14)(7).unwrap();
    assert_eq!(coverage(4, u, v, 0.0).to_bits(), 0x3f0f_e7d1);
}

#[test]
fn shifted_region_keeps_the_original_center_for_tiled_copies() {
    let shifted = [-0.5, 1.5, 0.0, 1.0];
    let fractional = [[0.0, 1.0, 0.0, 1.0], [-0.173, 0.369, 0.096, 0.869]];
    for (regions, canvas, center, size, [x, y], expected) in [
        ([shifted; 2], [32, 16], [-0.333_333_3, 0.637_500_05], [0.270_833_34, 0.162_5], [1, 9], 0x3e6c_4ec4),
        (fractional, [32, 64], [-3.631_460_7, -3.076_536_2], [0.416_212_65, 0.932_240_3], [0, 32], 0x3e16_8cbc),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(10)),
            ("blendingmode".into(), Value::Int(0)),
            ("branchoffset".into(), Value::Float2([-0.5; 2])),
            ("frameoffset".into(), Value::Float2(center)),
            ("patternsize".into(), Value::Float2(size)),
        ]);
        let image = cook_in_regions(graph, &BTreeMap::new(), 319091, canvas, regions, 0.0).unwrap().remove(0);
        assert_eq!(image.get_gray(x, y, false).to_bits(), expected, "center {center:?}");
    }
}

#[test]
fn shifted_region_stores_the_snap_offset_in_pattern_space() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(10)),
        ("blendingmode".into(), Value::Int(0)),
        ("branchoffset".into(), Value::Float2([-0.5; 2])),
        ("frameoffset".into(), Value::Float2([0.005_702_4, 0.984_657_3])),
        ("patternsize".into(), Value::Float2([0.203_478_44, 0.709_372_1])),
        ("patternrotation".into(), Value::Float(0.25)),
    ]);
    let regions = [[-0.173, 0.369, 0.096, 0.869], [0.019, 0.631, -0.073, 0.689]];
    let image = cook_in_regions(graph, &BTreeMap::new(), 319613, [32, 64], regions, 0.0).unwrap().remove(0);
    assert_eq!(image.get_gray(0, 2, false).to_bits(), 0x3cd4_7660);
}

#[test]
fn stamp_vertices_keep_the_last_sample_of_a_wrapped_square() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(2)),
        ("branchoffset".into(), Value::Float2([-0.5; 2])),
        ("frameoffset".into(), Value::Float2([0.958_333_3, 0.4375])),
        ("patternsize".into(), Value::Float2([0.270_833_34, 0.8125])),
    ]);
    let image = cook_at_dimensions(graph, &BTreeMap::new(), 311024, 16, 32).unwrap().remove(0);
    assert_eq!(image.get_gray(1, 1, false), 1.0);
}

#[test]
fn wrapped_centers_store_each_step_across_fractional_periods() {
    let region = super::region::Region::new([-0.173, 0.369, 0.096, 0.869]).unwrap();
    let copies = region.repeats(&region, [0.307_120_5, 0.5], [2.0, 0.0], [true, false]).unwrap();
    let leftmost = copies[0].centers().min_by(f32::total_cmp);
    assert_eq!(leftmost.map(f32::to_bits), Some(0xbfee_314e));
}

#[test]
fn tiled_ramp_retains_the_phase_of_copies_above_the_visible_window() {
    let tiling = [0.019, 0.631, -0.073, 0.689];
    for (axes, canvas, center, size, render, [x, y], expected) in [
        (3, [32, 64], [0.064_685_315, 0.205_253_44], [0.579_976_9, 0.400_788_1], [-0.173, 0.369, 0.096, 0.869], [29, 0], 0x3f73_a212),
        (1, [128, 32], [-0.935_683_2, 0.335_352_5], [33.782_4, 0.866_160_7], [-9.979, -9.558, 0.096, 0.869], [6, 0], 0x4160_7c0c),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(10)),
            ("blendingmode".into(), Value::Int(0)),
            ("branchoffset".into(), Value::Float2([-0.5; 2])),
            ("frameoffset".into(), Value::Float2(center)),
            ("patternsize".into(), Value::Float2(size)),
        ]);
        let variables = BTreeMap::from([("$tiling".into(), Value::Int(axes))]);
        let image = cook_in_regions(graph, &variables, 329457, canvas, [render, tiling], 0.0).unwrap().remove(0);
        assert_eq!(image.get_gray(x, y, false).to_bits(), expected, "render window {render:?}");
    }
}

#[test]
fn wrapped_additive_stamps_visit_the_outer_row_and_column_first() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(10)),
        ("blendingmode".into(), Value::Int(0)),
        ("branchoffset".into(), Value::Float2([-0.5; 2])),
        ("frameoffset".into(), Value::Float2([0.1875, 0.34375])),
        ("patternsize".into(), Value::Float2([2.75, 2.5])),
    ]);
    let image = cook_at_size(graph, &BTreeMap::new(), 311197, 32).unwrap().remove(0);
    assert_eq!(image.get_gray(3, 0, false).to_bits(), 0x4035_1747);
}

#[test]
fn a_wrapped_stamp_keeps_the_center_precision_of_its_outer_copy() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(10)),
        ("branchoffset".into(), Value::Float2([-0.5; 2])),
        ("frameoffset".into(), Value::Float2([0.063_709_915, 0.906_030_9])),
        ("patternsize".into(), Value::Float2([0.128_019_7, 0.062_836_37])),
    ]);
    let image = cook_at_size(graph, &BTreeMap::new(), 303021, 64).unwrap().remove(0);
    assert_eq!(image.get_gray(0, 56, false).to_bits(), 0x3d81_c730);
}

#[test]
fn snapped_stamp_vertices_clip_the_last_solid_sample() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(2)),
        ("patternsize".into(), Value::Float2([0.703125, 0.765625])),
        ("frameoffset".into(), Value::Float2([-0.796875, -0.8125])),
    ]);
    let image = cook_at_size(graph, &BTreeMap::new(), 270681, 64).unwrap().remove(0);
    assert_eq!(image.get_gray(0, 3, false), 1.0);
    assert_eq!(image.get_gray(0, 4, false), 0.0);
}

#[test]
fn gaussian_stamp_coordinates_follow_the_pixel_footprint() {
    let definition = find("fx_map").unwrap();
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(6)),
        ("blendingmode".into(), Value::Int(1)),
        ("patternsize".into(), Value::Float2([3.0, 1.5])),
        ("frameoffset".into(), Value::Float2([-0.25; 2])),
    ]);
    let params = resolve_params(
        definition,
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph))),
            ("color_mode".into(), Value::Bool(false)),
            ("roughness".into(), Value::Float(0.0)),
            ("opacity".into(), Value::Float(0.995)),
        ]),
    );
    for (w, h, x, y, expected) in [
        (16, 16, 7, 1, 0x3ef8_8006),
        (16, 32, 0, 14, 0x3eb2_37e3),
        (32, 32, 0, 14, 0x3eac_19c4),
        (8, 64, 0, 30, 0x3e9b_8a3c),
        (64, 128, 2, 62, 0x3e85_65a0),
        (128, 256, 0, 126, 0x3e69_16dd),
    ] {
        let context = CookContext {
            w,
            h,
            seed: 223,
            format: materialcraft_core::OutputFormat::F32,
            input_formats: &[],
            tile: false,
            inputs: &[],
            params: &params,
            resource: &|_| None,
            vars: &BTreeMap::new(),
        };
        let image = (definition.cook)(&context).unwrap().remove(0);
        assert_eq!(image.get_gray(x, y, false).to_bits(), expected, "{w}x{h} at ({x},{y})");
    }
}

#[test]
fn oblique_square_keeps_the_last_sample_inside_its_edge() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(2)),
        ("patternsize".into(), Value::Float2([0.755, 0.391])),
        ("patternrotation".into(), Value::Float(0.13125)),
        ("frameoffset".into(), Value::Float2([-0.1113, 0.1437])),
    ]);
    let image = cook_at_size(graph, &BTreeMap::new(), 1108, 512).unwrap().remove(0);
    assert_eq!(image.get_gray(307, 492, false), 1.0);
    assert_eq!(image.get_gray(308, 492, false), 0.0);
    assert_eq!(image.get_gray(307, 493, false), 0.0);
}

#[test]
fn quarter_turns_keep_stamp_edge_ownership_in_image_space() {
    // The sample centers on all four rectangle edges distinguish image-space ownership after rotation.
    let size = [0.625, 0.25];
    for (angle, size, first, last) in [
        (0.0, size, [13, 25], [52, 40]),
        (0.25, size, [25, 13], [40, 52]),
        (0.5, size, [13, 25], [52, 40]),
        (0.75, size, [25, 13], [40, 52]),
        (-0.25, size, [25, 13], [40, 52]),
        (1.25, size, [25, 13], [40, 52]),
        (0.0, [-0.625, 0.25], [13, 25], [52, 40]),
        (0.25, [-0.625, -0.25], [25, 13], [40, 52]),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(2)),
            ("patternsize".into(), Value::Float2(size)),
            ("patternrotation".into(), Value::Float(angle)),
            ("frameoffset".into(), Value::Float2([1.0 / 128.0; 2])),
        ]);
        let image = cook_at_size(graph, &BTreeMap::new(), 943, 64).unwrap().remove(0);
        for y in 0..64 {
            for x in 0..64 {
                let covered = (first[0]..=last[0]).contains(&x) && (first[1]..=last[1]).contains(&y);
                assert_eq!(image.get_gray(x, y, false), f32::from(covered), "turn {angle} at ({x},{y})");
            }
        }
    }
}

#[test]
fn a_sample_on_the_stored_lower_stamp_edge_is_excluded() {
    let size = [0.4, 0.3];
    let frame = super::footprint::Footprint { sine: 0.0, cosine: 1.0, radius: size.map(|v| v * 0.5) };
    let sample = [14.5 / 32.0, 9.5 / 16.0];
    for axis in 0..2 {
        let mut center = sample;
        center[axis] += frame.radius[axis];
        assert!(frame.local(sample, center, size).is_none());
        let mut inside = sample;
        inside[axis] += 1.0 / [32.0, 16.0][axis];
        assert!(frame.local(inside, center, size).is_some());
    }
}

#[test]
fn pattern_functions_keep_host_time_and_quadrant_depth() {
    for (name, expected) in [("$time", 0.75), ("$depth", 1.0), ("$depthpow2", 0.5)] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].parameters.insert("patterntype".into(), Value::Int(0));
        graph.nodes[0].children[0] = Some(child);
        graph.nodes[1].parameters.insert("patterntype".into(), Value::Int(2));
        graph.nodes[1].parameters.insert("patternsize".into(), Value::Float2([2.0; 2]));
        let mut function = FunctionGraph::default();
        function.output = Some(function.add("get_float1", &[], Some(Value::Text(name.into()))));
        graph.nodes[1].parameters.insert("opacity".into(), Value::Function(Box::new(function)));

        let image = cook_with_variables(graph, &BTreeMap::from([("$time".into(), Value::Float(0.75))]), 7).unwrap().remove(0);

        assert!((image.mean()[0] - expected).abs() < 1e-6, "{name}: {}", image.mean()[0]);
    }
}

#[test]
fn iterate_sets_number_and_switch_selects_one_child() {
    let mut graph = PatternGraph::default();
    let iterate = graph.add(PatternNodeKind::Iterate).unwrap();
    let switch = graph.add(PatternNodeKind::Switch).unwrap();
    let quadrant = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.root = Some(iterate);
    graph.nodes[0].children = vec![None, Some(switch)];
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(3));
    graph.nodes[1].children = vec![Some(quadrant), None];
    graph.nodes[2].parameters.insert("patterntype".into(), Value::Int(2));
    let mut function = FunctionGraph::default();
    function.output = Some(function.add("get_float1", &[], Some(Value::Text("$number".into()))));
    graph.nodes[2].parameters.insert("opacity".into(), Value::Function(Box::new(function)));
    let image = cook(graph.clone()).unwrap().remove(0);

    assert!((image.mean()[0] - 3.0).abs() < 1e-6);

    let dot = graph.add(PatternNodeKind::Dot).unwrap();
    graph.nodes[1].children[0] = Some(dot);
    graph.nodes[3].children[0] = Some(quadrant);

    assert_eq!(cook(graph.clone()).unwrap()[0].data(), image.data());

    graph.nodes[1].parameters.insert("switch".into(), Value::Bool(true));

    assert_eq!(cook(graph).unwrap()[0].mean()[0], 0.0);
}

#[test]
fn an_iterators_seed_settings_apply_to_its_repeated_branch() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let initial = graph.add(PatternNodeKind::Quadrant).unwrap();
    let repeated = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children = vec![Some(initial), Some(repeated)];
    graph.nodes[0].parameters.insert("randomseed".into(), Value::Int(17));
    let mut tint = FunctionGraph::default();
    let half = tint.add_constant(Value::Float(0.5));
    tint.output = Some(tint.add("rand", &[half], None));
    for (square, x) in graph.nodes[1..].iter_mut().zip([-0.25, 0.25]) {
        square.parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(2)),
            ("patternsize".into(), Value::Float2([0.5, 1.0])),
            ("frameoffset".into(), Value::Float2([x, 0.0])),
            ("opacity".into(), Value::Function(Box::new(tint.clone()))),
        ]);
    }
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    for (inherit, seed, left, right) in [(true, 42, 0x3e2a_2edc, 0x3e37_d570), (false, 42, 0x3e2a_2edc, 0x3ec2_59fa), (false, 329779, 0x3f14_0041, 0x3ec2_59fa)]
    {
        graph.nodes[0].parameters.insert("randominherited".into(), Value::Bool(inherit));
        let image = cook_in_regions(graph.clone(), &variables, seed, [16; 2], [[0.0, 1.0, 0.0, 1.0]; 2], 0.125).unwrap().remove(0);
        for y in 0..16 {
            for x in 0..16 {
                assert_eq!(image.get_gray(x, y, false).to_bits(), if x < 8 { left } else { right });
            }
        }
    }
}

#[test]
fn a_nested_iterators_initial_branch_inherits_the_callers_number() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let inner = graph.add(PatternNodeKind::Iterate).unwrap();
    let initial = graph.add(PatternNodeKind::Quadrant).unwrap();
    let repeated = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children[1] = Some(inner);
    graph.nodes[1].children = vec![Some(initial), Some(repeated)];

    let mut gain = FunctionGraph::default();
    let number = gain.add("get_float1", &[], Some(Value::Text("$number".into())));
    let weight = gain.add_constant(Value::Float(1.0 / 32.0));
    gain.output = Some(gain.add("mul", &[number, weight], None));
    for (square, offset) in graph.nodes[2..].iter_mut().zip([-0.25, 0.25]) {
        square.parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(2)),
            ("blendingmode".into(), Value::Int(0)),
            ("patternsize".into(), Value::Float2([0.5, 1.0])),
            ("frameoffset".into(), Value::Float2([offset, 0.0])),
            ("opacity".into(), Value::Function(Box::new(gain.clone()))),
        ]);
    }

    // The left half records the initial branch. The right half records the inner repetitions.
    let variables = BTreeMap::from([("$tiling".into(), Value::Int(0))]);
    for (outer, inner, seed, width, height, left, right) in [
        (3, 1, 283271, 16, 16, 0.09375, 0.0),
        (4, 3, 283328, 8, 8, 0.1875, 0.375),
        (2, 0, 283347, 16, 8, 0.03125, 0.0),
        (5, 2, 283404, 32, 64, 0.3125, 0.15625),
    ] {
        graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(outer));
        graph.nodes[1].parameters.insert("numberadded".into(), Value::Int(inner));
        let image = cook_at_dimensions(graph.clone(), &variables, seed, width, height).unwrap().remove(0);
        for row in image.data().chunks_exact(width as usize) {
            let (a, b) = row.split_at(width as usize / 2);
            assert!(a.iter().all(|&pixel| pixel == left), "outer {outer}, inner {inner}, initial branch");
            assert!(b.iter().all(|&pixel| pixel == right), "outer {outer}, inner {inner}, repeated branch");
        }
    }
}

#[test]
fn long_pattern_iterations_charge_only_connected_visits() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let square = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[1].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(2)),
        ("blendingmode".into(), Value::Int(0)),
        ("patternsize".into(), Value::Float2([1.0; 2])),
        ("opacity".into(), Value::Float(1.0 / 32_768.0)),
    ]);
    for (initial, repeated, count, expected) in [
        (None, Some(square), 12_003, 12_003.0 / 32_768.0),
        (Some(square), None, 16_384, 1.0 / 32_768.0),
        (None, None, 16_384, 0.0),
        (Some(square), None, i32::MAX, 1.0 / 32_768.0),
    ] {
        graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(count));
        graph.nodes[0].children = vec![initial, repeated];
        let image = cook_at_size(graph.clone(), &BTreeMap::new(), 452933, 4).unwrap().remove(0);
        assert!(image.data().iter().all(|&v| v == expected));
    }
}

#[test]
fn repeated_empty_size_switches_leave_the_visit_budget_for_drawing() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let mark = graph.add(PatternNodeKind::Quadrant).unwrap();
    let repeat = graph.add(PatternNodeKind::Iterate).unwrap();
    let gate = graph.add(PatternNodeKind::Switch).unwrap();
    let bright = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children = vec![Some(mark), Some(repeat)];
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(1024));
    graph.nodes[1].parameters.insert("patterntype".into(), Value::Int(2));
    graph.nodes[1].parameters.insert("opacity".into(), Value::Float(0.5));
    graph.nodes[2].children[1] = Some(gate);
    graph.nodes[2].parameters.insert("numberadded".into(), Value::Int(1024));
    graph.nodes[3].children[1] = Some(bright);
    let mut condition = FunctionGraph::default();
    let log_size = condition.add("get_float2", &[], Some(Value::Text("$sizelog2".into())));
    let width = condition.add("swizzle1", &[log_size], Some(Value::Int(0)));
    let threshold = condition.add_constant(Value::Float(6.0));
    condition.output = Some(condition.add("gteq", &[width, threshold], None));
    graph.nodes[3].parameters.insert("switch".into(), Value::Function(Box::new(condition)));
    graph.nodes[4].parameters.insert("patterntype".into(), Value::Int(2));
    assert_eq!(cook(graph).unwrap()[0].get_gray(4, 4, false), 0.5);
}

fn center_at_roughness(graph: PatternGraph, roughness: f32) -> f32 {
    let definition = find("fx_map").unwrap();
    let parameters = resolve_params(
        definition,
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph))),
            ("roughness".into(), Value::Float(roughness)),
            ("color_mode".into(), Value::Bool(false)),
        ]),
    );
    let context = CookContext {
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        w: 16,
        h: 16,
        seed: 0,
        tile: true,
        inputs: &[],
        params: &parameters,
        resource: &|_| None,
        vars: &BTreeMap::new(),
    };
    (definition.cook)(&context).unwrap()[0].get_gray(8, 8, false)
}

fn render_with_visit_limit(graph: &PatternGraph, limit: usize) -> crate::CookResult {
    render_with_inputs_and_visit_limit(graph, &[], false, limit)
}

fn render_with_inputs_and_visit_limit(graph: &PatternGraph, inputs: &[Option<craft_texture::Image>], tile: bool, limit: usize) -> crate::CookResult {
    let params = resolve_params(
        find("fx_map").unwrap(),
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph.clone()))),
            ("roughness".into(), Value::Float(0.0)),
            ("color_mode".into(), Value::Bool(false)),
        ]),
    );
    let context = CookContext {
        w: 4,
        h: 4,
        seed: 19,
        tile,
        inputs,
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        params: &params,
        resource: &|_| None,
        vars: &BTreeMap::new(),
    };
    super::evaluation::cook_with_visit_limit(&context, &[], limit, false)
}

#[test]
fn narrow_quadrants_only_consume_a_draw_when_their_shape_covers_a_sample() {
    // At 4 by 4 the nearest sample to the lower left of center is (.375, .375).
    // The thin rectangle lies between columns. Only the corner of the disc's box reaches that sample.
    for (shape, size, offset) in [(2, [0.0625, 0.75], [0.0; 2]), (3, [0.25; 2], [-0.03125; 2])] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
        let marker = graph.add(PatternNodeKind::Quadrant).unwrap();
        let repeated = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].children = vec![Some(marker), Some(repeated)];
        graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(80));
        square(&mut graph, 1);
        graph.nodes[1].parameters.insert("opacity".into(), Value::Float(0.375));
        graph.nodes[2].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(shape)),
            ("patternsize".into(), Value::Float2(size)),
            ("frameoffset".into(), Value::Float2(offset)),
            ("opacity".into(), Value::Float(0.25)),
        ]);
        assert_eq!(render_with_visit_limit(&graph, 64).unwrap()[0].data(), &[0.375; 16]);

        graph.nodes[2].parameters.insert("frameoffset".into(), Value::Float2([-0.125; 2]));
        assert!(render_with_visit_limit(&graph, 64).unwrap_err().contains("execution budget"));
        assert_eq!(render_with_visit_limit(&graph, 128).unwrap()[0].get_gray(1, 1, false), 20.375);
    }
}

#[test]
fn terminal_quadrants_charge_the_first_discard_of_each_sibling_group() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let marker = graph.add(PatternNodeKind::Quadrant).unwrap();
    let branch = graph.add(PatternNodeKind::Quadrant).unwrap();
    let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children = vec![Some(marker), Some(branch)];
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(48));
    square(&mut graph, 1);
    graph.nodes[1].parameters.insert("opacity".into(), Value::Float(0.375));
    graph.nodes[2].children.fill(Some(leaf));
    square(&mut graph, 3);
    graph.nodes[3].parameters.insert("patternsize".into(), Value::Float2([1.0 / 64.0; 2]));

    let image = render_with_visit_limit(&graph, 64).unwrap().remove(0);
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(image.get_gray(x, y, false), 0.375);
        }
    }

    // Visible siblings each consume a visit, even if their pixels coincide.
    let mut visible = graph.clone();
    visible.nodes[3].parameters.insert("patternsize".into(), Value::Float2([2.0; 2]));
    assert!(render_with_visit_limit(&visible, 64).unwrap_err().contains("execution budget"));

    // With only one child per branch, extending the iteration still exhausts the ordinary allowances.
    graph.nodes[2].children[1..].fill(None);
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(80));
    assert!(render_with_visit_limit(&graph, 64).unwrap_err().contains("execution budget"));
}

fn nested_square_draws(inner_count: i32) -> PatternGraph {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    for _ in 0..6 {
        let parent = graph.nodes.len() - 1;
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[parent].children[0] = Some(child);
    }
    let outer = graph.add(PatternNodeKind::Iterate).unwrap();
    let inner = graph.add(PatternNodeKind::Iterate).unwrap();
    let square = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[6].children[0] = Some(outer);
    graph.nodes[7].children[1] = Some(inner);
    graph.nodes[8].children[1] = Some(square);
    graph.nodes[7].parameters.insert("numberadded".into(), Value::Int(4));
    graph.nodes[8].parameters.insert("numberadded".into(), Value::Int(inner_count));
    let leaf = &mut graph.nodes[9].parameters;
    leaf.insert("patterntype".into(), Value::Int(2));
    leaf.insert("patternsize".into(), Value::Float2([512.0; 2]));
    leaf.insert("opacity".into(), Value::Float(0.01));
    graph
}

#[test]
fn a_single_repeat_passes_its_written_value_and_seed_directly_to_the_child() {
    let mut graph = nested_square_draws(1);
    graph.nodes[7].parameters.insert("numberadded".into(), Value::Int(64));

    let mut count = FunctionGraph::default();
    let number = count.add("get_float1", &[], Some(Value::Text("$number".into())));
    let scale = count.add_constant(Value::Float(1.0 / 128.0));
    let weight = count.add("mul", &[number, scale], None);
    let write = count.add("set", &[weight], Some(Value::Text("weight".into())));
    let one = count.add_constant(Value::Int(1));
    count.output = Some(count.add("sequence", &[write, one], None));
    graph.nodes[8].parameters.insert("numberadded".into(), Value::Function(Box::new(count)));

    let mut tint = FunctionGraph::input("weight", materialcraft_core::ValueType::Float);
    let gain = tint.output.unwrap();
    let span = tint.add_constant(Value::Float(0.125));
    let draw = tint.add("rand", &[span], None);
    tint.output = Some(tint.add("add", &[gain, draw], None));
    graph.nodes[9].parameters.insert("opacity".into(), Value::Function(Box::new(tint)));

    let expected = render_with_visit_limit(&graph, 8192).unwrap().remove(0);
    assert_eq!(expected.data(), &[f32::from_bits(1_100_931_893); 16]);
    assert_eq!(render_with_visit_limit(&graph, 64).unwrap()[0].data(), expected.data());
    assert!(render_with_visit_limit(&graph, 32).unwrap_err().contains("budget"));
}

#[test]
fn streamed_analytic_draws_keep_the_accumulated_pixels_and_bounded_work() {
    let mut graph = nested_square_draws(24);
    let expected = (0..96).fold(0.0_f32, |background, _| background + 0.01_f32);
    assert_eq!(render_with_visit_limit(&graph, 256).unwrap()[0].data(), &[expected; 16]);
    assert_eq!(render_with_visit_limit(&graph, 64).unwrap()[0].data(), &[expected; 16]);

    graph.nodes[7].parameters.insert("numberadded".into(), Value::Int(12));
    let expected = (0..288).fold(0.0_f32, |background, _| background + 0.01_f32);
    assert_eq!(render_with_visit_limit(&graph, 64).unwrap()[0].data(), &[expected; 16]);

    graph.nodes[7].parameters.insert("numberadded".into(), Value::Int(20));
    assert!(render_with_visit_limit(&graph, 64).unwrap_err().contains("execution budget"));
}

#[test]
fn streamed_squares_charge_the_tile_copies_after_the_first() {
    let mut graph = nested_square_draws(21);
    // Each of the 84 draws covers 12,484 horizontal copies and a single row of tiles.
    // The first copy was visited with its draw, leaving 1,048,572 extra copies to rasterize.
    let leaf = &mut graph.nodes[9].parameters;
    leaf.insert("frameoffset".into(), Value::Float2([0.0, 0.25]));
    leaf.insert("patternsize".into(), Value::Float2([12_483.0 * 128.0, 32.0]));
    let image = render_with_inputs_and_visit_limit(&graph, &[], true, 64).unwrap().remove(0);
    assert!(image.get_gray(1, 1, false) > 1.0);

    graph.nodes[9].parameters.insert("patternsize".into(), Value::Float2([12_485.0 * 128.0, 32.0]));
    assert!(render_with_inputs_and_visit_limit(&graph, &[], true, 64).unwrap_err().contains("pattern repetition exceeds its budget"));
}

#[test]
fn streamed_image_draws_share_their_filter_and_source_levels() {
    let mut graph = nested_square_draws(35);
    let source = craft_texture::Image::gray(32, 32, |x, y| ((13 * x + 29 * y + 7 * x * y + 17) % 256) as f32 / 255.0);
    let inputs = [None, Some(source)];
    graph.nodes[9].parameters.insert("patterntype".into(), Value::Int(1));
    graph.nodes[9].parameters.insert("patternsize".into(), Value::Float2([96.0, 80.0]));
    for filter in [0, 1, 2] {
        graph.nodes[9].parameters.insert("imagefiltering".into(), Value::Int(filter));
        let expected = render_with_inputs_and_visit_limit(&graph, &inputs, false, 512).unwrap().remove(0);
        let streamed = render_with_inputs_and_visit_limit(&graph, &inputs, false, 64).unwrap().remove(0);
        assert_eq!(streamed.data(), expected.data());
        assert!(streamed.data().iter().any(|value| *value > 0.0));
    }

    // A different filter has a different raster rank even when both quadrants use the same slot.
    let child = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[9].children[0] = Some(child);
    graph.nodes[10].parameters = graph.nodes[9].parameters.clone();
    graph.nodes[10].parameters.insert("imagefiltering".into(), Value::Int(0));
    assert!(render_with_inputs_and_visit_limit(&graph, &inputs, false, 64).unwrap_err().contains("execution budget"));
}

#[test]
fn streamed_quadrant_children_reuse_fixed_image_properties() {
    let mut graph = nested_square_draws(16);
    graph.nodes[7].parameters.insert("numberadded".into(), Value::Int(8));
    graph.nodes[9].parameters.insert("patterntype".into(), Value::Int(1));
    graph.nodes[9].parameters.insert("imageindex".into(), Value::Int(1));
    graph.nodes[9].parameters.insert("randominherited".into(), Value::Bool(false));
    let child = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[9].children.fill(Some(child));
    let mut tint = FunctionGraph::default();
    let one = tint.add_constant(Value::Float(1.0));
    tint.output = Some(tint.add("rand", &[one], None));
    graph.nodes[10].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(257)),
        ("patternsize".into(), Value::Float2([512.0; 2])),
        ("opacity".into(), Value::Function(Box::new(tint))),
    ]);
    let field = craft_texture::Image::gray(8, 8, |x, y| (1 + x + 2 * y) as f32 / 32.0);
    let inputs = [None, None, Some(field)];
    let expected = render_with_inputs_and_visit_limit(&graph, &inputs, false, 2_048).unwrap().remove(0);
    let streamed = render_with_inputs_and_visit_limit(&graph, &inputs, false, 64).unwrap().remove(0);
    assert_eq!(streamed.data(), expected.data());
    assert!(streamed.data().iter().any(|value| *value > 1.0));
}

#[test]
fn seeded_placement_fields_keep_sparse_draws_within_the_work_budget() {
    let mut graph = nested_square_draws(24);
    let mut offset = FunctionGraph::default();
    let range = offset.add_constant(Value::Float(0.75));
    let x = offset.add("rand", &[range], None);
    let y = offset.add("rand", &[range], None);
    let right = offset.add_constant(Value::Float(0.0625));
    let down = offset.add_constant(Value::Float(-0.0625));
    let x = offset.add("add", &[x, right], None);
    let y = offset.add("add", &[y, down], None);
    let point = offset.add("vector2", &[x, y], None);
    offset.output = Some(point);

    let mut rotation = FunctionGraph::default();
    let quarter = rotation.add_constant(Value::Float(0.25));
    let turn = rotation.add("rand", &[quarter], None);
    let base = rotation.add_constant(Value::Float(0.125));
    rotation.output = Some(rotation.add("add", &[base, turn], None));
    let mut tint = FunctionGraph::default();
    let one = tint.add_constant(Value::Float(1.0));
    tint.output = Some(tint.add("rand", &[one], None));

    let child = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[9].children.fill(Some(child));
    graph.nodes[9].parameters.extend([
        ("frameoffset".into(), Value::Function(Box::new(offset.clone()))),
        ("patternrotation".into(), Value::Function(Box::new(rotation))),
        ("opacity".into(), Value::Function(Box::new(tint))),
        ("patternsize".into(), Value::Float2([2.0, 8.0])),
    ]);
    graph.nodes[10].parameters = graph.nodes[9].parameters.clone();
    let mut ordinary = graph.clone();
    // The Sequence preserves both random coordinates and evaluates the offset through the full VM.
    let zero = offset.add_constant(Value::Float(0.0));
    offset.output = Some(offset.add("sequence", &[zero, point], None));
    for node in &mut ordinary.nodes[9..] {
        node.parameters.insert("frameoffset".into(), Value::Function(Box::new(offset.clone())));
    }
    let expected = render_with_visit_limit(&ordinary, 4096).unwrap().remove(0);
    assert!(expected.data().windows(2).any(|v| v[0] != v[1]));
    assert!(render_with_visit_limit(&ordinary, 80).unwrap_err().contains("execution budget"));
    assert_eq!(render_with_visit_limit(&graph, 80).unwrap()[0].data(), expected.data());
    assert!(render_with_visit_limit(&graph, 32).unwrap_err().contains("execution budget"));
}

#[test]
fn repeated_image_space_offsets_and_rotations_reuse_the_parent_draw() {
    let mut graph = nested_square_draws(55);
    graph.nodes[7].parameters.insert("numberadded".into(), Value::Int(2));
    graph.nodes[8].parameters.insert("randominherited".into(), Value::Bool(false));
    let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[9].children.fill(Some(leaf));
    graph.nodes[10].parameters = graph.nodes[9].parameters.clone();
    for node in &mut graph.nodes[9..=10] {
        let mut tint = FunctionGraph::default();
        let one = tint.add_constant(Value::Float(1.0));
        tint.output = Some(tint.add("rand", &[one], None));
        node.parameters.insert("opacity".into(), Value::Function(Box::new(tint)));
        node.parameters.insert("frameoffset".into(), Value::Function(Box::new(FunctionGraph::constant(Value::Float2([0.125, -0.25])))));
        let mut angle = FunctionGraph::default();
        let quarter = angle.add_constant(Value::Float(0.25));
        angle.output = Some(angle.add("rand", &[quarter], None));
        node.parameters.insert("patternrotation".into(), Value::Function(Box::new(angle)));
    }
    let expected = render_with_visit_limit(&graph, 1_024).unwrap().remove(0);
    let limited = render_with_visit_limit(&graph, 64).unwrap().remove(0);
    assert_eq!(limited.data(), expected.data());
    assert!(limited.data().iter().any(|value| *value > 1.0));
}

fn quadrants_with_a_live_tint() -> (PatternGraph, PatternGraph) {
    let mut graph = nested_square_draws(40);
    graph.nodes[7].parameters.insert("numberadded".into(), Value::Int(6));
    graph.nodes[8].parameters.insert("randominherited".into(), Value::Bool(false));
    let mut count = FunctionGraph::default();
    let number = count.add("get_float1", &[], Some(Value::Text("$number".into())));
    let row = count.add("set", &[number], Some(Value::Text("row".into())));
    let eighth = count.add_constant(Value::Float(0.125));
    let weight = count.add("mul", &[number, eighth], None);
    let gain = count.add("set", &[weight], Some(Value::Text("gain".into())));
    let forty = count.add_constant(Value::Int(40));
    let finish = count.add("sequence", &[gain, forty], None);
    count.output = Some(count.add("sequence", &[row, finish], None));
    graph.nodes[8].parameters.insert("numberadded".into(), Value::Function(Box::new(count)));

    let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
    let marker = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[9].children = vec![Some(leaf), Some(leaf), Some(leaf), Some(marker)];
    let mut tint = FunctionGraph::default();
    let width = tint.add_constant(Value::Float(1.0 / 128.0));
    let shift = tint.add("rand", &[width], None);
    let corner = tint.add_constant(Value::Float(0.118));
    let shift = tint.add("add", &[corner, shift], None);
    let position = tint.add("vector2", &[shift, shift], None);
    let publish = tint.add("set", &[position], Some(Value::Text("placed".into())));
    let one = tint.add_constant(Value::Float(1.0));
    let base = tint.add("rand", &[one], None);
    let gain = tint.add("get_float1", &[], Some(Value::Text("gain".into())));
    let scaled = tint.add("mul", &[base, gain], None);
    let row = tint.add("get_float1", &[], Some(Value::Text("row".into())));
    let threshold = tint.add_constant(Value::Float(1.5));
    let test = tint.add("gt", &[row, threshold], None);
    let alpha = tint.add("ifelse", &[test, scaled, base], None);
    tint.output = Some(tint.add("sequence", &[publish, alpha], None));
    let offset = FunctionGraph::input("placed", materialcraft_core::ValueType::Float2);
    for node in &mut graph.nodes[9..] {
        node.parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(2)),
            ("frameoffset".into(), Value::Function(Box::new(offset.clone()))),
            ("patternsize".into(), Value::Float2([1.0; 2])),
        ]);
    }
    for node in &mut graph.nodes[9..11] {
        node.parameters.insert("opacity".into(), Value::Function(Box::new(tint.clone())));
    }
    graph.nodes[11].parameters.insert("opacity".into(), Value::Float(0.25));

    // This equivalent computed offset reads the published value through the full evaluator.
    let mut ordinary = graph.clone();
    let mut computed = offset;
    let link = computed.output.unwrap();
    let zero = computed.add_constant(Value::Float2([0.0; 2]));
    computed.output = Some(computed.add("add", &[link, zero], None));
    for node in &mut ordinary.nodes[9..11] {
        node.parameters.insert("frameoffset".into(), Value::Function(Box::new(computed.clone())));
    }
    (graph, ordinary)
}

#[test]
fn repeated_quadrants_reapply_the_live_tint_scale_and_publish_the_frame_offset() {
    let (mut graph, mut ordinary) = quadrants_with_a_live_tint();
    let expected = render_with_visit_limit(&ordinary, 4096).unwrap().remove(0);
    assert!(expected.data().iter().any(|&v| v > 1.0));
    assert!(render_with_visit_limit(&ordinary, 192).unwrap_err().contains("execution budget"));
    let cached = render_with_visit_limit(&graph, 192).unwrap().remove(0);
    assert_eq!(cached.data(), expected.data());

    // Inheriting the outer seed gives different random offsets and tints on later
    // iterations. A bound arithmetic pass keeps the offset, gain and every draw.
    for drawing in [&mut graph, &mut ordinary] {
        drawing.nodes[8].parameters.insert("randominherited".into(), Value::Bool(true));
    }
    let expected = render_with_visit_limit(&ordinary, 4096).unwrap().remove(0);
    assert!(expected.data().iter().any(|&v| v > 1.0));
    assert_eq!(render_with_visit_limit(&graph, 96).unwrap()[0].data(), expected.data());
    graph.nodes[7].parameters.insert("numberadded".into(), Value::Int(10));
    assert!(render_with_visit_limit(&graph, 96).unwrap_err().contains("execution budget"));
}

#[test]
fn reused_terminal_offsets_and_compact_parent_draws_share_bounded_work() {
    let (mut graph, mut ordinary) = quadrants_with_a_live_tint();
    for drawing in [&mut graph, &mut ordinary] {
        drawing.nodes[8].parameters.insert("randominherited".into(), Value::Bool(true));
        // Three additional branches inherit distinct seeds. Their terminal children
        // reset theirs, so the same offset is published again in a different parent.
        let branch = drawing.add(PatternNodeKind::Quadrant).unwrap();
        drawing.nodes[12].parameters = drawing.nodes[10].parameters.clone();
        drawing.nodes[12].children = drawing.nodes[9].children.clone();
        drawing.nodes[9].children[..3].fill(Some(branch));
        drawing.nodes[10].parameters.insert("randominherited".into(), Value::Bool(false));
    }
    let expected = render_with_visit_limit(&ordinary, 65_536).unwrap().remove(0);
    assert!(expected.data().iter().any(|value| *value > 1.0));
    assert_eq!(render_with_visit_limit(&graph, 256).unwrap()[0].data(), expected.data());
    assert!(render_with_visit_limit(&graph, 160).unwrap_err().contains("execution budget"));
}

#[test]
fn a_seeded_branch_offset_keeps_every_scheduled_child() {
    let mut graph = nested_square_draws(32);
    let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[9].children[0] = Some(leaf);
    graph.nodes[9].parameters.clear();
    let mut offset = FunctionGraph::default();
    let span = offset.add_constant(Value::Float(0.875));
    let x = offset.add("rand", &[span], None);
    let y = offset.add("rand", &[span], None);
    offset.output = Some(offset.add("vector2", &[x, y], None));
    graph.nodes[9].parameters.insert("branchoffset".into(), Value::Function(Box::new(offset.clone())));
    graph.nodes[10].parameters =
        BTreeMap::from([("patterntype".into(), Value::Int(2)), ("patternsize".into(), Value::Float2([96.0, 48.0])), ("opacity".into(), Value::Float(0.125))]);
    let mut ordinary = graph.clone();
    // An equivalent Sequence uses the general evaluator and leaves the random stream intact.
    let value = offset.output.unwrap();
    let zero = offset.add_constant(Value::Float(0.0));
    offset.output = Some(offset.add("sequence", &[zero, value], None));
    ordinary.nodes[9].parameters.insert("branchoffset".into(), Value::Function(Box::new(offset)));
    let expected = render_with_visit_limit(&ordinary, 8192).unwrap().remove(0);
    assert!(expected.data().windows(2).any(|v| v[0] != v[1]));
    assert!(render_with_visit_limit(&ordinary, 64).unwrap_err().contains("execution budget"));
    assert_eq!(render_with_visit_limit(&graph, 64).unwrap()[0].data(), expected.data());
    assert!(render_with_visit_limit(&graph, 32).unwrap_err().contains("execution budget"));
}

#[test]
fn sparse_branch_offsets_and_tints_reuse_properties_across_drawing_depths() {
    let mut graph = nested_square_draws(40);
    graph.nodes[8].parameters.insert("randominherited".into(), Value::Bool(false));
    for parent in [9, 10] {
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[parent].children.fill(Some(child));
    }
    let mut tint = FunctionGraph::default();
    let one = tint.add_constant(Value::Float(1.0));
    tint.output = Some(tint.add("rand", &[one], None));
    let offset = FunctionGraph::constant(Value::Float2([1.0 / 4096.0, -1.0 / 4096.0]));
    for node in &mut graph.nodes[9..=11] {
        node.parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(3)),
            ("branchoffset".into(), Value::Function(Box::new(offset.clone()))),
            ("opacity".into(), Value::Function(Box::new(tint.clone()))),
            ("frameoffset".into(), Value::Float2([0.125 - 1.0 / 256.0; 2])),
            ("patternsize".into(), Value::Float2([1.0; 2])),
        ]);
    }
    let expected = render_with_visit_limit(&graph, 8_192).unwrap().remove(0);
    let limited = render_with_visit_limit(&graph, 384).unwrap().remove(0);
    assert_eq!(limited.data(), expected.data());
    assert!(limited.data().iter().any(|v| *v > 0.0));
    assert!(limited.data().contains(&0.0));
}

#[test]
fn cached_branch_tints_keep_their_drawing_depth_and_stamp_size() {
    let fixed = FunctionGraph::constant(Value::Float(0.75));
    let depth = FunctionGraph::input("$depth", materialcraft_core::ValueType::Float);
    for (expression, luminosities) in [(fixed, [0.75, 0.75]), (depth, [0.0, 1.0])] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].children[0] = Some(child);
        for node in &mut graph.nodes {
            node.parameters = BTreeMap::from([
                ("patterntype".into(), Value::Int(2)),
                ("randominherited".into(), Value::Bool(false)),
                ("patternsize".into(), Value::Float2([1.0; 2])),
                ("branchoffset".into(), Value::Function(Box::new(FunctionGraph::constant(Value::Float2([0.0; 2]))))),
                ("opacity".into(), Value::Function(Box::new(expression.clone()))),
            ]);
        }
        let mut literal = graph.clone();
        for (node, value) in literal.nodes.iter_mut().zip(luminosities) {
            node.parameters.insert("branchoffset".into(), Value::Float2([0.0; 2]));
            node.parameters.insert("opacity".into(), Value::Float(value));
        }
        let expected = render_with_visit_limit(&literal, 64).unwrap().remove(0);
        let actual = render_with_visit_limit(&graph, 64).unwrap().remove(0);
        assert!(expected.data().iter().any(|v| *v > 0.0));
        assert_eq!(actual.data(), expected.data());
    }
}

#[test]
fn cached_parents_that_miss_still_draw_their_terminal_children() {
    let mut graph = nested_square_draws(41);
    graph.nodes[8].parameters.insert("randominherited".into(), Value::Bool(false));
    let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[9].children.fill(Some(leaf));
    graph.nodes[10].parameters = graph.nodes[9].parameters.clone();
    // At this depth the parent falls between pixel centers. Its four children cover the image.
    graph.nodes[9].parameters.insert("patternsize".into(), Value::Float2([0.015625; 2]));
    for node in &mut graph.nodes[9..=10] {
        let mut tint = FunctionGraph::default();
        let one = tint.add_constant(Value::Float(1.0));
        tint.output = Some(tint.add("rand", &[one], None));
        node.parameters.insert("opacity".into(), Value::Function(Box::new(tint)));
    }
    let expected = render_with_visit_limit(&graph, 1_024).unwrap().remove(0);
    assert!(expected.data().iter().all(|value| *value > 1.0));
    assert_eq!(render_with_visit_limit(&graph, 64).unwrap()[0].data(), expected.data());

    graph.nodes[7].parameters.insert("numberadded".into(), Value::Int(10));
    assert!(render_with_visit_limit(&graph, 64).unwrap_err().contains("execution budget"));
}

#[test]
fn repeated_quadrants_keep_both_translations_when_reusing_a_draw() {
    let mut graph = nested_square_draws(41);
    graph.nodes[8].parameters.insert("randominherited".into(), Value::Bool(false));
    let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[9].children.fill(Some(leaf));
    graph.nodes[10].parameters = graph.nodes[9].parameters.clone();
    graph.nodes[9].parameters.insert("patternsize".into(), Value::Float2([0.015625; 2]));
    for node in &mut graph.nodes[9..] {
        let mut angle = FunctionGraph::default();
        let eighth = angle.add_constant(Value::Float(0.125));
        angle.output = Some(angle.add("rand", &[eighth], None));
        node.parameters.insert("patternrotation".into(), Value::Function(Box::new(angle)));
        for (field, offset) in [("branchoffset", [0.03125, -0.015625]), ("frameoffset", [-0.015625, 0.03125])] {
            node.parameters.insert(field.into(), Value::Function(Box::new(FunctionGraph::constant(Value::Float2(offset)))));
        }
    }
    let expected = render_with_visit_limit(&graph, 1_024).unwrap().remove(0);
    assert!(expected.data().iter().any(|value| *value > 0.0));
    // A smaller budget still stops the four repetitions of 41 parent draws.
    assert!(render_with_visit_limit(&graph, 64).unwrap_err().contains("execution budget"));
    assert_eq!(render_with_visit_limit(&graph, 128).unwrap()[0].data(), expected.data());
}

#[test]
fn repeated_terminal_misses_cover_uncached_misses_in_the_next_branch() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let marker = graph.add(PatternNodeKind::Quadrant).unwrap();
    let pair = graph.add(PatternNodeKind::Quadrant).unwrap();
    let first = graph.add(PatternNodeKind::Quadrant).unwrap();
    let second = graph.add(PatternNodeKind::Quadrant).unwrap();
    let cached = graph.add(PatternNodeKind::Quadrant).unwrap();
    let uncached = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children = vec![Some(marker), Some(pair)];
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(34));
    square(&mut graph, 1);
    graph.nodes[1].parameters.insert("opacity".into(), Value::Float(0.375));
    graph.nodes[2].children = vec![Some(first), Some(second), None, None];
    graph.nodes[3].children.fill(Some(cached));
    graph.nodes[4].children.fill(Some(uncached));
    for node in &mut graph.nodes[3..] {
        node.parameters.insert("patterntype".into(), Value::Int(2));
        node.parameters.insert("patternsize".into(), Value::Float2([0.015625; 2]));
    }
    // Both sets of leaves fall between sample centers. Only the first set can reuse its properties.
    for node in &mut graph.nodes[5..] {
        node.parameters.insert("frameoffset".into(), Value::Float2([0.0625; 2]));
    }
    let mut tint = FunctionGraph::default();
    let gain = tint.add_constant(Value::Float(0.5));
    tint.output = Some(tint.add("rand", &[gain], None));
    graph.nodes[5].parameters.insert("randominherited".into(), Value::Bool(false));
    graph.nodes[5].parameters.insert("opacity".into(), Value::Function(Box::new(tint)));
    assert_eq!(render_with_visit_limit(&graph, 64).unwrap()[0].data(), &[0.375; 16]);

    // The cached leaf still pays for every retained draw when moved onto the samples.
    graph.nodes[5].parameters.insert("frameoffset".into(), Value::Float2([0.0; 2]));
    assert!(render_with_visit_limit(&graph, 64).unwrap_err().contains("execution budget"));
}

#[test]
fn repeated_terminal_properties_leave_room_for_their_visible_siblings() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let marker = graph.add(PatternNodeKind::Quadrant).unwrap();
    let parent = graph.add(PatternNodeKind::Quadrant).unwrap();
    let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children = vec![Some(marker), Some(parent)];
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(100));
    square(&mut graph, 1);
    graph.nodes[1].parameters.insert("opacity".into(), Value::Float(0.375));
    graph.nodes[2].children.fill(Some(leaf));
    for i in [2, 3] {
        square(&mut graph, i);
        graph.nodes[i].parameters.insert("patternsize".into(), Value::Float2([1.0 / 64.0; 2]));
    }
    let mut tint = FunctionGraph::default();
    let scale = tint.add("const_float1", &[], Some(Value::Float(0.5)));
    tint.output = Some(tint.add("rand", &[scale], None));
    graph.nodes[3].parameters.insert("opacity".into(), Value::Function(Box::new(tint)));
    graph.nodes[3].parameters.insert("randominherited".into(), Value::Bool(false));

    let image = render_with_visit_limit(&graph, 64).unwrap().remove(0);
    assert_eq!(image.data(), &[0.375; 16]);

    let mut positioned = graph.clone();
    let mut position = FunctionGraph::default();
    position.output = Some(position.add("get_float2", &[], Some(Value::Text("$pos".into()))));
    positioned.nodes[3].parameters.insert("frameoffset".into(), Value::Function(Box::new(position)));
    assert!(render_with_visit_limit(&positioned, 64).unwrap_err().contains("execution budget"));

    let mut writable = graph.clone();
    let mut write = FunctionGraph::default();
    let gain = write.add("const_float1", &[], Some(Value::Float(0.5)));
    write.output = Some(write.add("set", &[gain], Some(Value::Text("gain".into()))));
    writable.nodes[2].parameters.insert("opacity".into(), Value::Function(Box::new(write)));
    let mut read = FunctionGraph::default();
    read.output = Some(read.add("get_float1", &[], Some(Value::Text("gain".into()))));
    writable.nodes[3].parameters.insert("opacity".into(), Value::Function(Box::new(read)));
    assert!(render_with_visit_limit(&writable, 64).unwrap_err().contains("execution budget"));

    // Reusing a tint does not make a visible stamp free.
    graph.nodes[3].parameters.insert("patternsize".into(), Value::Float2([2.0; 2]));
    assert!(render_with_visit_limit(&graph, 64).unwrap_err().contains("execution budget"));
}

#[test]
fn empty_terminal_quadrants_share_the_discard_allowance() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let marker = graph.add(PatternNodeKind::Quadrant).unwrap();
    let parent = graph.add(PatternNodeKind::Quadrant).unwrap();
    let empty = graph.add(PatternNodeKind::Quadrant).unwrap();
    let gate = graph.add(PatternNodeKind::Switch).unwrap();
    let live = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children = vec![Some(marker), Some(parent)];
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(24));
    square(&mut graph, 1);
    graph.nodes[1].parameters.insert("opacity".into(), Value::Float(0.375));
    graph.nodes[2].children.fill(Some(empty));
    graph.nodes[3].children[0] = Some(gate);
    graph.nodes[4].children[1] = Some(live);
    square(&mut graph, 5);

    let image = render_with_visit_limit(&graph, 64).unwrap().remove(0);
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(image.get_gray(x, y, false), 0.375);
        }
    }

    graph.nodes[4].parameters.insert("switch".into(), Value::Bool(true));
    assert!(render_with_visit_limit(&graph, 64).unwrap_err().contains("execution budget"));
    graph.nodes[4].parameters.insert("switch".into(), Value::Bool(false));
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(80));
    assert!(render_with_visit_limit(&graph, 64).unwrap_err().contains("execution budget"));
}

#[test]
fn empty_switches_at_later_depths_still_affect_roughness() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let empty = graph.add(PatternNodeKind::Switch).unwrap();
    let deeper = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].parameters.insert("patterntype".into(), Value::Int(2));
    graph.nodes[0].children = vec![Some(empty), None, None, Some(deeper)];
    graph.nodes[2].children[3] = Some(empty);
    assert_eq!(center_at_roughness(graph, 1.0), 1.0 / 3.0);
}

#[test]
fn connected_quadrants_set_roughness_even_when_their_branch_is_idle() {
    for kind in [PatternNodeKind::Switch, PatternNodeKind::Iterate] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        let gate = graph.add(kind).unwrap();
        let middle = graph.add(PatternNodeKind::Quadrant).unwrap();
        let leaf = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].parameters.insert("patterntype".into(), Value::Int(2));
        graph.nodes[0].children[0] = Some(gate);
        graph.nodes[1].children[1] = Some(middle);
        graph.nodes[2].children[3] = Some(leaf);
        for enabled in [false, true] {
            if kind == PatternNodeKind::Switch {
                graph.nodes[1].parameters.insert("switch".into(), Value::Bool(enabled));
            } else {
                graph.nodes[1].parameters.insert("numberadded".into(), Value::Int(i32::from(enabled)));
            }
            assert_eq!(center_at_roughness(graph.clone(), 1.0), 1.0 / 3.0);
            assert_eq!(center_at_roughness(graph.clone(), 0.6), 0.510_204_1);
        }
        graph.nodes[0].children[0] = None;
        assert_eq!(center_at_roughness(graph, 0.6), 1.0);
    }
}

#[test]
fn missing_direct_pattern_inputs_use_the_field_default() {
    use materialcraft_core::ValueType;

    let direct = FunctionGraph::input("strength", ValueType::Float);
    let mut derived = direct.clone();
    let read = derived.output.unwrap();
    let offset = derived.add_constant(Value::Float(0.375));
    derived.output = Some(derived.add("add", &[read, offset], None));
    for (expression, binding, expected) in [
        (direct.clone(), None, 1.0),
        (direct.clone(), Some(0.0), 0.0),
        (direct, Some(0.375), 0.375),
        (derived, None, 0.375),
        (FunctionGraph::input("tint", ValueType::Float4), None, 1.0),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        graph.nodes[0].parameters = BTreeMap::from([("patterntype".into(), Value::Int(2)), ("opacity".into(), Value::Function(Box::new(expression)))]);
        let vars = binding.map(|v| ("strength".into(), Value::Float(v))).into_iter().collect();
        let image = cook_with_variables(graph, &vars, 7).unwrap().remove(0);
        assert_eq!(image.get_gray(8, 8, false), expected);
    }

    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let square = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children[1] = Some(square);
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Function(Box::new(FunctionGraph::input("count", ValueType::Integer))));
    graph.nodes[1].parameters.insert("patterntype".into(), Value::Int(2));
    graph.nodes[1].parameters.insert("patternsize".into(), Value::Function(Box::new(FunctionGraph::input("extent", ValueType::Float2))));
    assert!(cook(graph).unwrap()[0].data().iter().all(|&value| value == 1.0));
}

#[test]
fn a_sibling_can_update_a_binding_declared_by_the_parent() {
    use materialcraft_core::ValueType;

    for (published, initialized, writer_first, expected) in
        [(None, false, true, 1.0), (Some(0.25), false, true, 0.25), (Some(0.25), true, true, 0.75), (None, true, true, 0.75), (Some(0.25), true, false, 0.5625)]
    {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
        let writer = graph.add(PatternNodeKind::Quadrant).unwrap();
        let reader = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].children = if writer_first { vec![Some(writer), Some(reader)] } else { vec![Some(reader), Some(writer)] };
        if initialized {
            graph.nodes[0].parameters.insert("numberadded".into(), set_then_return("tint", Value::Float(0.5625), Value::Int(1)));
        }
        graph.nodes[1].parameters.insert("patterntype".into(), Value::Int(2));
        graph.nodes[1].parameters.insert("opacity".into(), set_then_return("tint", Value::Float(0.75), Value::Float(0.0)));
        graph.nodes[2].parameters.insert("patterntype".into(), Value::Int(2));
        graph.nodes[2].parameters.insert("opacity".into(), Value::Function(Box::new(FunctionGraph::input("tint", ValueType::Float))));
        let variables = published.map(|v| ("tint".into(), Value::Float(v))).into_iter().collect();
        let image = cook_with_variables(graph, &variables, 17).unwrap().remove(0);
        assert_eq!(image.get_gray(8, 8, false), expected, "published {published:?}, initialized {initialized}, writer first {writer_first}");
    }
}

fn advance_tree_offset() -> Value {
    let mut advance = FunctionGraph::default();
    let index = advance.add("get_float1", &[], Some(Value::Text("tree_index".into())));
    let one = advance.add_constant(Value::Float(1.0));
    let next = advance.add("add", &[index, one], None);
    let stored = advance.add("set", &[next], Some(Value::Text("tree_index".into())));
    let origin = advance.add_constant(Value::Float2([0.0; 2]));
    advance.output = Some(advance.add("sequence", &[stored, origin], None));
    Value::Function(Box::new(advance))
}

fn set_then_return(name: &str, value: Value, result: Value) -> Value {
    let mut function = FunctionGraph::default();
    let value = function.add_constant(value);
    let set = function.add("set", &[value], Some(Value::Text(name.into())));
    let result = function.add_constant(result);
    function.output = Some(function.add("sequence", &[set, result], None));
    Value::Function(Box::new(function))
}

#[test]
fn first_writable_float_depends_on_connected_draw_properties() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let writer = graph.add(PatternNodeKind::Quadrant).unwrap();
    let switch = graph.add(PatternNodeKind::Switch).unwrap();
    let other = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children = vec![Some(writer), Some(writer), Some(writer), Some(switch)];
    graph.nodes[2].children[0] = Some(writer);
    square(&mut graph, 1);

    let mut field = FunctionGraph::input("local", materialcraft_core::ValueType::Float);
    let current = field.output.unwrap();
    let defined = field.add("eq", &[current, current], None);
    let yes = field.add_constant(Value::Float(0.375));
    let no = field.add_constant(Value::Float(0.625));
    let next = field.add("ifelse", &[defined, yes, no], None);
    field.output = Some(field.add("set", &[next], Some(Value::Text("local".into()))));
    graph.nodes[1].parameters.insert("opacity".into(), Value::Function(Box::new(field)));
    let published = FunctionGraph::input("gain", materialcraft_core::ValueType::Float);
    graph.nodes[3].parameters.insert("opacity".into(), Value::Function(Box::new(published)));
    let bindings = BTreeMap::from([("gain".into(), Value::Float(0.8125))]);
    for (connected, shape, first) in [(false, 2, 0.375), (true, 0, 0.375), (true, 2, 0.625)] {
        graph.nodes[2].children[1] = connected.then_some(other);
        graph.nodes[3].parameters.insert("patterntype".into(), Value::Int(shape));
        let image = cook_with_variables(graph.clone(), &bindings, 41301).unwrap().remove(0);
        for (x, y, expected) in [(4, 4, first), (12, 4, 0.375), (4, 12, 0.375), (12, 12, 0.375)] {
            assert_eq!(image.get_gray(x, y, false), expected, "connected {connected}, shape {shape}, pixel ({x}, {y})");
        }
    }
}

#[test]
fn quadrant_children_accumulate_in_their_own_depth() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let writer = graph.add(PatternNodeKind::Quadrant).unwrap();
    let reader = graph.add(PatternNodeKind::Quadrant).unwrap();
    let increment = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children = vec![Some(writer), Some(reader), Some(increment), Some(increment)];
    graph.nodes[0].parameters.insert("branchoffset".into(), set_then_return("count", Value::Float(0.25), Value::Float2([0.0; 2])));
    for node in 1..=3 {
        square(&mut graph, node);
    }
    graph.nodes[1].parameters.insert("opacity".into(), set_then_return("count", Value::Float(0.8125), Value::Float(0.8125)));
    let read = FunctionGraph::input("count", materialcraft_core::ValueType::Float);
    graph.nodes[2].parameters.insert("opacity".into(), Value::Function(Box::new(read.clone())));

    let mut counter = read;
    let current = counter.output.unwrap();
    let step = counter.add_constant(Value::Float(0.0625));
    let next = counter.add("add", &[current, step], None);
    counter.output = Some(counter.add("set", &[next], Some(Value::Text("count".into()))));
    graph.nodes[3].parameters.insert("opacity".into(), Value::Function(Box::new(counter)));

    let image = cook(graph).unwrap().remove(0);
    for (x, y, expected) in [(4, 4, 0.8125), (12, 4, 0.25), (4, 12, 0.875), (12, 12, 0.9375)] {
        assert_eq!(image.get_gray(x, y, false), expected);
    }
}

#[test]
fn six_level_quadrant_children_start_independent_counters() {
    for depth in [5, 6] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        for level in 0..depth {
            let next = graph.add(PatternNodeKind::Quadrant).unwrap();
            graph.nodes[level].children.fill(Some(next));
        }
        square(&mut graph, depth);
        let mut counter = FunctionGraph::input("count", materialcraft_core::ValueType::Float);
        let previous = counter.output.unwrap();
        let step = counter.add_constant(Value::Float(1.0 / 4096.0));
        let next = counter.add("add", &[previous, step], None);
        counter.output = Some(counter.add("set", &[next], Some(Value::Text("count".into()))));
        graph.nodes[depth].parameters.insert("opacity".into(), Value::Function(Box::new(counter)));

        let side = 1 << depth;
        let image = cook_at_size(graph, &BTreeMap::new(), 598241, side).unwrap().remove(0);
        let half = i64::from(side / 2);
        for slot in 0..4 {
            let x = slot % 2 * half;
            let y = slot / 2 * half;
            let first = if depth == 5 { 256 * slot + 1 } else { 1 };
            assert_eq!(image.get_gray(x, y, false), first as f32 / 4096.0, "depth {depth}, child {slot}");
            assert_eq!(image.get_gray(x + 1, y, false), (first + 1) as f32 / 4096.0);
        }
    }
}

#[test]
fn quadrant_modes_read_published_inputs_and_shadow_outer_pattern_bindings() {
    use materialcraft_core::ValueType;

    let direct = FunctionGraph::input("form", ValueType::Integer);
    let mut add_two = direct.clone();
    let read = add_two.output.unwrap();
    let two = add_two.add_constant(Value::Int(2));
    add_two.output = Some(add_two.add("add", &[read, two], None));
    for (shadowed, expression, expected_corner) in [(false, direct.clone(), 1.0), (true, direct, 0.0), (true, add_two, 1.0)] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].children[0] = Some(child);
        if shadowed {
            graph.nodes[0].parameters.insert("numberadded".into(), set_then_return("form", Value::Int(3), Value::Int(0)));
        }
        graph.nodes[1].parameters.insert("patterntype".into(), Value::Function(Box::new(expression)));
        let image = cook_with_variables(graph, &BTreeMap::from([("form".into(), Value::Int(2))]), 17).unwrap().remove(0);
        assert_eq!(image.get_gray(0, 0, false), expected_corner, "shadowed {shadowed}");
        if expected_corner == 0.0 {
            assert_eq!(image.get_gray(8, 8, false), 0.0);
        }
    }
}

#[test]
fn a_quadrant_mode_evaluates_before_it_has_a_traversal_depth() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let child = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children[0] = Some(child);
    let mut shape = FunctionGraph::default();
    let extent = shape.add("get_float1", &[], Some(Value::Text("$depthpow2".into())));
    let threshold = shape.add_constant(Value::Float(0.25));
    let test = shape.add("gt", &[extent, threshold], None);
    let disc = shape.add_constant(Value::Int(3));
    let square = shape.add_constant(Value::Int(2));
    shape.output = Some(shape.add("ifelse", &[test, disc, square], None));
    graph.nodes[1].parameters.insert("patterntype".into(), Value::Function(Box::new(shape)));

    let image = cook(graph).unwrap().remove(0);
    assert_eq!(image.get_gray(0, 0, false), 1.0);
}

#[test]
fn a_quadrant_only_publishes_writes_from_active_stamp_properties() {
    for (pattern, property, result, expected) in [
        (0, "opacity", Value::Float(0.0), 0.125),
        (0, "branchoffset", Value::Float2([0.0; 2]), 0.75),
        (2, "opacity", Value::Float(0.0), 0.75),
        (2, "imageindex", Value::Int(0), 0.125),
        (2, "patterntype", Value::Int(2), 0.125),
        (2, "blendingmode", Value::Int(0), 0.125),
        (1, "imagefiltering", Value::Int(1), 0.125),
        (1, "imagepremul", Value::Bool(true), 0.125),
        (2, "randominherited", Value::Bool(true), 0.125),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].children[0] = Some(child);
        let parent = &mut graph.nodes[0].parameters;
        parent.insert("patterntype".into(), Value::Int(pattern));
        parent.insert("opacity".into(), Value::Float(0.0));
        let mut expression = FunctionGraph::default();
        let value = expression.add_constant(Value::Float(0.75));
        let write = expression.add("set", &[value], Some(Value::Text("marker".into())));
        let result = expression.add_constant(result);
        expression.output = Some(expression.add("sequence", &[write, result], None));
        parent.insert(property.into(), Value::Function(Box::new(expression)));
        graph.nodes[1].parameters.insert("patterntype".into(), Value::Int(2));
        graph.nodes[1].parameters.insert("opacity".into(), Value::Function(Box::new(FunctionGraph::input("marker", materialcraft_core::ValueType::Float))));
        let image = cook_with_variables(graph, &BTreeMap::from([("marker".into(), Value::Float(0.125))]), 19).unwrap().remove(0);
        assert_eq!(image.get_gray(3, 3, false), expected, "pattern {pattern}, property {property}");
    }
}

#[test]
fn an_empty_switch_still_runs_its_seed_assignment() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let repeat = graph.add(PatternNodeKind::Iterate).unwrap();
    let empty = graph.add(PatternNodeKind::Switch).unwrap();
    let mark = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children = vec![Some(repeat), Some(repeat), Some(mark), None];
    graph.nodes[1].parameters.insert("numberadded".into(), Value::Int(2));
    graph.nodes[1].children[1] = Some(empty);
    let mut advance = FunctionGraph::default();
    let count = advance.add("get_float1", &[], Some(Value::Text("count".into())));
    let quarter = advance.add_constant(Value::Float(0.25));
    let next = advance.add("add", &[count, quarter], None);
    let set = advance.add("set", &[next], Some(Value::Text("count".into())));
    let zero = advance.add_constant(Value::Int(0));
    advance.output = Some(advance.add("sequence", &[set, zero], None));
    graph.nodes[2].parameters.insert("randomseed".into(), Value::Function(Box::new(advance)));
    graph.nodes[3].parameters.insert("patterntype".into(), Value::Int(2));
    graph.nodes[3].parameters.insert("opacity".into(), Value::Function(Box::new(FunctionGraph::input("count", materialcraft_core::ValueType::Float))));
    assert_eq!(cook(graph).unwrap()[0].get_gray(3, 11, false), 1.0);
}

#[test]
fn iterate_preserves_assignments_from_previous_iterations() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let child = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children = vec![None, Some(child)];
    let mut initialize = FunctionGraph::default();
    let zero = initialize.add_constant(Value::Float(0.0));
    let count = initialize.add_constant(Value::Int(3));
    let set = initialize.add("set", &[zero], Some(Value::Text("counter".into())));
    initialize.output = Some(initialize.add("sequence", &[set, count], None));
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Function(Box::new(initialize)));
    let mut increment = FunctionGraph::default();
    let value = increment.add("get_float1", &[], Some(Value::Text("counter".into())));
    let one = increment.add_constant(Value::Float(1.0));
    let next = increment.add("add", &[value, one], None);
    increment.output = Some(increment.add("set", &[next], Some(Value::Text("counter".into()))));
    graph.nodes[1].parameters.insert("patterntype".into(), Value::Int(2));
    graph.nodes[1].parameters.insert("opacity".into(), Value::Function(Box::new(increment)));
    let image = cook(graph).unwrap().remove(0);

    assert!(image.data().iter().all(|value| *value == 6.0), "expected 1 + 2 + 3, got {:?}", image.mean());
}

#[test]
fn an_iterate_keeps_the_initial_seed_and_advances_each_repetition() {
    for (slot, count, expected) in [(0, 1, 0x3f042a5e), (1, 1, 0x3f1abd9c), (1, 2, 0x3f2b7424)] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].children[slot] = Some(child);
        graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(count));
        let params = &mut graph.nodes[1].parameters;
        params.insert("patterntype".into(), Value::Int(2));
        params.insert("blendingmode".into(), Value::Int(2));
        let mut draw = FunctionGraph::default();
        let one = draw.add_constant(Value::Float(1.0));
        draw.output = Some(draw.add("rand", &[one], None));
        params.insert("opacity".into(), Value::Function(Box::new(draw)));
        let image = cook(graph).unwrap().remove(0);
        assert_eq!(image.get(8, 8, false)[0].to_bits(), expected);
    }
}

#[test]
fn driven_scalar_arithmetic_in_an_iterate_seed_uses_storage_words() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Iterate);
    let square = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children[1] = Some(square);
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(1));
    let mut tint = FunctionGraph::default();
    let unit = tint.add_constant(Value::Float(1.0));
    tint.output = Some(tint.add("rand", &[unit], None));
    graph.nodes[1].parameters = BTreeMap::from([
        ("patterntype".into(), Value::Int(2)),
        ("blendingmode".into(), Value::Int(0)),
        ("patternsize".into(), Value::Float2([1.0; 2])),
        ("opacity".into(), Value::Function(Box::new(tint))),
    ]);

    let cases = [
        (0.0, 1.0, 0, [16, 16]),
        (1.0, 0.5, 845872, [16, 32]),
        (3.0, 0.75, 113, [32, 16]),
        (5.25, 0.5, 845872, [16, 16]),
        (17.0, 1.0, 42, [16, 32]),
        (-1.25, -0.5, 37, [32, 16]),
    ];
    for (expression, expected) in [
        ("cast", [0x3f4bd028, 0x3f2676b8, 0x3ef06e34, 0x3f833b5c, 0x3f7d60e6, 0x3ee3256c]),
        ("float sum", [0x3e97a050, 0x3f2676b8, 0x3e60dc68, 0x3f5e76b8, 0x3f6560e6, 0x3ea3256c]),
        ("integer sum", [0x3e97a050, 0x3f2676b8, 0x3ef06e34, 0x3f833b5c, 0x3efac1cc, 0x3f7192b6]),
    ] {
        let mut seed = FunctionGraph::default();
        let first = seed.add("get_float1", &[], Some(Value::Text("first".into())));
        let second = seed.add("get_float1", &[], Some(Value::Text("second".into())));
        seed.output = Some(match expression {
            "float sum" => {
                let sum = seed.add("add", &[first, second], None);
                seed.add("toint1", &[sum], None)
            }
            "integer sum" => {
                let left = seed.add("toint1", &[first], None);
                let right = seed.add("toint1", &[second], None);
                seed.add("add", &[left, right], None)
            }
            _ => seed.add("toint1", &[first], None),
        });
        graph.nodes[0].parameters.insert("randomseed".into(), Value::Function(Box::new(seed)));
        for ((first, second, parent_seed, size), bits) in cases.into_iter().zip(expected) {
            let vars = BTreeMap::from([("first".into(), Value::Float(first)), ("second".into(), Value::Float(second))]);
            let image = cook_in_regions(graph.clone(), &vars, parent_seed, size, [[0.0, 1.0, 0.0, 1.0]; 2], 0.125).unwrap().remove(0);
            assert_eq!(image.get_gray(8, 8, false).to_bits(), bits, "{expression}, {first}, {second}");
        }
    }
}

#[test]
fn a_pattern_seed_expression_runs_before_the_stamp_properties() {
    let mut seed = FunctionGraph::default();
    let one = seed.add_constant(Value::Float(1.0));
    let draw = seed.add("rand", &[one], None);
    let save = seed.add("set", &[draw], Some(Value::Text("seed_draw".into())));
    let local = seed.add_constant(Value::Int(43));
    seed.output = Some(seed.add("sequence", &[save, local], None));
    for (inherits, scope_value, expected) in [(true, false, 0x3f4fd146), (false, false, 0x3e98dd54), (true, true, 0x3ed93158), (false, true, 0x3ed93158)] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        let params = &mut graph.nodes[0].parameters;
        params.insert("randominherited".into(), Value::Bool(inherits));
        params.insert("randomseed".into(), Value::Function(Box::new(seed.clone())));
        params.insert("patterntype".into(), Value::Int(2));
        let mut opacity = FunctionGraph::default();
        let one = opacity.add_constant(Value::Float(1.0));
        opacity.output =
            Some(if scope_value { opacity.add("get_float1", &[], Some(Value::Text("seed_draw".into()))) } else { opacity.add("rand", &[one], None) });
        params.insert("opacity".into(), Value::Function(Box::new(opacity)));
        let image = cook_with_variables(graph, &BTreeMap::new(), 7).unwrap().remove(0);
        assert_eq!(image.get(8, 8, false)[0].to_bits(), expected);
    }
}

#[test]
fn a_pattern_uses_the_seed_without_exposing_a_variable_for_it() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    let params = &mut graph.nodes[0].parameters;
    params.insert("patterntype".into(), Value::Int(2));
    params.insert("randomseed".into(), Value::Int(43));
    let mut opacity = FunctionGraph::default();
    let variable = Value::Text("$randomseed".into());
    let exists = opacity.add("isdefined", &[], Some(variable.clone()));
    let value = opacity.add("get_integer1", &[], Some(variable));
    let numeric = opacity.add("tofloat", &[value], None);
    let fallback = opacity.add_constant(Value::Float(0.25));
    opacity.output = Some(opacity.add("ifelse", &[exists, numeric, fallback], None));
    params.insert("opacity".into(), Value::Function(Box::new(opacity)));
    let image = cook_with_variables(graph, &BTreeMap::new(), 7).unwrap().remove(0);
    assert_eq!(image.get(8, 8, false)[0], 0.25);
}

#[test]
fn quadrant_properties_draw_from_independent_streams() {
    for (property, setting, expected) in [
        ("branchoffset", Value::Float2([0.0; 2]), 0x3ee082ec),
        ("frameoffset", Value::Float2([0.0; 2]), 0x3f11adbc),
        ("patternsize", Value::Float2([0.25; 2]), 0x3f0db18c),
        ("patternrotation", Value::Float(0.0), 0x3f30ca04),
        ("patternsuppl", Value::Float(0.5), 0x3ad62400),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[0].children[0] = Some(child);
        let parent = &mut graph.nodes[0].parameters;
        parent.insert("patterntype".into(), Value::Int(2));
        parent.insert("opacity".into(), Value::Float(0.0));
        let mut write = FunctionGraph::default();
        let one = write.add_constant(Value::Float(1.0));
        let draw = write.add("rand", &[one], None);
        let store = write.add("set", &[draw], Some(Value::Text("measured".into())));
        let setting = write.add_constant(setting);
        write.output = Some(write.add("sequence", &[store, setting], None));
        parent.insert(property.into(), Value::Function(Box::new(write)));
        let mut read = FunctionGraph::default();
        read.output = Some(read.add("get_float1", &[], Some(Value::Text("measured".into()))));
        let leaf = &mut graph.nodes[1].parameters;
        leaf.insert("patterntype".into(), Value::Int(2));
        leaf.insert("opacity".into(), Value::Function(Box::new(read)));
        let image = cook_with_variables(graph, &BTreeMap::new(), 42).unwrap().remove(0);
        assert_eq!(image.get(4, 4, false)[0].to_bits(), expected, "{property}");
    }
}

#[test]
fn patterns_reject_cycles_and_bound_exponential_traversal() {
    let mut graph = PatternGraph::default();
    let root = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.nodes[0].children[0] = Some(root);
    assert!(cook(graph).unwrap_err().contains("cycle"));

    let mut graph = PatternGraph::default();
    for _ in 0..12 {
        graph.add(PatternNodeKind::Quadrant).unwrap();
    }
    for index in 0..11 {
        graph.nodes[index].children = vec![Some(graph.nodes[index + 1].id); 4];
    }
    assert!(cook(graph).unwrap_err().contains("budget"));
}

#[test]
fn deep_quadrants_draw_only_the_leaves_covering_output_samples() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    for depth in 0..10 {
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[depth].children.fill(Some(child));
    }
    let increment = 1.0 / 1_048_576.0;
    graph.nodes[10].parameters = BTreeMap::from([("patterntype".into(), Value::Int(2)), ("opacity".into(), Value::Float(increment))]);
    let image = cook_at_size(graph.clone(), &BTreeMap::new(), 107185, 4).unwrap().remove(0);
    assert_eq!(image.data(), &[increment; 16]);
    let expanded = [-0.5, 1.5, -0.5, 1.5];
    let image = cook_in_regions(graph, &BTreeMap::new(), 417341, [4; 2], [expanded; 2], 0.0).unwrap().remove(0);
    assert_eq!(image.data(), &[0.0, 0.0, 0.0, 0.0, 0.0, increment, increment, 0.0, 0.0, increment, increment, 0.0, 0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn image_selectors_use_zero_based_slots_and_override_packed_patterns() {
    use craft_texture::{Image, Kind};

    let gray = |v| Some(Image::filled(4, 4, Kind::Gray, [v; 4]));
    let inputs = [None, gray(0.25), gray(0.75), None, gray(0.5)];
    let definition = find("fx_map").unwrap();
    for (pattern, slot, computed, expected) in [
        (1, None, false, 0.25),
        (1, Some(0), false, 0.25),
        (1, Some(1), false, 0.75),
        (1, Some(2), false, 0.0),
        (256, Some(1), false, 0.75),
        (257, None, false, 0.75),
        (258, Some(1), false, 0.75),
        (257, Some(-1), false, 0.0),
        (1, Some(2), true, 0.5),
        (1, Some(12), true, 0.5),
        (257, Some(-1), true, 0.25),
        (257, Some(0), true, 0.25),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        let node = &mut graph.nodes[0];
        node.parameters.insert("patterntype".into(), Value::Int(pattern));
        let mut vars = BTreeMap::new();
        if let Some(slot) = slot {
            let selector = if computed {
                vars.insert("requested".into(), Value::Int(slot));
                let mut function = FunctionGraph::default();
                function.output = Some(function.add("get_integer1", &[], Some(Value::Text("requested".into()))));
                Value::Function(Box::new(function))
            } else {
                Value::Int(slot)
            };
            node.parameters.insert("imageindex".into(), selector);
        }
        let parameters =
            resolve_params(definition, &BTreeMap::from([("pattern".into(), Value::PatternGraph(Box::new(graph))), ("color_mode".into(), Value::Bool(false))]));
        let context = CookContext {
            input_formats: &[],
            format: materialcraft_core::OutputFormat::F32,
            w: 4,
            h: 4,
            seed: 0,
            tile: false,
            inputs: &inputs,
            params: &parameters,
            resource: &|_| None,
            vars: &vars,
        };
        let image = (definition.cook)(&context).unwrap().remove(0);
        assert_eq!(image.get(2, 2, false)[0], expected, "pattern {pattern}, slot {slot:?}");
    }
}

#[test]
fn image_indices_outside_the_supported_range_return_an_error() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters.insert("patterntype".into(), Value::Int(1));
    graph.nodes[0].parameters.insert("imageindex".into(), Value::Int(i32::MAX));
    assert!(cook(graph).unwrap_err().contains("unsupported pattern"));
}

fn render_bitmap(
    input: craft_texture::Image,
    format: materialcraft_core::OutputFormat,
    [width, height]: [u32; 2],
    stamp: &[(&str, Value)],
    controls: &[(&str, Value)],
) -> craft_texture::Image {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters.insert("patterntype".into(), Value::Int(1));
    graph.nodes[0].parameters.extend(stamp.iter().map(|(k, v)| ((*k).into(), v.clone())));
    let definition = find("fx_map").unwrap();
    let mut params = BTreeMap::from([("pattern".into(), Value::PatternGraph(Box::new(graph)))]);
    params.extend(controls.iter().map(|(k, v)| ((*k).into(), v.clone())));
    let context = CookContext {
        w: width,
        h: height,
        seed: 0,
        format: materialcraft_core::OutputFormat::F32,
        tile: false,
        input_formats: &[materialcraft_core::OutputFormat::F32, format],
        inputs: &[None, Some(input)],
        params: &resolve_params(definition, &params),
        resource: &|_| None,
        vars: &BTreeMap::new(),
    };
    (definition.cook)(&context).unwrap().remove(0)
}

#[test]
fn an_unbound_image_selector_keeps_the_fixed_slot() {
    use materialcraft_core::{OutputFormat, ValueType};

    let source = craft_texture::Image::gray(4, 4, |_, _| 0.75);
    for (pattern, offset, expected) in [(259, None, 0.0), (259, Some(0), 0.0), (1, Some(0), 0.75), (1, Some(2), 0.0)] {
        let mut selector = FunctionGraph::input("missing", ValueType::Integer);
        if let Some(offset) = offset {
            let index = selector.output.unwrap();
            let step = selector.add_constant(Value::Int(offset));
            selector.output = Some(selector.add("add", &[index, step], None));
        }
        let stamp = [("patterntype", Value::Int(pattern)), ("imageindex", Value::Function(Box::new(selector)))];
        let image = render_bitmap(source.clone(), OutputFormat::F32, [4; 2], &stamp, &[("color_mode", Value::Bool(false))]);
        assert_eq!(image.get_gray(2, 2, false), expected, "pattern {pattern}, offset {offset:?}");
    }
}

#[test]
fn grayscale_bitmap_stores_coverage_before_applying_opacity() {
    use craft_texture::Image;
    use materialcraft_core::OutputFormat;

    let values = [-0.0, 0.0, 2.0_f32.powi(-30), -2.0_f32.powi(-30), 2.0_f32.powi(-24), -2.0_f32.powi(-24), 0.125, -0.125];
    let source = Image::gray(16, 16, |x, _| values[(x % 8) as usize]);
    let stamp = [("opacity", Value::Float4([0.75, 0.75, 0.75, 1.0])), ("blendingmode", Value::Int(1)), ("imagefiltering", Value::Int(2))];
    for (gain, expected) in
        [(-1.0, [-0.0, -0.0, -0.0, -0.0, -0.75 * 2.0_f32.powi(-24), -0.0, -0.09375, 0.09375]), (0.0, [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.0])]
    {
        let controls = [("color_mode", Value::Bool(false)), ("background", Value::Color([-4.0; 4])), ("opacity", Value::Float(gain))];
        let image = render_bitmap(source.clone(), OutputFormat::F32, [16; 2], &stamp, &controls);
        let bits: [u32; 8] = std::array::from_fn(|x| image.get_gray(x as i64, 0, false).to_bits());
        assert_eq!(bits, expected.map(f32::to_bits));
    }

    let source = Image::gray(16, 16, |x, _| if x % 2 == 0 { 16_777_218.0 } else { -16_777_218.0 });
    for (gain, x, expected) in [(0.6137, 0, 0x4aeb_a92a), (-1.1875, 1, 0x4b64_0004)] {
        let controls = [("color_mode", Value::Bool(false)), ("background", Value::Color([-4.0; 4])), ("opacity", Value::Float(gain))];
        let image = render_bitmap(source.clone(), OutputFormat::F32, [16; 2], &stamp, &controls);
        assert_eq!(image.get_gray(x, 0, false).to_bits(), expected);
    }
}

#[test]
fn bitmap_stamp_filters_use_the_selected_source_level() {
    use materialcraft_core::OutputFormat;

    let input = craft_texture::Image::gray(64, 64, |x, y| ((47 * x + 31 * y + 7 * x * y + 13) % 256) as f32 / 255.0).with_integer_sampling(true);
    for (mode, expected) in
        [(None, [0x3f4ea0cf, 0x3ecc32cc]), (Some(0), [0x3f4ea0cf, 0x3ecc32cc]), (Some(1), [0x3f5e5ade, 0x3e2cf0ac]), (Some(2), [0x3f62e2e3, 0x3e109090])]
    {
        let mut stamp = vec![("patternsize", Value::Float2([0.375; 2]))];
        if let Some(mode) = mode {
            stamp.push(("imagefiltering", Value::Int(mode)));
        }
        let image = render_bitmap(input.clone(), OutputFormat::U8, [64; 2], &stamp, &[("color_mode", Value::Bool(false))]);
        for (x, bits) in [32, 33].into_iter().zip(expected) {
            assert_eq!(image.get_gray(x, 32, false).to_bits(), bits, "filter {mode:?}, x {x}");
        }
    }
    for (angle, expected) in [(0.0625, 0x3f2f57af), (0.125, 0x3f0b1f8b)] {
        let stamp =
            [("patternsize", Value::Float2([0.17, 1.0])), ("frameoffset", Value::Float2([0.07125, -0.0390625])), ("patternrotation", Value::Float(angle))];
        let image = render_bitmap(input.clone(), OutputFormat::U8, [64; 2], &stamp, &[("color_mode", Value::Bool(false))]);
        assert_eq!(image.get_gray(36, 29, false).to_bits(), expected, "angle {angle}");
    }
}

#[test]
fn automatic_bitmap_mips_use_texel_gradients_from_neighboring_pixels() {
    use materialcraft_core::OutputFormat;

    for (canvas, size, turn, points) in [
        ([128, 32], [0.1767934, 0.38444963], 0.00014097655, &[(64, 16, 0x3ee1_d8e2)][..]),
        ([64, 64], [0.17683654, 0.3839], 0.0005, &[(32, 32, 0x3ed0_40d0)][..]),
        ([128, 32], [0.10727861, 0.12664622], 0.48012114, &[(64, 16, 0x3ef4_f4f4)][..]),
        ([256, 128], [0.17679575, 0.24720412], 0.00014, &[(128, 64, 0x3e8b_bc8c), (128, 63, 0x3f09_4d89)][..]),
    ] {
        let input = craft_texture::Image::gray(canvas[0], canvas[1], |x, y| {
            let band = |dx, dy| (x / dx + y / dy) & 1_u32;
            let byte = 6 + 64 * band(1, 1) + 48 * band(2, 2) + 36 * band(4, 4) + 32 * band(8, 8) + 28 * band(16, 8) + 24 * band(32, 8);
            byte as f32 / 255.0
        })
        .with_integer_sampling(true);
        let stamp = [("patternsize", Value::Float2(size)), ("patternrotation", Value::Float(turn))];
        let image = render_bitmap(input, OutputFormat::U8, canvas, &stamp, &[("color_mode", Value::Bool(false))]);
        for &(x, y, expected) in points {
            assert_eq!(image.get_gray(x, y, false).to_bits(), expected, "{canvas:?} at ({x},{y})");
        }
    }
}

#[test]
fn rectangular_bitmap_rotation_crosses_the_first_mip_boundary() {
    use materialcraft_core::OutputFormat;

    let input = craft_texture::Image::gray(64, 32, |x, y| {
        let byte = (17 * x + 59 * y + 9 * x * y + 7) % 251 + 4 * u32::from((x + 3 * y) % 7 < 3);
        byte as f32 / 255.0
    })
    .with_integer_sampling(true);
    for (numerator, expected) in [(40, 0x3f1a_eb9b), (41, 0x3f1c_699c), (42, 0x3f0e_ca8f), (-41, 0x3eff_d300)] {
        let stamp = [("patternsize", Value::Float2([1.0; 2])), ("patternrotation", Value::Float(numerator as f32 / 512.0))];
        let image = render_bitmap(input.clone(), OutputFormat::U8, [64, 32], &stamp, &[("color_mode", Value::Bool(false))]);
        assert_eq!(image.get_gray(32, 16, false).to_bits(), expected, "rotation {numerator}/512");
    }
}

#[test]
fn bitmap_stamp_filters_include_the_last_interpolated_texel() {
    use materialcraft_core::OutputFormat;

    let input = craft_texture::Image::gray(32, 32, |_, _| 1.0).with_integer_sampling(true);
    let controls = [("color_mode", Value::Bool(false)), ("opacity", Value::Float(0.6875))];
    for (mode, expected) in [(1, 0x3eb0_00b0), (2, 0x3f30_0000)] {
        let stamp = [
            ("branchoffset", Value::Float2([-0.5; 2])),
            ("frameoffset", Value::Float2([-0.020_833_343, 0.4375])),
            ("patternsize", Value::Float2([0.135_416_67, 0.8125])),
            ("imagefiltering", Value::Int(mode)),
        ];
        let image = render_bitmap(input.clone(), OutputFormat::U8, [32; 2], &stamp, &controls);
        assert_eq!(image.get_gray(1, 1, false).to_bits(), expected, "filter {mode}");
    }
}

#[test]
fn bitmap_stamp_requires_an_input_matching_the_output_kind() {
    use craft_texture::{Image, Kind};
    use materialcraft_core::OutputFormat;

    let background = [0.125, 0.5, 0.875, 0.75];
    for (kind, color_output) in [(Kind::Color, false), (Kind::Gray, true)] {
        let controls = [("color_mode", Value::Bool(color_output)), ("background", Value::Color(background))];
        let image = render_bitmap(Image::filled(4, 4, kind, [1.0; 4]), OutputFormat::F32, [4; 2], &[], &controls);
        let expected = if color_output { background } else { [0.5, 0.5, 0.5, 1.0] };
        assert_eq!(image.get(2, 2, false), expected);
    }
}

#[test]
fn connected_pattern_backgrounds_prepare_small_samples_before_filtering() {
    use craft_texture::{Image, Kind};
    use materialcraft_core::OutputFormat;

    let smallest = f32::MIN_POSITIVE;
    let largest_subnormal = f32::from_bits(0x007f_ffff);
    let gray = Image::gray(2, 2, |x, _| if x == 0 { largest_subnormal } else { 2.0 * smallest });
    let underflow = Image::gray(2, 2, |x, _| if x == 0 { smallest } else { 0.0 });
    let mixed = [-0.0_f32, -0.375, 0.5, -0.0];
    let negative = [-0.0_f32, -0.375, -0.5, -0.0];
    for (input, size, source, expected) in [
        (gray, 1, OutputFormat::F32, [smallest.to_bits(); 4]),
        (underflow, 1, OutputFormat::F32, [0; 4]),
        (Image::filled(2, 2, Kind::Gray, [-largest_subnormal; 4]), 2, OutputFormat::F32, [0; 4]),
        (
            Image::filled(2, 2, Kind::Color, [-largest_subnormal, -0.0, largest_subnormal, -smallest]),
            2,
            OutputFormat::F32,
            [0x8000_0000, 0x8000_0000, 0, 0x8080_0000],
        ),
        (Image::filled(2, 2, Kind::Color, mixed), 2, OutputFormat::F32, mixed.map(f32::to_bits)),
        (Image::filled(2, 2, Kind::Color, mixed), 4, OutputFormat::F32, [0, mixed[1].to_bits(), mixed[2].to_bits(), 0]),
        (Image::filled(2, 2, Kind::Color, mixed), 4, OutputFormat::F16, [0, mixed[1].to_bits(), mixed[2].to_bits(), 0]),
        (Image::filled(2, 2, Kind::Color, negative), 4, OutputFormat::F32, negative.map(f32::to_bits)),
    ] {
        let kind = input.kind();
        let graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        let definition = find("fx_map").unwrap();
        let params = resolve_params(
            definition,
            &BTreeMap::from([("pattern".into(), Value::PatternGraph(Box::new(graph))), ("color_mode".into(), Value::Bool(kind == Kind::Color))]),
        );
        let context = CookContext {
            w: size,
            h: size,
            seed: 100237,
            format: OutputFormat::F32,
            tile: true,
            input_formats: &[source],
            inputs: &[Some(input)],
            params: &params,
            resource: &|_| None,
            vars: &BTreeMap::new(),
        };
        let image = (definition.cook)(&context).unwrap().remove(0);
        let pixel = image.get(0, 0, false).map(f32::to_bits);
        assert_eq!(&pixel[..image.channels()], &expected[..image.channels()]);
    }
}

#[test]
fn bitmap_stamp_tint_and_coverage_compose_straight_and_premultiplied_colors() {
    use craft_texture::{Image, Kind};
    use materialcraft_core::OutputFormat;

    let source = [0x3f12d2d3, 0xbe67e7e8, 0x3ef57575, 0x3f97b7b8].map(f32::from_bits);
    let controls = [("color_mode", Value::Bool(true)), ("background", Value::Color([0.25, 0.375, 0.625, 0.8125])), ("opacity", Value::Float(0.375))];
    for (blend, premultiplied, expected) in [
        (0, false, [0x3ebd2e9e, 0x3eb3eba3, 0x3f42183a, 0x3f5d55a6]),
        (1, false, [0x3e999f8f, 0x3e7d2a19, 0x3f15a567, 0x3f5d55a6]),
        (2, false, [0x3e999f8f, 0x3e7d2a19, 0x3f15a567, 0x3f5d55a6]),
        (0, true, [0x3eb39e1e, 0x3eb5cf0f, 0x3f3cc3c4, 0x3f5d55a6]),
    ] {
        let stamp = [
            ("opacity", Value::Float4([0.75, 0.375, 1.0, 0.625])),
            ("blendingmode", Value::Int(blend)),
            ("imagepremul", Value::Bool(premultiplied)),
            ("imagefiltering", Value::Int(2)),
        ];
        let image = render_bitmap(Image::filled(1, 1, Kind::Color, source), OutputFormat::F32, [1; 2], &stamp, &controls);
        assert_eq!(image.get(0, 0, false).map(f32::to_bits), expected, "blend {blend}, premultiplied {premultiplied}");
    }
}

#[test]
fn bitmap_stamp_blend_preserves_guard_bits_during_cancellation() {
    use craft_texture::{Image, Kind};
    use materialcraft_core::OutputFormat;

    let source = [0xbaf97631, 0xc061d2bc, 0xbc4a2f47, 0xbe8c58b1].map(f32::from_bits);
    let background = [0x3ac3d70f, 0x4031485b, 0x3c1eb9a8, 0x3e5c5bb0].map(f32::from_bits);
    let controls = [("color_mode", Value::Bool(true)), ("background", Value::Color(background))];
    for (mode, expected) in [
        (0, [0xb9d67c88, 0xbf422984, 0xbb2dd67c, 0x388c6000]),
        (1, [0x34f98400, 0x3a61d400, 0x364a3400, 0x388c5c00]),
        (2, [0x34f98400, 0x3a61d400, 0x364a3400, 0x388c5c00]),
    ] {
        let stamp = [("imagefiltering", Value::Int(2)), ("imagepremul", Value::Bool(true)), ("blendingmode", Value::Int(mode))];
        let image = render_bitmap(Image::filled(1, 1, Kind::Color, source), OutputFormat::F32, [1; 2], &stamp, &controls);
        assert_eq!(image.get(0, 0, false).map(f32::to_bits), expected, "blend {mode}");
    }
    let controls = [("color_mode", Value::Bool(true)), ("background", Value::Color([-0.0; 4]))];
    for mode in 0..=2 {
        let stamp = [("imagefiltering", Value::Int(2)), ("blendingmode", Value::Int(mode))];
        let image = render_bitmap(Image::filled(1, 1, Kind::Color, [0.0; 4]), OutputFormat::F32, [1; 2], &stamp, &controls);
        assert_eq!(image.get(0, 0, false).map(f32::to_bits), [(-0.0_f32).to_bits(); 4]);
    }
}

fn roughness_sample(roughness: f32, depth: usize, blend: i32, color: bool, repeat: bool) -> crate::CookResult {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    for index in 0..depth {
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[index].children[0] = Some(child);
    }
    let leaf = depth;
    let leaf_id = graph.nodes[leaf].id;
    graph.nodes[leaf].parameters.insert("patterntype".into(), Value::Int(2));
    let tint = if color { [0.8, 0.4, 0.2, 1.0] } else { [0.8, 0.8, 0.8, 1.0] };
    graph.nodes[leaf].parameters.insert("opacity".into(), Value::Float4(tint));
    graph.nodes[leaf].parameters.insert("blendingmode".into(), Value::Int(blend));
    if repeat {
        let dot = graph.add(PatternNodeKind::Dot).unwrap();
        graph.nodes.last_mut().unwrap().children[0] = Some(leaf_id);
        let switch = graph.add(PatternNodeKind::Switch).unwrap();
        graph.nodes.last_mut().unwrap().children[0] = Some(dot);
        let iterate = graph.add(PatternNodeKind::Iterate).unwrap();
        graph.nodes.last_mut().unwrap().children = vec![None, Some(switch)];
        graph.nodes.last_mut().unwrap().parameters.insert("numberadded".into(), Value::Int(2));
        if leaf == 0 {
            graph.root = Some(iterate);
        } else {
            graph.nodes[leaf - 1].children[0] = Some(iterate);
        }
    }
    let definition = find("fx_map").unwrap();
    let parameters = resolve_params(
        definition,
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph))),
            ("roughness".into(), Value::Float(roughness)),
            ("opacity".into(), Value::Float(0.5)),
            ("color_mode".into(), Value::Bool(color)),
            ("background".into(), Value::Color([0.1; 4])),
        ]),
    );
    let context = CookContext {
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        w: 16,
        h: 16,
        seed: 7,
        tile: true,
        inputs: &[],
        params: &parameters,
        resource: &|_| None,
        vars: &BTreeMap::new(),
    };

    (definition.cook)(&context)
}

#[test]
fn single_square_preserves_the_stored_opacity_at_each_depth() {
    let definition = find("fx_map").unwrap();
    let gain = f32::from_bits(0x4020_0150);
    for (last, draw, roughness, opacity, blend, bits) in [
        (5, 5, 0.95, gain, 1, 0x3eba_f23c),
        (1, 0, 0.99, 1.0, 1, 0x3f00_a4b1),
        (2, 2, 0.99, 1.0, 1, 0x3ea8_f445),
        (2, 2, 1.5, 0.6137, 1, 0x3e94_d6a2),
        (2, 1, 1.0, gain, 1, 0x3f55_5716),
        (5, 5, 4_194_304.0, 1.0, 1, 0),
        (0, 0, 0.0, -1.0, 0, 0xbf80_0000),
        (0, 0, 0.95, 327.125, 0, 0x43a3_9000),
    ] {
        let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
        for depth in 0..last {
            let child = graph.add(PatternNodeKind::Quadrant).unwrap();
            graph.nodes[depth].children[0] = Some(child);
        }
        graph.nodes[draw].parameters = BTreeMap::from([
            ("patterntype".into(), Value::Int(2)),
            ("patternsize".into(), Value::Float2([(1_u32 << draw) as f32; 2])),
            ("blendingmode".into(), Value::Int(blend)),
        ]);
        let params = resolve_params(
            definition,
            &BTreeMap::from([
                ("pattern".into(), Value::PatternGraph(Box::new(graph))),
                ("color_mode".into(), Value::Bool(false)),
                ("roughness".into(), Value::Float(roughness)),
                ("opacity".into(), Value::Float(opacity)),
            ]),
        );
        let vars = BTreeMap::new();
        let context = CookContext {
            input_formats: &[],
            format: materialcraft_core::OutputFormat::F32,
            w: 16,
            h: 16,
            seed: 7,
            tile: false,
            inputs: &[],
            params: &params,
            resource: &|_| None,
            vars: &vars,
        };
        let image = (definition.cook)(&context).unwrap().remove(0);
        assert_eq!(image.get_gray(0, 0, false).to_bits(), bits, "drawing depth {draw} of {last}, roughness {roughness}, opacity {opacity}");
    }
}

#[test]
fn roughness_attenuates_quadrants_by_depth_and_keeps_background() {
    for color in [false, true] {
        for (roughness, depth, weight) in [(0.0, 2, 1.0), (1.0, 0, 1.0), (1.0, 1, 0.5), (0.5, 2, 1.0 / 7.0), (2.0, 2, 4.0 / 7.0)] {
            for blend in 0..=2 {
                let image = roughness_sample(roughness, depth, blend, color, false).unwrap().remove(0);
                let pixel = image.get(0, 0, false);
                for (channel, foreground) in [0.8_f32, 0.4, 0.2, 1.0].into_iter().take(image.channels()).enumerate() {
                    let value = foreground * 0.5 * weight;
                    let expected = match (color, blend) {
                        (false, 1 | 2) => value.max(0.1),
                        (true, 1 | 2) => value + 0.1 * (1.0 - 0.5 * weight),
                        (true, 0) if channel == 3 => value + 0.1 * (1.0 - value),
                        _ => value + 0.1,
                    };

                    assert!((pixel[channel] - expected).abs() < 1e-6, "roughness={roughness}, depth={depth}, blend={blend}, channel={channel}: {pixel:?}");
                }
                if depth > 0 {
                    assert!((image.get(15, 15, false)[0] - 0.1).abs() < 1e-6);
                }
            }
        }
    }
}

#[test]
fn repetition_switches_and_dots_do_not_add_roughness_depth() {
    let image = roughness_sample(1.0, 2, 0, false, true).unwrap().remove(0);

    assert!((image.get(0, 0, false)[0] - (0.1 + 0.8 / 3.0)).abs() < 1e-6);
    assert!((image.get(15, 15, false)[0] - 0.1).abs() < 1e-6);
}

#[test]
fn negative_roughness_reports_an_error() {
    assert!(roughness_sample(-32.0, 5, 0, false, false).unwrap_err().contains("roughness must be finite and nonnegative"));
}

#[test]
fn a_full_nine_level_quadtree_renders_one_pattern_per_pixel() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    for index in 0..8 {
        let child = graph.add(PatternNodeKind::Quadrant).unwrap();
        graph.nodes[index].children = vec![Some(child); 4];
    }
    graph.nodes.last_mut().unwrap().parameters.insert("patterntype".into(), Value::Int(2));
    let definition = find("fx_map").unwrap();
    let parameters = resolve_params(
        definition,
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph))),
            ("roughness".into(), Value::Float(0.0)),
            ("color_mode".into(), Value::Bool(false)),
        ]),
    );
    let context = CookContext {
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        w: 256,
        h: 256,
        seed: 7,
        tile: true,
        inputs: &[],
        params: &parameters,
        resource: &|_| None,
        vars: &BTreeMap::new(),
    };

    let images = (definition.cook)(&context).unwrap();

    assert!(images[0].data().iter().all(|value| (*value - 1.0).abs() < 1e-6));
}

fn square(graph: &mut PatternGraph, node: usize) {
    graph.nodes[node].parameters.insert("patterntype".into(), Value::Int(2));
}

#[test]
fn an_iteration_reads_the_variables_the_previous_iteration_set() {
    let mut graph = PatternGraph::default();
    let iterate = graph.add(PatternNodeKind::Iterate).unwrap();
    let quadrant = graph.add(PatternNodeKind::Quadrant).unwrap();
    graph.root = Some(iterate);
    graph.nodes[0].parameters.insert("numberadded".into(), Value::Int(3));
    graph.nodes[0].children[1] = Some(quadrant);
    square(&mut graph, 1);

    let mut function = FunctionGraph::default();
    let total = function.add("get_float1", &[], Some(Value::Text("total".into())));
    let step = function.add_constant(Value::Float(0.25));
    let next = function.add("add", &[total, step], None);
    function.output = Some(function.add("set", &[next], Some(Value::Text("total".into()))));
    graph.nodes[1].parameters.insert("opacity".into(), Value::Function(Box::new(function)));

    for published in [0.0, 0.125] {
        let image = cook_with_variables(graph.clone(), &BTreeMap::from([("total".into(), Value::Float(published))]), 7).unwrap().remove(0);
        // The pattern declares a local accumulator. Its three visits add 0.25, 0.5 and 0.75.
        assert_eq!(image.mean()[0], 1.5, "published {published}");
    }
}

#[test]
fn the_render_region_is_the_part_of_pattern_space_the_image_shows() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    square(&mut graph, 0);
    let definition = find("fx_map").unwrap();
    let parameters = resolve_params(
        definition,
        &BTreeMap::from([("pattern".into(), Value::PatternGraph(Box::new(graph))), ("render_region".into(), Value::Float4([0.0, 2.0, 0.0, 2.0]))]),
    );
    let context = CookContext {
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        w: 16,
        h: 16,
        seed: 7,
        tile: false,
        inputs: &[],
        params: &parameters,
        resource: &|_| None,
        vars: &BTreeMap::new(),
    };

    let image = (definition.cook)(&context).unwrap().remove(0);

    // The unit square fills the top-left quarter of a region twice as wide and tall
    assert!((image.mean()[0] - 0.25).abs() < 1e-5, "{}", image.mean()[0]);
    assert_eq!(image.get(3, 3, false)[0], 1.0);
    assert_eq!(image.get(12, 12, false)[0], 0.0);
}

#[test]
fn reversing_a_render_interval_selects_the_full_extent_of_that_axis() {
    for (bounds, first_center) in [
        ([0.8125, 0.1875, 0.25, 0.75], [0.0078125, 0.25390625]),
        ([0.1875, 0.8125, 0.75, 0.25], [0.19238281, 0.0078125]),
        ([0.8125, 0.1875, 0.75, 0.25], [0.0078125; 2]),
    ] {
        let region = super::region::Region::new(bounds).unwrap();
        assert_eq!(region.point([0, 0], [64, 64]), first_center);
    }
}

#[test]
fn every_visit_draws_its_own_random_numbers() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters.insert("patterntype".into(), Value::Int(0));
    let child = graph.add(PatternNodeKind::Quadrant).unwrap();
    for slot in 0..4 {
        graph.nodes[0].children[slot] = Some(child);
    }
    square(&mut graph, 1);
    let mut function = FunctionGraph::default();
    let range = function.add_constant(Value::Float(1.0));
    function.output = Some(function.add("rand", &[range], None));
    graph.nodes[1].parameters.insert("opacity".into(), Value::Function(Box::new(function)));

    let image = cook(graph).unwrap().remove(0);

    let quarters = [image.get(4, 4, false)[0], image.get(12, 4, false)[0], image.get(4, 12, false)[0], image.get(12, 12, false)[0]];

    assert!(quarters.windows(2).all(|pair| (pair[0] - pair[1]).abs() > 1e-4), "{quarters:?}");
}

#[test]
fn a_pixel_on_the_seam_of_two_patterns_is_drawn_once() {
    let mut graph = PatternGraph::with_root(PatternNodeKind::Quadrant);
    graph.nodes[0].parameters.insert("patterntype".into(), Value::Int(0));
    let child = graph.add(PatternNodeKind::Quadrant).unwrap();
    let grandchild = graph.add(PatternNodeKind::Quadrant).unwrap();
    for slot in 0..4 {
        graph.nodes[0].children[slot] = Some(child);
        graph.nodes[1].children[slot] = Some(grandchild);
    }
    graph.nodes[1].parameters.insert("patterntype".into(), Value::Int(0));
    square(&mut graph, 2);
    let definition = find("fx_map").unwrap();
    let parameters = resolve_params(
        definition,
        &BTreeMap::from([
            ("pattern".into(), Value::PatternGraph(Box::new(graph))),
            ("roughness".into(), Value::Float(0.0)),
            ("color_mode".into(), Value::Bool(false)),
        ]),
    );
    let variables = BTreeMap::new();

    // Two pixels a side put every pixel center on the corner of four of the sixteen squares
    let context = CookContext {
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        w: 2,
        h: 2,
        seed: 7,
        tile: true,
        inputs: &[],
        params: &parameters,
        resource: &|_| None,
        vars: &variables,
    };

    let image = (definition.cook)(&context).unwrap().remove(0);

    assert_eq!(image.get(0, 0, false)[0], 1.0);
    assert_eq!(image.get(1, 1, false)[0], 1.0);
}
