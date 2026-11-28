use craft_texture::{Image, Kind};
use materialcraft_core::{FunctionGraph, Value};
use std::collections::{BTreeMap, BTreeSet};

use super::{EvaluationContext, EvaluationState, FUNCTIONS, compile, default_value};

fn evaluate(graph: &FunctionGraph, variables: &BTreeMap<String, Value>) -> Result<Value, String> {
    let image = Image::filled(3, 5, Kind::Color, [0.25, 0.5, 0.75, 1.0]);
    let context = EvaluationContext {
        image_formats: &[],
        scope: Default::default(),
        position: [0.5; 2],
        size: [3.0, 5.0],
        images: &[Some(image)],
        tile: true,
        variables,
    };
    compile(graph, 7)?.evaluate(&context, &mut EvaluationState::default())
}

fn operation(name: &str, arguments: &[Value], setting: Option<Value>) -> Result<Value, String> {
    let mut graph = FunctionGraph::default();
    let inputs: Vec<_> = arguments.iter().map(|value| graph.add_constant(value.clone())).collect();
    graph.output = Some(graph.add(name, &inputs, setting));
    evaluate(&graph, &BTreeMap::new())
}

#[test]
fn bound_pattern_arithmetic_preserves_nested_arguments_and_shared_random_draws() {
    use super::{FunctionScope, PatternExpression};
    use crate::random::FunctionRandom;
    use materialcraft_core::{GraphInstance, GraphPort, ValueType};

    fn call(graph: &mut FunctionGraph, body: FunctionGraph, wires: &[(&str, u32)]) -> u32 {
        let inputs: Vec<_> = wires.iter().map(|&(_, id)| id).collect();
        let id = graph.add("instance", &inputs, None);
        let node = graph.nodes.last_mut().unwrap();
        let port = |name: &str| GraphPort { identifier: name.into(), type_code: ValueType::Float as u32 };
        node.instance = Some(GraphInstance {
            graph: "sample".into(),
            label: String::new(),
            package: String::new(),
            inputs: wires.iter().map(|&(name, _)| port(name)).collect(),
            outputs: vec![port("output")],
            defaults: wires.iter().map(|&(name, _)| (name.into(), Value::Float(0.0))).collect(),
        });
        node.source = Some(Ok(std::sync::Arc::new(body)));
        id
    }

    let mut leaf = FunctionGraph::input("gain", ValueType::Float);
    let gain = leaf.output.unwrap();
    leaf.output = Some(leaf.add("rand", &[gain], None));
    let mut wrapper = FunctionGraph::default();
    let draw = call(&mut wrapper, leaf, &[]);
    wrapper.output = Some(wrapper.add("add", &[draw, draw], None));
    let mut graph = FunctionGraph::default();
    let range = graph.add_constant(Value::Float(0.75));
    let gain = graph.add("rand", &[range], None);
    let twice = call(&mut graph, wrapper, &[("gain", gain)]);
    graph.output = Some(graph.add("vector2", &[twice, gain], None));

    let program = compile(&graph, 0).unwrap();
    let variables = BTreeMap::from([("gain".into(), Value::Float(0.125))]);
    let assigned = BTreeSet::from(["gain"]);
    let context = EvaluationContext {
        scope: FunctionScope::FxMap,
        variables: &variables,
        position: [0.5; 2],
        size: [16.0; 2],
        tile: true,
        images: &[],
        image_formats: &[],
    };
    let mut bound = PatternExpression::bind(&program, &variables, &assigned, None).unwrap();
    for seed in [0, 19, 105_532] {
        let mut original = FunctionRandom::image(seed);
        let mut compact = original;
        let expected = program.evaluate_with_random(&context, &mut EvaluationState::default(), &mut original).unwrap();
        assert_eq!(bound.evaluate(&mut compact).unwrap(), (expected, None));
        assert_eq!(compact.draw().to_bits(), original.draw().to_bits());
    }

    // A free read of the same mutable name and a per-visit position both need live bindings.
    for (name, kind) in [("gain", ValueType::Float), ("$pos", ValueType::Float2), ("$number", ValueType::Float)] {
        let program = compile(&FunctionGraph::input(name, kind), 0).unwrap();
        assert!(PatternExpression::bind(&program, &variables, &assigned, None).is_none());
    }
}

#[test]
fn seeded_vector_modulo_preserves_the_period_and_random_stream() {
    use super::{FunctionScope, PatternExpression};
    use crate::random::FunctionRandom;

    let mut graph = FunctionGraph::default();
    let range = graph.add_constant(Value::Float(1.0));
    let draw = graph.add("rand", &[range], None);
    let pair = graph.add("vector2", &[draw, draw], None);
    let offset = graph.add_constant(Value::Float2([0.8125, 0.21875]));
    let shifted = graph.add("sub", &[pair, offset], None);
    let period = graph.add_constant(Value::Float2([0.375, -0.1875]));
    graph.output = Some(graph.add("mod", &[shifted, period], None));
    let program = compile(&graph, 0).unwrap();
    let variables = BTreeMap::new();
    let context = EvaluationContext {
        scope: FunctionScope::FxMap,
        variables: &variables,
        position: [0.5; 2],
        size: [16.0; 2],
        tile: true,
        images: &[],
        image_formats: &[],
    };
    let mut bound = PatternExpression::bind(&program, &variables, &BTreeSet::new(), None).expect("seeded vector modulo should bind");
    for seed in [0, 19, 42, 329_779] {
        let mut normal = FunctionRandom::image(seed);
        let mut prepared = normal;
        let value = program.evaluate_with_random(&context, &mut EvaluationState::default(), &mut normal).unwrap();
        let Value::Float2([x, y]) = value else { panic!("expected a two-component offset") };
        assert!((0.0..0.375).contains(&x) && y > -0.1875 && y <= 0.0);
        assert_eq!(bound.evaluate(&mut prepared).unwrap(), (value, None));
        assert_eq!(prepared.draw().to_bits(), normal.draw().to_bits());
    }
}

#[test]
fn folded_integers_use_word_order_and_convert_unsigned_magnitudes() {
    let variables = BTreeMap::from([("lhs".into(), Value::Int(-5)), ("rhs".into(), Value::Int(2))]);
    for (left_literal, right_literal) in [(true, true), (true, false), (false, true), (false, false)] {
        let mut graph = FunctionGraph::default();
        let mut operand = |name: &str, value, literal| {
            if literal { graph.add_constant(Value::Int(value)) } else { graph.add("get_integer1", &[], Some(Value::Text(name.into()))) }
        };
        let left = operand("lhs", -5, left_literal);
        let right = operand("rhs", 2, right_literal);
        let unsigned = left_literal && right_literal;
        for (op, expected) in [("lr", !unsigned), ("lreq", !unsigned), ("gt", unsigned), ("gteq", unsigned)] {
            graph.output = Some(graph.add(op, &[left, right], None));
            assert_eq!(evaluate(&graph, &variables).unwrap(), Value::Bool(expected), "{op} with literal operands {left_literal}/{right_literal}");
        }
        graph.output = Some(graph.add("tofloat", &[left], None));
        assert_eq!(evaluate(&graph, &variables).unwrap(), Value::Float(if left_literal { 4_294_967_296.0 } else { -5.0 }));
    }
}

#[test]
fn variable_reads_consume_one_reference_marker() {
    let variables = BTreeMap::from([("level".into(), Value::Float(0.1875))]);
    for (assigned, getter, expected) in
        [("level", "%level", 0.6875), ("%level", "level", 0.1875), ("%level", "%level", 0.1875), ("%level", "%%level", 0.6875), ("level", "%%level", 0.0)]
    {
        let mut graph = FunctionGraph::default();
        let value = graph.add_constant(Value::Float(0.6875));
        let set = graph.add("set", &[value], Some(Value::Text(assigned.into())));
        let get = graph.add("get_float1", &[], Some(Value::Text(getter.into())));
        graph.output = Some(graph.add("sequence", &[set, get], None));
        assert_eq!(evaluate(&graph, &variables), Ok(Value::Float(expected)), "{assigned} then {getter}");
    }

    let mut graph = FunctionGraph::default();
    graph.output = Some(graph.add("isdefined", &[], Some(Value::Text("%level".into()))));
    assert_eq!(evaluate(&graph, &variables), Ok(Value::Bool(false)));
    graph.output = Some(graph.add("get_float2", &[], Some(Value::Text("%$size".into()))));
    assert_eq!(evaluate(&graph, &variables), Ok(Value::Float2([3.0, 5.0])));
}

fn pixel_operation(op: &str, operands: &[Value], literal: bool) -> Value {
    scoped_operation(super::FunctionScope::PixelProcessor, op, operands, literal)
}

fn scoped_operation(scope: super::FunctionScope, op: &str, operands: &[Value], literal: bool) -> Value {
    let mut graph = FunctionGraph::default();
    let mut variables = BTreeMap::new();
    let mut inputs = Vec::new();
    for (i, operand) in operands.iter().enumerate() {
        let input = if literal {
            graph.add_constant(operand.clone())
        } else {
            let name = format!("input{i}");
            variables.insert(name.clone(), operand.clone());
            graph.add(&format!("get_float{}", super::value_type::dimensions(operand)), &[], Some(Value::Text(name)))
        };
        inputs.push(input);
    }
    graph.output = Some(graph.add(op, &inputs, None));
    let context = EvaluationContext { image_formats: &[], scope, position: [0.5; 2], size: [1.0; 2], images: &[], tile: true, variables: &variables };
    compile(&graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap()
}

#[test]
fn floating_modulo_wraps_a_rounded_period_to_zero() {
    let gap = 1.0 - 1.0_f32.next_down();
    let inputs = [Value::Float4([-gap * 0.5, gap * 0.25, -gap * 0.75, gap]), Value::Float4([1.0, -1.0, 1.0, -1.0])];
    for scope in [super::FunctionScope::FxMap, super::FunctionScope::Node, super::FunctionScope::PixelProcessor] {
        assert_eq!(scoped_operation(scope, "mod", &inputs, false), Value::Float4([0.0, 0.0, 1.0 - gap, -1.0 + gap]));
        assert_eq!(scoped_operation(scope, "mod", &inputs, true), Value::Float4([1.0, -1.0, 1.0 - gap, -1.0 + gap]));
    }
}

#[test]
fn runtime_arithmetic_flushes_subnormal_operands_and_results_with_their_signs() {
    for scope in [super::FunctionScope::Node, super::FunctionScope::ValueProcessor, super::FunctionScope::PixelProcessor, super::FunctionScope::FxMap] {
        for (op, a, b, expected) in [
            ("add", 0x00800001, 0x80800000, 0),
            ("sub", 0x00800001, 0x00800000, 0),
            ("mul", 0x807fffff, 0x40000000, 0x80000000),
            ("mul", 0x00800000, 0x3f000000, 0),
            ("div", 0x00800000, 0x40000000, 0),
            ("div", 0x00800000, 0x807fffff, f32::NEG_INFINITY.to_bits()),
        ] {
            let args = [Value::Float(f32::from_bits(a)), Value::Float(f32::from_bits(b))];
            let actual = scoped_operation(scope, op, &args, false);
            assert!(matches!(actual, Value::Float(value) if value.to_bits() == expected), "{scope:?} {op}: {actual:?}");
        }
    }
    let a = Value::Float2([f32::from_bits(0x00400000), f32::MIN_POSITIVE]);
    let b = Value::Float2([f32::from_bits(0x00400000), 0.5]);
    assert_eq!(pixel_operation("dot", &[a, b], false), Value::Float(0.0));
}

#[test]
fn uniform_logarithms_and_powers_use_their_measured_domains() {
    for (op, input, exponent, bits) in [
        ("log", -1.0, None, 0),
        ("log", f32::from_bits(1), None, 0),
        ("log", 0.5625, None, 0xbf134b11),
        ("log", 0.75, None, 0xbe934b11),
        ("log2", -2.0, None, 0),
        ("log2", f32::from_bits(1), None, 0),
        ("log2", f32::MIN_POSITIVE, None, 0xc2fbffff),
        ("log2", 0.625, None, 0xbf2d961e),
        ("pow", 0.0, Some(-2.0), 0),
        ("pow", -2.0, Some(0.5), 0),
        ("pow", -2.0, Some(-3.0), 0xbe000000),
        ("pow", 0.3, Some(2.4), 0x3d63befe),
        ("pow", 0.563_981_1, Some(2.4), 0x3e8182ad),
        ("pow", 0.95, Some(-4.125), 0x3f9e2948),
    ] {
        let mut inputs = vec![Value::Float(input)];
        inputs.extend(exponent.map(Value::Float));
        for scope in [super::FunctionScope::Node, super::FunctionScope::ValueProcessor] {
            let Value::Float(value) = scoped_operation(scope, op, &inputs, false) else { panic!("expected a scalar") };
            assert_eq!(value.to_bits(), bits, "{op}({input}, {exponent:?})");
        }
    }
}

#[test]
fn uniform_square_root_clears_negative_and_subnormal_lanes() {
    for scope in [super::FunctionScope::Node, super::FunctionScope::ValueProcessor, super::FunctionScope::FxMap] {
        let input = Value::Float4([-0.25, -0.0, f32::from_bits(1), 1.375]);
        let Value::Float4(result) = scoped_operation(scope, "sqrt", &[input], false) else { panic!("expected four components") };
        assert_eq!(result.map(f32::to_bits), [0, 0, 0, 1.375_f32.sqrt().to_bits()], "{scope:?}");
        assert_eq!(scoped_operation(scope, "sqrt", &[Value::Float(f32::MIN_POSITIVE)], false), Value::Float(f32::from_bits(0x20000000)));
        let special = Value::Float4([f32::NAN, f32::NEG_INFINITY, f32::INFINITY, f32::from_bits(0x3f801000)]);
        let Value::Float4(result) = scoped_operation(scope, "sqrt", &[special], false) else { panic!("expected four components") };
        assert_eq!(result.map(f32::to_bits), [0, 0, f32::INFINITY.to_bits(), 0x3f800800]);
        let literal = Value::Float4([-1.0, -0.0, f32::from_bits(1), f32::from_bits(0x007fffff)]);
        let Value::Float4(result) = scoped_operation(scope, "sqrt", &[literal], true) else { panic!("expected four components") };
        assert_eq!(result.map(f32::to_bits), [0, 0x80000000, 0x1a3504f3, 0x1fffffff]);
    }
}

#[test]
fn runtime_pixel_square_root_normalizes_exponents_and_rounds_halfway_down() {
    for (input, expected) in [
        (0x3f801000, 0x3f8007ff),
        (0x40300000, 0x3fd44394),
        (0x3fe20000, 0x3faa1513),
        (0x00800000, 0x20000000),
        (0x007fffff, 0),
        (0x80000000, 0),
        (0xbf800000, 0),
        (0x7fc00000, 0),
        (0x7f800000, 0x7f800000),
    ] {
        let Value::Float(value) = pixel_operation("sqrt", &[Value::Float(f32::from_bits(input))], false) else { panic!("expected a scalar") };
        assert_eq!(value.to_bits(), expected, "sqrt({input:08x})");
    }
    let x = f32::from_bits(0x3f801000);
    assert_eq!(pixel_operation("sqrt", &[Value::Float(x)], true), Value::Float(x.sqrt()));
}

#[test]
fn runtime_pixel_natural_exponential_reduces_to_a_positive_interval() {
    for (input, expected) in [
        (2.0, 0x40ec7325),
        (f32::from_bits(0xbf317219), 0x3effffff),
        (f32::from_bits(0xc2aeac4f), 0x00800026),
        (f32::from_bits(0xc2aeac50), 0),
        (f32::from_bits(0x42b17217), 0x7f7fff84),
        (f32::from_bits(0x42b17218), 0x7f800000),
    ] {
        let Value::Float(value) = pixel_operation("exp", &[Value::Float(input)], false) else { panic!("expected a scalar") };
        assert_eq!(value.to_bits(), expected, "exp({input})");
    }
    assert_eq!(pixel_operation("pow", &[Value::Float(0.7), Value::Float(4.0)], false), Value::Float(f32::from_bits(0x3e75dccb)));
}

#[test]
fn uniform_exponential_rounds_each_product_before_adding() {
    use super::FunctionScope::{FxMap, Node, ValueProcessor};
    for scope in [Node, ValueProcessor, FxMap] {
        for (input, expected) in [(0x3d07db5a, 0x3f845114), (0x3eb3866e, 0x3fb5c18a), (0x3f25a984, 0x3ff47bcc), (0xc2b00000, 0)] {
            let Value::Float(value) = scoped_operation(scope, "exp", &[Value::Float(f32::from_bits(input))], false) else { panic!("expected a scalar") };
            assert_eq!(value.to_bits(), expected, "{scope:?} exp({input:08x})");
        }
    }
    let input = Value::Float(f32::from_bits(0x3d07db5a));
    assert_eq!(pixel_operation("exp", std::slice::from_ref(&input), false), Value::Float(f32::from_bits(0x3f845115)));
    for scope in [Node, ValueProcessor, FxMap, super::FunctionScope::PixelProcessor] {
        assert_eq!(scoped_operation(scope, "exp", std::slice::from_ref(&input), true), Value::Float(f32::from_bits(0x3f845114)));
    }
}

#[test]
fn uniform_powers_of_two_round_the_fraction_and_limit_the_exponent() {
    use super::FunctionScope::{FxMap, Node, PixelProcessor, ValueProcessor};
    for (input, expected) in [
        (-127.5, 0x00800000),
        (-126.0, 0x00800000),
        (-12.015625, 0x397d3e02),
        (-0.125, 0x3f6ac0c3),
        (-f32::MIN_POSITIVE, 0x3f7ffff4),
        (-f32::from_bits(1), 0x3f800000),
        (0.9375, 0x3ff52576),
        (16.984375, 0x47fd3e02),
        (127.5, 0x7e800000),
    ] {
        for (scope, literal) in [(Node, false), (ValueProcessor, false), (FxMap, false), (PixelProcessor, true)] {
            let result = scoped_operation(scope, "pow2", &[Value::Float(input)], literal);
            assert_eq!(result, Value::Float(f32::from_bits(expected)), "{scope:?} pow2({input}), literal={literal}");
        }
    }
    assert_eq!(pixel_operation("pow2", &[Value::Float(0.9375)], false), Value::Float(f32::from_bits(0x3ff5257e)));
    for (input, expected) in [(f32::NAN, 0x00800000), (f32::NEG_INFINITY, 0x00800000), (f32::INFINITY, 0x7e800000)] {
        assert_eq!(super::binary_exponential::uniform_exp2(input).to_bits(), expected);
    }
}

#[test]
fn uniform_logarithms_accept_exceptional_intermediate_values() {
    for (input, expected) in [(f32::NAN, 0), (f32::NEG_INFINITY, 0), (f32::INFINITY, 0x42b17218)] {
        assert_eq!(super::binary_exponential::uniform_ln(input).to_bits(), expected);
    }
}

#[test]
fn runtime_pixel_power_uses_the_natural_logarithm_and_the_sign_of_integer_powers() {
    for (base, exponent, expected) in [
        (0.7, 1.0, 0x3f333334),
        (0.7, 1.375, 0x3f1cc3e8),
        (-0.7, 3.0, 0xbeaf9db5),
        (-2.0, -1.0, 0xbf000000),
        (-2.0, 0.5, 0),
        (0.0, -1.0, 0),
        (0.0, 0.0, 0),
        (f32::MIN_POSITIVE, 1.0, 0x00800026),
    ] {
        let Value::Float(value) = pixel_operation("pow", &[Value::Float(base), Value::Float(exponent)], false) else { panic!("expected a scalar") };
        assert_eq!(value.to_bits(), expected, "{base}^{exponent}");
    }
}

#[test]
fn runtime_pixel_logarithms_and_powers_of_two() {
    for (op, input, expected) in [
        ("log2", 0.5, 0xbf7ffffe),
        ("log2", 0.7, 0xbf03bb10),
        ("log2", 1.0, 0),
        ("log2", f32::MIN_POSITIVE, 0xc2fbffff),
        ("log", 0.5, 0xbf317217),
        ("pow2", -0.5, 0x3f3504f2),
        ("pow2", 0.5, 0x3fb504f3),
        ("pow2", 1.375, 0x4025fed7),
        ("pow2", -126.0, 0x00800000),
        ("pow2", -127.0, 0),
        ("pow2", 128.0, 0x7f800000),
    ] {
        let Value::Float(value) = pixel_operation(op, &[Value::Float(input)], false) else { panic!("expected a scalar") };
        assert_eq!(value.to_bits(), expected, "{op}({input})");
    }
    for scope in [super::FunctionScope::Node, super::FunctionScope::ValueProcessor, super::FunctionScope::FxMap] {
        assert_eq!(scoped_operation(scope, "log2", &[Value::Float(0.5)], false), Value::Float(-1.0));
    }
    assert_eq!(pixel_operation("log2", &[Value::Float(0.5)], true), Value::Float(-1.0));
}

#[test]
fn runtime_pixel_sine_and_cosine_reduce_the_phase_before_interpolation() {
    use std::f32::consts::{FRAC_PI_4, TAU};

    for (angle, sine, cosine) in [
        (-0.0, 0x80000000, 0x3f800000),
        (0.625, 0x3f15c8eb, 0x3f4f9b48),
        (-FRAC_PI_4, 0xbf3504f3, 0x3f3504f2),
        (TAU, 0x80000000, 0x3f800000),
        (f32::from_bits(0xc96df2ae), 0x3e98c108, 0x3f74574d),
    ] {
        for (op, expected) in [("sin", sine), ("cos", cosine)] {
            let Value::Float(result) = pixel_operation(op, &[Value::Float(angle)], false) else { panic!("expected a scalar") };
            assert_eq!(result.to_bits(), expected, "{op}({angle})");
        }
    }
    assert_eq!(pixel_operation("sin", &[Value::Float(0.625)], true), Value::Float(f32::from_bits(0x3f15c8ef)));
    assert_eq!(pixel_operation("cos", &[Value::Float(0.625)], true), Value::Float(f32::from_bits(0x3f4f9b47)));
}

#[test]
fn runtime_pixel_cartesian_multiplies_the_radius_by_each_quarter_wave() {
    for (radius, angle, expected) in [
        (-1.25, std::f32::consts::FRAC_PI_4, [0xbf62462e, 0xbf624630]),
        (0.375, -0.0, [0x3ec00000, 0x80000000]),
        (0.375, std::f32::consts::FRAC_PI_2, [0, 0x3ec00000]),
    ] {
        let result = pixel_operation("cartesian", &[Value::Float(radius), Value::Float(angle)], false);
        let Value::Float2(xy) = result else { panic!("expected a coordinate") };
        assert_eq!(xy.map(f32::to_bits), expected);
    }
    let result = pixel_operation("cartesian", &[Value::Float(0.875), Value::Float(0.625)], true);
    let Value::Float2(xy) = result else { panic!("expected a coordinate") };
    assert_eq!(xy.map(f32::to_bits), [0x3f35a7de, 0x3f030fd1]);
}

#[test]
fn runtime_pixel_tangent_uses_the_pixel_sine_cosine_and_reciprocal() {
    for (angle, expected) in [(0x3db7cb5d, 0x3db84a0d), (0xc094d5bd, 0xc18258bb), (0x412febe1, 0x450051c9), (0x4453660e, 0x3f0de743)] {
        let Value::Float(value) = pixel_operation("tan", &[Value::Float(f32::from_bits(angle))], false) else { panic!("expected a scalar") };
        assert_eq!(value.to_bits(), expected);
    }
    for scope in [super::FunctionScope::Node, super::FunctionScope::ValueProcessor, super::FunctionScope::FxMap] {
        assert_eq!(scoped_operation(scope, "tan", &[Value::Float(0.625)], false), Value::Float(f32::from_bits(0x3f38b30b)));
    }
    let literal = Value::Float(f32::from_bits(0x3d07db5a));
    assert_eq!(pixel_operation("tan", &[literal], true), Value::Float(f32::from_bits(0x3d07e81c)));
}

#[test]
fn parameter_and_value_angles_fold_the_phase_before_the_polynomial() {
    use super::FunctionScope::{Node, ValueProcessor};
    for scope in [Node, ValueProcessor] {
        for (angle, sine, cosine) in [
            (0.625, 0x3f15c8f2, 0x3f4f9b7a),
            (-std::f32::consts::FRAC_PI_4, 0xbf350503, 0x3f350503),
            (1e-7, 0x34490fdb, 0x3f800000),
            (1e9, 0xbf800000, 0xbf800000),
        ] {
            for (op, expected) in [("sin", sine), ("cos", cosine)] {
                let Value::Float(v) = scoped_operation(scope, op, &[Value::Float(angle)], false) else { panic!("expected a scalar") };
                assert_eq!(v.to_bits(), expected, "{scope:?} {op}({angle})");
            }
        }
        assert_eq!(scoped_operation(scope, "sin", &[Value::Float(0.625)], true), Value::Float(f32::from_bits(0x3f15c8ef)));
    }
}

#[test]
fn pattern_angles_and_uniform_cartesian_use_the_same_components() {
    use super::FunctionScope::{FxMap, Node, ValueProcessor};
    for scope in [Node, ValueProcessor, FxMap] {
        let angle = Value::Float(0.625);
        assert_eq!(scoped_operation(scope, "cos", std::slice::from_ref(&angle), false), Value::Float(f32::from_bits(0x3f4f9b7a)));
        let Value::Float2(result) = scoped_operation(scope, "cartesian", &[Value::Float(0.875), angle], false) else { panic!("expected a coordinate") };
        assert_eq!(result.map(f32::to_bits), [0x3f35a80b, 0x3f030fd4]);
    }
}

#[test]
fn fixed_pixel_clipping_preserves_the_live_quotient_precision() {
    let rounded = 0x3b9a_ff00;
    let contracted = 0x3b9a_ff02;
    let definition = crate::find("pixel_processor").unwrap();
    for (control, tiling, expected) in [
        ("tiling", 0, rounded),
        ("tiling", 1, contracted),
        ("tiling", 2, rounded),
        ("tiling", 3, contracted),
        ("literal", 0, contracted),
        ("parameter", 3, rounded),
    ] {
        let variables = BTreeMap::from([
            ("numerator".into(), Value::Float(f32::from_bits(0xbeb2_0000))),
            ("width".into(), Value::Float(f32::from_bits(0x3f33_b333))),
            ("clip".into(), Value::Bool(false)),
            ("$tiling".into(), Value::Int(tiling)),
        ]);
        let mut graph = FunctionGraph::default();
        let [n, d] = ["numerator", "width"].map(|name| graph.add("get_float1", &[], Some(Value::Text(name.into()))));
        let q = graph.add("div", &[n, d], None);
        let half = graph.add_constant(Value::Float(0.5));
        let coordinate = graph.add("add", &[q, half], None);
        let magnitude = graph.add("abs", &[q], None);
        let outside = graph.add("gt", &[magnitude, half], None);
        let clip = match control {
            "tiling" => {
                let mask = graph.add("get_integer1", &[], Some(Value::Text("$tiling".into())));
                let two = graph.add_constant(Value::Int(2));
                let zero = graph.add_constant(Value::Int(0));
                let horizontal = graph.add("mod", &[mask, two], None);
                graph.add("eq", &[horizontal, zero], None)
            }
            "literal" => graph.add_constant(Value::Bool(false)),
            _ => graph.add("get_bool", &[], Some(Value::Text("clip".into()))),
        };
        let condition = graph.add("and", &[outside, clip], None);
        graph.output = Some(graph.add("ifelse", &[condition, half, coordinate], None));
        let params = crate::resolve_params(
            definition,
            &BTreeMap::from([("function".into(), Value::Function(Box::new(graph))), ("color_mode".into(), Value::Bool(false))]),
        );
        let context = crate::CookContext {
            w: 1,
            h: 1,
            seed: 0,
            tile: true,
            format: materialcraft_core::OutputFormat::F32,
            inputs: &[],
            input_formats: &[],
            resource: &|_| None,
            params: &params,
            vars: &variables,
        };
        let image = (definition.cook)(&context).unwrap().remove(0);
        assert_eq!(image.get_gray(0, 0, false).to_bits(), expected, "{control}, tiling {tiling}");
    }
}

#[test]
fn a_swizzle_of_a_quotient_keeps_the_other_vector_components_available_for_a_sum() {
    for (numerator, divisor, offset, expected) in [
        (
            [0x3ef8_f8f9, 0x3f59_d9da, 0x3e58_d8d9],
            [0x3ee6_e6e7, 0x3ee8_e8e9, 0x3eec_eced],
            [0.0, 0.0, -0.5],
            [0x3f8a_0474, 0x3fef_72c2, 0xbd2d_8f2f, 0x3f8a_0474],
        ),
        (
            [0x3f74_c9cf, 0x3fe6_626a, 0x3f47_c930],
            [0x3fe6_626a, 0x3f47_c930, 0x3fcf_e21a],
            [-0.375, 0.3125, -0.5],
            [0x3e20_02c4, 0x4027_9aa4, 0xbc9f_8b65, 0x3f08_00b1],
        ),
    ] {
        let variables = BTreeMap::from([("a".into(), Value::Float3(numerator.map(f32::from_bits))), ("b".into(), Value::Float3(divisor.map(f32::from_bits)))]);
        let context = EvaluationContext {
            scope: super::FunctionScope::PixelProcessor,
            image_formats: &[],
            position: [0.5; 2],
            size: [1.0; 2],
            images: &[],
            tile: true,
            variables: &variables,
        };
        let mut graph = FunctionGraph::default();
        let [a, b] = ["a", "b"].map(|name| graph.add("get_float3", &[], Some(Value::Text(name.into()))));
        let ratio = graph.add("div", &[a, b], None);
        let offset = graph.add_constant(Value::Float3(offset));
        let shifted = graph.add("add", &[ratio, offset], None);
        let red = graph.add("swizzle1", &[ratio], Some(Value::Int(0)));
        graph.output = Some(graph.add("vector4", &[shifted, red], None));
        let actual = compile(&graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
        assert_eq!(actual, Value::Float4(expected.map(f32::from_bits)));
    }
}

#[test]
fn pixel_quotient_sums_and_second_interpolation_operands_contract() {
    let values = [0x4044_ca77, 0x4077_425b, 0xbf00_bb3e, 0x4029_426b].map(|v| Value::Float(f32::from_bits(v)));
    let variables = ["a", "b", "c", "t"].into_iter().map(str::to_owned).zip(values).collect();
    let context = EvaluationContext {
        image_formats: &[],
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: true,
        variables: &variables,
    };
    let mut graph = FunctionGraph::default();
    let [a, b, c, t] = ["a", "b", "c", "t"].map(|name| graph.add("get_float1", &[], Some(Value::Text(name.into()))));
    let quotient = graph.add("div", &[a, b], None);
    let product = graph.add("mul", &[a, b], None);
    let pixel = |graph: &FunctionGraph| compile(graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
    for (op, inputs, bits) in [
        ("add", &[quotient, c][..], 0x3e96_0845),
        ("sub", &[quotient, c], 0x3fa6_3d4f),
        ("sub", &[c, quotient], 0xbfa6_3d4f),
        ("lerp", &[c, quotient, t], 0x403b_a465),
        ("lerp", &[c, product, t], 0x4200_fa51),
    ] {
        graph.output = Some(graph.add(op, inputs, None));
        assert_eq!(pixel(&graph), Value::Float(f32::from_bits(bits)));
    }
    let blend = graph.add("lerp", &[c, quotient, t], None);
    graph.output = Some(graph.add("vector2", &[quotient, blend], None));
    assert_eq!(pixel(&graph), Value::Float2([0x3f4b_bf61, 0x403b_a467].map(f32::from_bits)));
}

#[test]
fn pixel_interpolation_contracts_only_unobserved_product_lanes_in_its_second_endpoint() {
    let sample = [0xbed0_b7e6, 0x3ee9_4b78, 0xbf01_df0b, 0xbf9a_729d].map(f32::from_bits);
    let variables = BTreeMap::from([("sample".into(), Value::Float4(sample))]);
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        image_formats: &[],
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: true,
        variables: &variables,
    };
    let pixel = |graph: &FunctionGraph| compile(graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
    let mut graph = FunctionGraph::default();
    let sample = graph.add("get_float4", &[], Some(Value::Text("sample".into())));
    let first = graph.add("swizzle4", &[sample], Some(Value::Int4([2, 0, 3, 1])));
    let rgb = graph.add("swizzle3", &[sample], Some(Value::Int3([0, 1, 2])));
    let gain = graph.add("swizzle1", &[sample], Some(Value::Int(3)));
    let weight = graph.add("swizzle1", &[sample], Some(Value::Int(1)));
    let product = graph.add("mulscalar", &[rgb, gain], None);
    let alpha = graph.add_constant(Value::Float(0.625));
    let last = graph.add("vector4", &[product, alpha], None);
    let mix = graph.add("lerp", &[first, last, weight], None);
    graph.output = Some(mix);
    assert_eq!(pixel(&graph), Value::Float4([0xbd55_1682, 0xbef1_e18c, 0xbec1_7c27, 0x3f08_66b4].map(f32::from_bits)));

    let mixed_rgb = graph.add("swizzle3", &[mix], Some(Value::Int3([0, 1, 2])));
    let blue = graph.add("swizzle1", &[product], Some(Value::Int(2)));
    graph.output = Some(graph.add("vector4", &[mixed_rgb, blue], None));
    assert_eq!(pixel(&graph), Value::Float4([0xbd55_1682, 0xbef1_e18c, 0xbec1_7c28, 0x3f1c_b4a3].map(f32::from_bits)));

    graph.output = Some(graph.add("add", &[mix, last], None));
    assert_eq!(pixel(&graph), Value::Float4([0x3ee1_3554, 0xbf82_d84f, 0x3e6f_da3f, 0x3f94_335a].map(f32::from_bits)));

    graph.output = Some(graph.add("lerp", &[last, first, weight], None));
    assert_eq!(pixel(&graph), Value::Float4([0x3d15_e6f2, 0xbef8_560b, 0xbe5d_ca0b, 0x3f0c_3f08].map(f32::from_bits)));
}

#[test]
fn pixel_sums_retain_assigned_factors_until_a_component_is_observed() {
    let sample = [0x3ef4_615d, 0xbf0c_f4e5, 0x3f25_886e, 0xbfbe_1bf6].map(f32::from_bits);
    let variables = BTreeMap::from([("sample".into(), Value::Float4(sample))]);
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        image_formats: &[],
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: false,
        variables: &variables,
    };
    let pixel = |graph: &FunctionGraph| compile(graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
    let mut graph = FunctionGraph::default();
    let sample = graph.add("get_float4", &[], Some(Value::Text("sample".into())));
    let direction = graph.add("swizzle4", &[sample], Some(Value::Int4([2, 0, 1, 3])));
    let scale = graph.add("swizzle1", &[sample], Some(Value::Int(3)));
    let origin = graph.add("swizzle4", &[sample], Some(Value::Int4([1, 2, 3, 0])));
    let product = graph.add("mulscalar", &[direction, scale], None);
    let stride = || Some(Value::Text("stride".into()));
    let assign = graph.add("set", &[product], stride());
    let mut position = origin;
    for expected in [[0xbfc1_67c9, 0xbd7f_2862, 0xbf2a_dd9e, 0x402b_b9b0], [0xc01e_2a90, 0xbf45_6d7a, 0x3e19_f2c2, 0x409c_739a]] {
        let step = graph.add("get_float4", &[], stride());
        position = graph.add("add", &[position, step], None);
        graph.output = Some(graph.add("sequence", &[assign, position], None));
        assert_eq!(pixel(&graph), Value::Float4(expected.map(f32::from_bits)));
    }
    let rgb = graph.add("swizzle3", &[position], Some(Value::Int3([0, 1, 2])));
    let step = graph.add("get_float4", &[], stride());
    let blue = graph.add("swizzle1", &[step], Some(Value::Int(2)));
    let observed = graph.add("vector4", &[rgb, blue], None);
    graph.output = Some(graph.add("sequence", &[assign, observed], None));
    assert_eq!(pixel(&graph), Value::Float4([0xc01e_2a90, 0xbf45_6d7a, 0x3e19_f2c0, 0x3f51_5a4e].map(f32::from_bits)));
}

#[derive(Clone, Copy)]
enum StepRead {
    Sum,
    Blue,
    BeforeWrite,
    AfterWrite,
    Copy,
    CopyBlue,
}

fn called_pixel_step(sample: [f32; 4], wired: bool, guard: Option<bool>, read: StepRead) -> Value {
    let variables = BTreeMap::from([("field".into(), Value::Float4(sample))]);
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        image_formats: &[],
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: false,
        variables: &variables,
    };
    let mut graph = FunctionGraph::default();
    let field = graph.add("get_float4", &[], Some(Value::Text("field".into())));
    let dir = graph.add("swizzle4", &[field], Some(Value::Int4([2, 0, 1, 3])));
    let gain = graph.add("swizzle1", &[field], Some(Value::Int(3)));
    let pos = graph.add("swizzle4", &[field], Some(Value::Int4([1, 2, 3, 0])));
    let product = graph.add("mulscalar", &[dir, gain], None);
    let set = graph.add("set", &[product], Some(Value::Text("stride".into())));
    let setup = if let Some(run) = guard {
        let condition = graph.add_constant(Value::Bool(run));
        let zero = graph.add_constant(Value::Float4([0.0; 4]));
        graph.add("ifelse", &[condition, set, zero], None)
    } else {
        set
    };
    let mut args = vec![("origin", pos)];
    if wired {
        args.push(("step", graph.add("get_float4", &[], Some(Value::Text("stride".into())))));
    }
    let mut body = FunctionGraph::default();
    let origin = body.add("get_float4", &[], Some(Value::Text("origin".into())));
    let step = body.add("get_float4", &[], Some(Value::Text(if wired { "step" } else { "stride" }.into())));
    let sum = body.add("add", &[origin, step], None);
    body.output = Some(match read {
        StepRead::Sum | StepRead::Blue => sum,
        StepRead::Copy | StepRead::CopyBlue => {
            let copy = body.add("set", &[step], Some(Value::Text("saved_step".into())));
            let replace = body.add("set", &[origin], Some(Value::Text("stride".into())));
            let effects = body.add("sequence", &[copy, replace], None);
            let saved = body.add("get_float4", &[], Some(Value::Text("saved_step".into())));
            let advance = body.add("add", &[origin, saved], None);
            body.add("sequence", &[effects, advance], None)
        }
        StepRead::BeforeWrite => {
            let answer = body.add("set", &[sum], Some(Value::Text("answer".into())));
            let write = body.add("set", &[origin], Some(Value::Text("stride".into())));
            let effects = body.add("sequence", &[answer, write], None);
            let saved = body.add("get_float4", &[], Some(Value::Text("answer".into())));
            body.add("sequence", &[effects, saved], None)
        }
        StepRead::AfterWrite => {
            let write = body.add("set", &[origin], Some(Value::Text("stride".into())));
            body.add("sequence", &[write, sum], None)
        }
    });
    let call = graph.add("instance", &[], None);
    let node = graph.nodes.last_mut().unwrap();
    node.instance = Some(materialcraft_core::GraphInstance {
        label: String::new(),
        package: String::new(),
        graph: "advance".into(),
        inputs: args.iter().map(|(name, _)| materialcraft_core::GraphPort { identifier: (*name).into(), type_code: 2048 }).collect(),
        outputs: vec![materialcraft_core::GraphPort { identifier: "output".into(), type_code: 2048 }],
        defaults: args.iter().map(|(name, _)| ((*name).into(), Value::Float4([0.0; 4]))).collect(),
    });
    node.inputs = args.iter().map(|(_, id)| Some(*id)).collect();
    node.source = Some(Ok(std::sync::Arc::new(body)));
    let result = if matches!(read, StepRead::Blue | StepRead::CopyBlue) {
        let color = graph.add("swizzle3", &[call], Some(Value::Int3([0, 1, 2])));
        let name = if matches!(read, StepRead::CopyBlue) { "saved_step" } else { "stride" };
        let stored = graph.add("get_float4", &[], Some(Value::Text(name.into())));
        let blue = graph.add("swizzle1", &[stored], Some(Value::Int(2)));
        graph.add("vector4", &[color, blue], None)
    } else {
        call
    };
    graph.output = Some(graph.add("sequence", &[setup, result], None));
    compile(&graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap()
}

#[test]
fn called_pixel_sums_use_the_factors_of_a_free_or_wired_step() {
    let sample = [0x3ef4_615d, 0xbf0c_f4e5, 0x3f25_886e, 0xbfbe_1bf6].map(f32::from_bits);
    for wired in [false, true] {
        assert_eq!(
            called_pixel_step(sample, wired, None, StepRead::Sum),
            Value::Float4([0xbfc1_67c9, 0xbd7f_2862, 0xbf2a_dd9e, 0x402b_b9b0].map(f32::from_bits))
        );
        assert_eq!(called_pixel_step(sample, wired, Some(false), StepRead::Sum), Value::Float4([sample[1], sample[2], sample[3], sample[0]]));
    }
}

#[test]
fn called_pixel_sums_round_the_observed_components_of_the_stored_step() {
    let sample = [0xbed0_b7e6, 0x3ee9_4b78, 0xbf01_df0b, 0xbf9a_729d].map(f32::from_bits);
    let expected = Value::Float4([0x3f88_ad30, 0xbc7c_be30, 0xbfe0_d28a, 0xbf0c_bfd9].map(f32::from_bits));
    for wired in [false, true] {
        assert_eq!(called_pixel_step(sample, wired, None, StepRead::Blue), expected, "wired {wired}");
    }
}

#[test]
fn called_pixel_sums_follow_assignments_in_execution_order() {
    let sample = [0xbed0_b7e6, 0x3ee9_4b78, 0xbf01_df0b, 0xbf9a_729d].map(f32::from_bits);
    let sum = Value::Float4([0x3f88_ad30, 0xbc7c_be30, 0xbfe0_d289, 0x3f86_2e37].map(f32::from_bits));
    for wired in [false, true] {
        assert_eq!(called_pixel_step(sample, wired, None, StepRead::BeforeWrite), sum, "before write, wired {wired}");
        assert_eq!(called_pixel_step(sample, wired, Some(true), StepRead::Sum), sum, "executed write, wired {wired}");
    }
    let origin = [sample[1], sample[2], sample[3], sample[0]];
    assert_eq!(called_pixel_step(sample, false, None, StepRead::AfterWrite), Value::Float4(origin.map(|v| v + v)));
}

#[test]
fn called_pixel_sums_keep_factors_through_a_copied_step() {
    let sample = [0xbed0_b7e6, 0x3ee9_4b78, 0xbf01_df0b, 0xbf9a_729d].map(f32::from_bits);
    for (read, expected) in
        [(StepRead::Copy, [0x3f88_ad30, 0xbc7c_be30, 0xbfe0_d289, 0x3f86_2e37]), (StepRead::CopyBlue, [0x3f88_ad30, 0xbc7c_be30, 0xbfe0_d28a, 0xbf0c_bfd9])]
    {
        assert_eq!(called_pixel_step(sample, false, None, read), Value::Float4(expected.map(f32::from_bits)));
    }
}

#[test]
fn pixel_product_sums_round_once_unless_the_product_is_also_read() {
    let values = [0x3fe34b19, 0x400d3c6f, 0xc0732711, 0x3f9665e4].map(|bits| Value::Float(f32::from_bits(bits)));
    let variables: BTreeMap<_, _> = ["a", "b", "c", "d"].into_iter().map(str::to_owned).zip(values).collect();
    let context = EvaluationContext {
        image_formats: &[],
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: true,
        variables: &variables,
    };
    let mut graph = FunctionGraph::default();
    let [a, b, c, d] = ["a", "b", "c", "d"].map(|name| graph.add("get_float1", &[], Some(Value::Text(name.into()))));
    let product = graph.add("mul", &[a, b], None);
    let other = graph.add("mul", &[c, d], None);
    let pixel = |graph: &FunctionGraph| compile(graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();

    for (op, inputs, bits) in [
        ("add", [product, c], 0x3df49f1b),
        ("add", [c, product], 0x3df49f1b),
        ("sub", [product, c], 0x40f6f98d),
        ("sub", [c, product], 0xc0f6f98d),
        ("add", [product, other], 0xbf0b9d31),
    ] {
        graph.output = Some(graph.add(op, &inputs, None));
        assert_eq!(pixel(&graph), Value::Float(f32::from_bits(bits)), "{op} {inputs:?}");
    }

    let sum = graph.add("add", &[product, c], None);
    graph.output = Some(graph.add("vector2", &[product, sum], None));
    assert_eq!(pixel(&graph), Value::Float2([0x407acc0a, 0x3df49f20].map(f32::from_bits)));
    let context = EvaluationContext { image_formats: &[], scope: super::FunctionScope::Node, ..context };
    graph.output = Some(sum);
    assert_eq!(compile(&graph, 0).unwrap().evaluate(&context, &mut EvaluationState::default()).unwrap(), Value::Float(f32::from_bits(0x3df49f20)));
}

#[test]
fn adding_selected_quotient_lanes_contracts_the_left_unshared_lane() {
    let variables = BTreeMap::new();
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        position: [0.15625, 0.03125],
        size: [16.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &variables,
    };
    let mut graph = FunctionGraph::default();
    let position = graph.add("get_float2", &[], Some(Value::Text("$pos".into())));
    let divisor = graph.add_constant(Value::Float2([3.625, -0.71875]));
    let quotient = graph.add("div", &[position, divisor], None);
    let x = graph.add("swizzle1", &[quotient], Some(Value::Int(0)));
    let y = graph.add("swizzle1", &[quotient], Some(Value::Int(1)));
    let pixel = |graph: &FunctionGraph| compile(graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
    for (terms, expected) in [([x, y], 0xb9c4_82a0), ([y, x], 0xb9c4_8280)] {
        graph.output = Some(graph.add("add", &terms, None));
        assert_eq!(pixel(&graph), Value::Float(f32::from_bits(expected)));
    }
    let sum = graph.add("add", &[x, y], None);
    for (shared, expected, rounded) in [(x, 0xb9c4_8280, 0.15625f32 / 3.625), (y, 0xb9c4_82a0, 0.03125f32 / -0.71875)] {
        graph.output = Some(graph.add("vector2", &[sum, shared], None));
        assert_eq!(pixel(&graph), Value::Float2([f32::from_bits(expected), rounded]));
    }
}

#[test]
fn subtracting_selected_products_keeps_the_operand_signs() {
    let variables = BTreeMap::new();
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        position: [0.15625, 0.03125],
        size: [16.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &variables,
    };
    let mut graph = FunctionGraph::default();
    let position = graph.add("get_float2", &[], Some(Value::Text("$pos".into())));
    let factors = graph.add_constant(Value::Float2([3.6123412, -0.7187911]));
    let product = graph.add("mul", &[position, factors], None);
    let x = graph.add("swizzle1", &[product], Some(Value::Int(0)));
    let y = graph.add("swizzle1", &[product], Some(Value::Int(1)));
    for (terms, expected) in [([x, y], 0x3f16_3e75), ([y, x], 0xbf16_3e76), ([x, x], 0)] {
        graph.output = Some(graph.add("sub", &terms, None));
        let pixel = compile(&graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
        assert_eq!(pixel, Value::Float(f32::from_bits(expected)));
    }
}

#[test]
fn pixel_product_sum_through_packed_lanes_and_calls_keeps_other_reads() {
    let [a, b, c] = [0xbfd8_e681, 0xc03e_5cec, 0xc05f_9255].map(f32::from_bits);
    let variables = ["a", "b", "c"].into_iter().map(str::to_owned).zip([a, b, c].map(Value::Float)).collect();
    let context = EvaluationContext {
        image_formats: &[],
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: true,
        variables: &variables,
    };
    let pixel = |graph: &FunctionGraph| compile(graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
    let mut draws = FunctionGraph::default();
    let one = draws.add_constant(Value::Float(1.0));
    let first = draws.add("rand", &[one], None);
    let second = draws.add("rand", &[one], None);
    draws.output = Some(draws.add("vector2", &[first, second], None));
    let Value::Float2([first_draw, second_draw]) = pixel(&draws) else { panic!("expected two random draws") };
    assert_ne!(first_draw, second_draw);

    for called in [false, true] {
        let mut graph = FunctionGraph::default();
        let [left, right] = ["a", "b"].map(|name| graph.add("get_float1", &[], Some(Value::Text(name.into()))));
        let product = graph.add("mul", &[left, right], None);
        let one = graph.add_constant(Value::Float(1.0));
        let draw = graph.add("rand", &[one], None);
        let mut pair = graph.add("vector2", &[product, draw], None);
        if called {
            graph.output = Some(pair);
            let body = std::mem::take(&mut graph);
            pair = graph.add("instance", &[], None);
            let node = graph.nodes.last_mut().unwrap();
            node.instance = Some(materialcraft_core::GraphInstance {
                label: String::new(),
                package: String::new(),
                graph: "pair".into(),
                inputs: Vec::new(),
                outputs: Vec::new(),
                defaults: BTreeMap::new(),
            });
            node.source = Some(Ok(std::sync::Arc::new(body)));
        }
        let x = graph.add("swizzle1", &[pair], Some(Value::Int(0)));
        let y = graph.add("swizzle1", &[pair], Some(Value::Int(1)));
        let addend = graph.add("get_float1", &[], Some(Value::Text("c".into())));
        for args in [[x, addend], [addend, x]] {
            let sum = graph.add("add", &args, None);
            graph.output = Some(graph.add("vector2", &[sum, y], None));
            assert_eq!(pixel(&graph), Value::Float2([f32::from_bits(0x3fc6_0290), first_draw]));
            graph.output = Some(graph.add("vector2", &[sum, x], None));
            assert_eq!(pixel(&graph), Value::Float2([f32::from_bits(0x3fc6_0292), a * b]));
            let one = graph.add_constant(Value::Float(1.0));
            let next = graph.add("rand", &[one], None);
            graph.output = Some(graph.add("vector2", &[sum, next], None));
            assert_eq!(pixel(&graph), Value::Float2([f32::from_bits(0x3fc6_0290), second_draw]));
        }
    }
}

#[test]
fn a_quotient_can_round_with_an_intermediate_sum_in_a_called_function() {
    let variables = BTreeMap::from([("dot".into(), Value::Float(f32::from_bits(0x3f01187e))), ("width".into(), Value::Float(0.6875))]);
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &variables,
    };

    let mut inner = FunctionGraph::default();
    let q = inner.add("get_float1", &[], Some(Value::Text("q".into())));
    let one = inner.add_constant(Value::Float(1.0));
    let sum = inner.add("add", &[q, one], None);
    let two = inner.add_constant(Value::Float(2.0));
    inner.output = Some(inner.add("div", &[sum, two], None));

    let mut graph = FunctionGraph::default();
    let [dot, width] = ["dot", "width"].map(|name| graph.add("get_float1", &[], Some(Value::Text(name.into()))));
    let quotient = graph.add("div", &[dot, width], None);
    let result = graph.add("instance", &[quotient], None);
    let call = graph.nodes.last_mut().unwrap();
    call.instance = Some(materialcraft_core::GraphInstance {
        label: String::new(),
        package: String::new(),
        graph: "half_range".into(),
        inputs: vec![materialcraft_core::GraphPort { identifier: "q".into(), type_code: 256 }],
        outputs: Vec::new(),
        defaults: BTreeMap::from([("q".into(), Value::Float(0.0))]),
    });
    call.source = Some(Ok(std::sync::Arc::new(inner)));
    let pixel = |g: &FunctionGraph| compile(g, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
    graph.output = Some(result);
    assert_eq!(pixel(&graph), Value::Float(f32::from_bits(0x3f5de345)));
    graph.output = Some(graph.add("vector2", &[result, quotient], None));
    assert_eq!(pixel(&graph), Value::Float2([0x3f5de344, 0x3f3bc689].map(f32::from_bits)));
}

#[test]
fn a_pixel_quotient_rounds_with_the_subtraction_in_a_called_function() {
    let names = ["offset", "amount"];
    let mut inner = FunctionGraph::default();
    let [offset, amount] = names.map(|name| inner.add("get_float1", &[], Some(Value::Text(name.into()))));
    inner.output = Some(inner.add("sub", &[offset, amount], None));

    let mut graph = FunctionGraph::default();
    let [x, y, z] = ["x", "y", "z"].map(|name| graph.add("get_float1", &[], Some(Value::Text(name.into()))));
    let quotient = graph.add("div", &[x, y], None);
    let result = graph.add("instance", &[z, quotient], None);
    let call = graph.nodes.last_mut().unwrap();
    call.instance = Some(materialcraft_core::GraphInstance {
        label: String::new(),
        package: String::new(),
        graph: "difference".into(),
        inputs: names.map(|name| materialcraft_core::GraphPort { identifier: name.into(), type_code: 256 }).into(),
        outputs: Vec::new(),
        defaults: names.into_iter().map(|name| (name.into(), Value::Float(0.0))).collect(),
    });
    call.source = Some(Ok(std::sync::Arc::new(inner)));
    let variables = ["x", "y", "z"].map(str::to_owned).into_iter().zip([0xc022f465, 0xbfb54d77, 0x3f4cdf41].map(|b| Value::Float(f32::from_bits(b)))).collect();
    let context = EvaluationContext {
        image_formats: &[],
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: true,
        variables: &variables,
    };
    let pixel = |g: &FunctionGraph| compile(g, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
    graph.output = Some(result);
    assert_eq!(pixel(&graph), Value::Float(f32::from_bits(0xbf7f5030)));
    graph.output = Some(graph.add("vector2", &[result, quotient], None));
    assert_eq!(pixel(&graph), Value::Float2([0xbf7f502f, 0x3fe617b8].map(f32::from_bits)));
}

#[test]
fn vector_sums_use_the_unrounded_product_of_each_independently_returned_lane() {
    use materialcraft_core::{GraphInstance, GraphPort};
    let mut body = FunctionGraph::default();
    let [left, right] = ["left", "right"].map(|name| body.add("get_float2", &[], Some(Value::Text(name.into()))));
    let product = body.add("mul", &[left, right], None);
    let side = body.add("get_float1", &[], Some(Value::Text("side".into())));
    let side = body.add("set", &[side], Some(Value::Text("last".into())));
    body.output = Some(body.add("vector3", &[product, side], None));
    let source = std::sync::Arc::new(body);
    let interface = GraphInstance {
        label: String::new(),
        package: String::new(),
        graph: "pair".into(),
        inputs: [("left", 512), ("right", 512), ("side", 256)].map(|(name, type_code)| GraphPort { identifier: name.into(), type_code }).into(),
        outputs: Vec::new(),
        defaults: [("left", Value::Float2([0.0; 2])), ("right", Value::Float2([0.0; 2])), ("side", Value::Float(0.0))].map(|(k, v)| (k.into(), v)).into(),
    };
    let mut graph = FunctionGraph::default();
    let [r, g, b, a] = ["r", "g", "b", "a"].map(|name| graph.add("get_float1", &[], Some(Value::Text(name.into()))));
    let [rg, ba, gr] = [[r, g], [b, a], [g, r]].map(|xy| graph.add("vector2", &xy, None));
    let [first, second] = [[rg, ba, a], [ba, gr, r]].map(|arguments| {
        let call = graph.add("instance", &arguments, None);
        let node = graph.nodes.last_mut().unwrap();
        node.instance = Some(interface.clone());
        node.source = Some(Ok(source.clone()));
        call
    });
    let sum = graph.add("add", &[first, second], None);
    let variables = ["r", "g", "b", "a"]
        .map(str::to_owned)
        .into_iter()
        .zip([0xc05f3333, 0x40680000, 0x3f600000, 0xbf200000].map(|v| Value::Float(f32::from_bits(v))))
        .collect();
    let context = EvaluationContext {
        image_formats: &[],
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: true,
        variables: &variables,
    };
    for (read, lane, expected) in [
        (first, 0, [0x3df66660, 0xbdb00000, 0xc083999a, 0xc0434ccd]),
        (second, 0, [0x3df6666c, 0xbdb00000, 0xc083999a, 0x404b0000]),
        (first, 2, [0x3df6666c, 0xbdb00000, 0xc083999a, 0xbf200000]),
    ] {
        let exposed = graph.add("swizzle1", &[read], Some(Value::Int(lane)));
        graph.output = Some(graph.add("vector4", &[sum, exposed], None));
        let value = compile(&graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
        assert_eq!(value, Value::Float4(expected.map(f32::from_bits)));
    }
}

#[test]
fn pixel_sums_contract_negated_products_and_preserve_shared_rounded_addends() {
    let values = [0xc0742acf, 0xbf116093, 0x405370ba, 0x3f3eb229].map(|bits| Value::Float(f32::from_bits(bits)));
    let variables = ["a", "b", "c", "d"].into_iter().map(str::to_owned).zip(values).collect();
    let context = EvaluationContext {
        image_formats: &[],
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: true,
        variables: &variables,
    };
    let mut graph = FunctionGraph::default();
    let [a, b, c, d] = ["a", "b", "c", "d"].map(|name| graph.add("get_float1", &[], Some(Value::Text(name.into()))));
    let product = graph.add("mul", &[a, b], None);
    let negative = graph.add("neg", &[product], None);
    let other = graph.add("mul", &[c, d], None);
    let evaluate = |graph: &FunctionGraph| compile(graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
    for (op, operands, expected) in [
        ("add", [negative, c], 0x3f9190bd),
        ("sub", [negative, c], 0xc0af0c8b),
        ("sub", [c, negative], 0x40af0c8b),
        ("add", [negative, other], 0x3e96c3a4),
        ("sub", [negative, other], 0xc0941496),
    ] {
        graph.output = Some(graph.add(op, &operands, None));
        assert_eq!(evaluate(&graph), Value::Float(f32::from_bits(expected)));
    }
    let sum = graph.add("add", &[negative, c], None);
    let both = graph.add("add", &[negative, other], None);
    graph.output = Some(graph.add("vector2", &[sum, both], None));
    assert_eq!(evaluate(&graph), Value::Float2([0x3f9190bc, 0x3e96c3a2].map(f32::from_bits)));
    graph.output = Some(graph.add("vector2", &[negative, sum], None));
    assert_eq!(evaluate(&graph), Value::Float2([0xc00aa85c, 0x3f9190bc].map(f32::from_bits)));
}

#[test]
fn paired_sums_keep_the_same_product_unrounded() {
    let values = [0xbfd8e681, 0xc00ec5b1, 0xc05f9255, 0xc020d2bc].map(|bits| Value::Float(f32::from_bits(bits)));
    let variables = ["r", "g", "b", "a"].into_iter().map(str::to_owned).zip(values).collect();
    let context = EvaluationContext {
        image_formats: &[],
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: true,
        variables: &variables,
    };
    let mut graph = FunctionGraph::default();
    let [r, g, b, a] = ["r", "g", "b", "a"].map(|name| graph.add("get_float1", &[], Some(Value::Text(name.into()))));
    let p = graph.add("mul", &[r, b], None);
    let q = graph.add("mul", &[g, a], None);
    let sum = graph.add("add", &[p, q], None);
    let difference = graph.add("sub", &[q, p], None);
    for (order, expected) in [([sum, difference], [0x41386778, 0xbea0a9ae]), ([difference, sum], [0xbea0a9ab, 0x41386779])] {
        graph.output = Some(graph.add("vector2", &order, None));
        let actual = compile(&graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
        assert_eq!(actual, Value::Float2(expected.map(f32::from_bits)));
    }
}

#[test]
fn clamped_and_repeating_uniform_texture_reads_keep_fractional_weights() {
    let image = Image::generate(32, 32, Kind::Color, |x, y| {
        let channel = |c: u32| ((47 * x + 31 * y + 7 * x * y + 13 + c * (29 + 3 * x)) % 251) as f32 / 255.0;
        [channel(0), channel(1), channel(2), 1.0]
    })
    .with_integer_sampling(true);
    let mut context = EvaluationContext {
        scope: super::FunctionScope::FxMap,
        position: [0.5; 2],
        size: [32.0; 2],
        images: &[Some(image)],
        image_formats: &[materialcraft_core::OutputFormat::U8],
        tile: false,
        variables: &BTreeMap::new(),
    };
    for (uv, mode, color, gray) in [
        ([-0.125, -0.25], 0, [0x3e868687, 0x3f31b1b2, 0x3e109091, 0x3f800000], 0x3ebb6612),
        ([0.71897, 0.39151], 1, [0x3e90e1aa, 0x3f25d28e, 0x3f06f4bc, 0x3f800000], 0x3ef8d016),
        ([-0.12, 0.016], 2, [0x3d50d0d2, 0x3e28a8a9, 0x3e8e8e8f, 0x3f800000], 0x3e28a8aa),
        ([-0.12, 0.016], 3, [0x3d56ca82, 0x3e2a2716, 0x3e8f4dc4, 0x3f800000], 0x3e2a2715),
        ([1.0311, -0.65315], 3, [0x3f57d7d9, 0x3eacacad, 0x3f50d0d2, 0x3f800000], 0x3f2a5500),
    ] {
        let mut graph = FunctionGraph::default();
        let position = graph.add_constant(Value::Float2(uv));
        for (sampler, expected) in [("samplecol", Value::Float4(color.map(f32::from_bits))), ("samplelum", Value::Float(f32::from_bits(gray)))] {
            graph.output = Some(graph.add(sampler, &[position], Some(Value::Int2([0, mode]))));
            let program = compile(&graph, 0).unwrap();
            for scope in [super::FunctionScope::FxMap, super::FunctionScope::ValueProcessor] {
                context.scope = scope;
                let value = program.evaluate(&context, &mut EvaluationState::default()).unwrap();
                assert_eq!(value, expected, "{scope:?} {sampler} mode {mode} at {uv:?}");
            }
        }
    }
}

#[test]
fn clamped_uniform_filter_keeps_its_fractions_when_all_four_taps_reach_one_corner() {
    let edge = [0x406bfbec, 0xbed93e37, 0x405db487, 0x3f800000].map(f32::from_bits);
    let image = Image::generate(32, 32, Kind::Color, |x, y| if (x, y) == (31, 0) { edge } else { [0.0; 4] });
    let mut graph = FunctionGraph::default();
    let position = graph.add_constant(Value::Float2([1.0311, -0.65315]));
    graph.output = Some(graph.add("samplecol", &[position], Some(Value::Int2([0, 3]))));
    let program = compile(&graph, 0).unwrap();
    let images = [Some(image)];
    for scope in [super::FunctionScope::FxMap, super::FunctionScope::ValueProcessor] {
        let context = EvaluationContext {
            scope,
            position: [0.5; 2],
            size: [32.0; 2],
            images: &images,
            image_formats: &[materialcraft_core::OutputFormat::F32],
            tile: true,
            variables: &BTreeMap::new(),
        };
        let value = program.evaluate(&context, &mut EvaluationState::default()).unwrap();
        assert_eq!(value.as_vec4().unwrap().map(f32::to_bits), [0x406bfbec, 0xbed93e38, 0x405db488, 0x3f800000]);
    }
}

#[test]
fn pixel_gray_sampler_combines_stored_color_channels_after_reading() {
    let mut graph = FunctionGraph::default();
    let position = graph.add_constant(Value::Float2([0.5; 2]));
    graph.output = Some(graph.add("samplelum", &[position], Some(Value::Int2([0, 0]))));
    let program = compile(&graph, 0).unwrap();
    let rgba = [0x40521096, 0xbfccf206, 0xbfa48635, 0x3f800000].map(f32::from_bits);
    for (kind, expected) in [(Kind::Color, 0x3e0717d6), (Kind::Gray, rgba[0].to_bits())] {
        let context = EvaluationContext {
            scope: super::FunctionScope::PixelProcessor,
            position: [0.5; 2],
            size: [1.0; 2],
            images: &[Some(Image::filled(1, 1, kind, rgba))],
            image_formats: &[],
            tile: true,
            variables: &BTreeMap::new(),
        };
        assert_eq!(program.evaluate_pixel(&context, &mut EvaluationState::default()).unwrap(), Value::Float(f32::from_bits(expected)));
    }
}

#[test]
fn clamped_pixel_samplers_select_the_edge_texel_and_filter_along_it() {
    let image = Image::generate(8, 4, Kind::Color, |x, y| {
        std::array::from_fn(|c| {
            let c = c as u32;
            let level = (79 * x + 53 * y + 29 * x * y + 17 * c + 11 * x * c + 3) % 251;
            (level as f32 - 91.75) / 71.375
        })
    });
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [16.0, 32.0],
        images: &[Some(image)],
        image_formats: &[materialcraft_core::OutputFormat::F32],
        tile: true,
        variables: &BTreeMap::new(),
    };
    for (mode, uv, color, gray) in [
        (2, [-0.125, 0.25], [0xbf003963, 0xbe868039, 0xbcc8daca, 0x3e5ac9c0], 0xbe86803a),
        (3, [-0.125, 0.25], [0xbf5f457e, 0xbf224c37, 0xbecaa5e2, 0xbe2166ac], 0xbf224c38),
        (2, [0.3359375, 0.40625], [0xbf7dc221, 0xbee3c13c, 0x3dd0072c, 0x3f25e269], 0xbee3c13c),
        (3, [0.3359375, 0.40625], [0xbf160647, 0xbc2fbf85, 0x3f10884b, 0x3f101cb2], 0xbc2fbf80),
        (3, [1.3125, -0.375], [0xbf0765c6, 0x3f49c056, 0x4006b99c, 0xbdc1ae67], 0x3f49c056),
    ] {
        let mut graph = FunctionGraph::default();
        let position = graph.add_constant(Value::Float2(uv));
        for (op, expected) in [("samplecol", Value::Float4(color.map(f32::from_bits))), ("samplelum", Value::Float(f32::from_bits(gray)))] {
            graph.output = Some(graph.add(op, &[position], Some(Value::Int2([0, mode]))));
            let actual = compile(&graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
            assert_eq!(actual, expected, "{op} mode {mode} at {uv:?}");
        }
    }
}

#[test]
fn filtered_pixel_reads_flush_single_precision_subnormals_before_and_after_filtering() {
    let normal = f32::MIN_POSITIVE;
    let subnormal = f32::from_bits(normal.to_bits() - 1);
    let left = [subnormal, -subnormal, -normal, normal * 2.0];
    let image = Image::generate(2, 1, Kind::Color, |x, _| if x == 0 { left } else { [normal, -normal, 0.0, subnormal] });
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        position: [0.0; 2],
        size: [2.0, 1.0],
        images: &[Some(image)],
        image_formats: &[materialcraft_core::OutputFormat::F32],
        tile: true,
        variables: &BTreeMap::new(),
    };
    for (filter, u, expected) in [(0, 0.25, left), (1, 0.25, [0.0, -0.0, -normal, normal * 2.0]), (1, 0.5, [0.0, -0.0, -0.0, normal])] {
        let mut graph = FunctionGraph::default();
        let uv = graph.add_constant(Value::Float2([u, 0.5]));
        graph.output = Some(graph.add("samplecol", &[uv], Some(Value::Int2([0, filter]))));
        let value = compile(&graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
        assert_eq!(value.as_vec4().unwrap().map(f32::to_bits), expected.map(f32::to_bits));
    }
}

#[test]
fn pixel_dot_rounds_in_channel_order() {
    // One sampled texel distinguishes the accumulation orders in all three widths.
    let [x, y, z, w] = [0xc07089a0, 0x3fa2fba9, 0xbe30266f, 0xbcf10944].map(f32::from_bits);
    for (left, right, dynamic_bits, literal_bits) in [
        (Value::Float2([x, y]), Value::Float2([0.2126, 0.7152]), 0x3de49f4e, 0x3de49f50),
        (Value::Float3([x, y, z]), Value::Float3([0.2126, 0.7152, 0.0722]), 0x3dcb2fab, 0x3dcb2fad),
        (Value::Float4([x, y, z, w]), Value::Float4([0.2126, 0.7152, 0.0722, -0.39]), 0x3de2afee, 0x3de2aff0),
    ] {
        let operands = [left, right];
        assert_eq!(pixel_operation("dot", &operands, false), Value::Float(f32::from_bits(dynamic_bits)));
        assert_eq!(pixel_operation("dot", &operands, true), Value::Float(f32::from_bits(literal_bits)));
    }
}

#[test]
fn uniform_atan2_rounds_each_term_and_preserves_literal_angles() {
    for scope in [super::FunctionScope::Node, super::FunctionScope::ValueProcessor, super::FunctionScope::FxMap] {
        for (coordinate, bits) in [
            ([1.0, 0.1316152], 0x3e061f75),
            ([1.0, -0.1316152], 0xbe061f75),
            ([1.875, 0.057_220_46], 0x3cfa72bc),
            ([-0.1316152, 1.0], 0x3fd9d3ca),
            ([1.0, 1.0], 0x3f490fe1),
            ([-1.0, -0.0], 0x40490fdb),
            ([f32::from_bits(0x00400000), f32::MIN_POSITIVE], 0x3fc90fdb),
            ([f32::from_bits(0x7f000000), f32::from_bits(0x7e800000)], 0x3eed6346),
        ] {
            let Value::Float(angle) = scoped_operation(scope, "atan2", &[Value::Float2(coordinate)], false) else { panic!("expected an angle") };
            assert_eq!(angle.to_bits(), bits, "{scope:?} {coordinate:?}");
        }
        assert_eq!(scoped_operation(scope, "atan2", &[Value::Float2([1.0, 1.0])], true), Value::Float(std::f32::consts::FRAC_PI_4));
    }
    assert_eq!(pixel_operation("atan2", &[Value::Float2([1.0, 0.1316152])], false), Value::Float(f32::from_bits(0x3e061f76)));
}

#[test]
fn pixel_atan2_reconstructs_the_quadrant_from_the_runtime_ratio() {
    for (coordinate, bits) in [
        ([1.0, 1.0], 0x3f490fe1),
        ([-1.0, 1.0], 0x4016cbe3),
        ([-1.0, -1.0], 0xc016cbe3),
        ([1.0, -1.0], 0xbf490fe1),
        ([f32::from_bits(0xc07089a0), f32::from_bits(0x3fa2fba9)], 0x40342917),
        ([-1.0, -0.0], 0x40490fdb),
        ([-0.0, 0.0], 0),
    ] {
        let Value::Float(angle) = pixel_operation("atan2", &[Value::Float2(coordinate)], false) else { panic!("expected an angle") };
        assert_eq!(angle.to_bits(), bits, "{coordinate:?}");
    }
    assert_eq!(pixel_operation("atan2", &[Value::Float2([1.0, 1.0])], true), Value::Float(std::f32::consts::FRAC_PI_4));
}

#[test]
fn a_literal_unit_interval_saturates_an_undefined_pixel_quotient() {
    let mut graph = FunctionGraph::default();
    let variable = graph.add("get_float1", &[], Some(Value::Text("zero".into())));
    let undefined = graph.add("div", &[variable, variable], None);
    let zero = graph.add_constant(Value::Float(0.0));
    let one = graph.add_constant(Value::Float(1.0));
    let upper = graph.add("min", &[undefined, one], None);
    let clamp = graph.add("max", &[zero, upper], None);
    graph.output = Some(clamp);
    let variables = BTreeMap::from([("zero".into(), Value::Float(0.0))]);
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &variables,
    };
    let pixel = |g: &FunctionGraph| compile(g, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
    assert_eq!(pixel(&graph), Value::Float(0.0));
    graph.output = Some(graph.add("vector2", &[clamp, upper], None));
    assert_eq!(pixel(&graph), Value::Float2([0.0, 1.0]));

    let literal_undefined = graph.add("div", &[zero, zero], None);
    let upper = graph.add("min", &[one, literal_undefined], None);
    let negative_zero = graph.add_constant(Value::Float(-0.0));
    graph.output = Some(graph.add("max", &[upper, negative_zero], None));
    assert!(matches!(pixel(&graph), Value::Float(value) if value.to_bits() == 0));

    let fractional = graph.add_constant(Value::Float(0.75));
    let upper = graph.add("min", &[undefined, fractional], None);
    graph.output = Some(graph.add("max", &[zero, upper], None));
    assert_eq!(pixel(&graph), Value::Float(0.75));
    let upper = graph.add("min", &[undefined, one], None);
    graph.output = Some(graph.add("max", &[variable, upper], None));
    assert_eq!(pixel(&graph), Value::Float(1.0));
}

#[test]
fn pixel_division_multiplies_by_the_reciprocal() {
    let operands = [Value::Float(-2.7353475), Value::Float(1.7320508)];
    assert_eq!(pixel_operation("div", &operands, false), Value::Float(f32::from_bits(0xbfca24fb)));
    assert_eq!(pixel_operation("div", &operands, true), Value::Float(f32::from_bits(0xbfca24fc)));
    // The runtime flushes both operands of this quotient, giving zero divided by zero.
    let tiny = Value::Float(f32::from_bits(1));
    let Value::Float(quotient) = pixel_operation("div", &[tiny.clone(), tiny], false) else { panic!("expected float") };
    assert!(quotient.is_nan());
}

#[test]
fn a_literal_pixel_divisor_folds_before_varying_products_and_sums() {
    let values = [0xc349_28a6, 0x437f_0000, 0xbeff_f6a8].map(|v| Value::Float(f32::from_bits(v)));
    let variables = ["numerator", "divisor", "offset"].into_iter().map(str::to_owned).zip(values).collect();
    let context = EvaluationContext {
        image_formats: &[],
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: true,
        variables: &variables,
    };
    let mut graph = FunctionGraph::default();
    let [numerator, runtime, offset] = ["numerator", "divisor", "offset"].map(|name| graph.add("get_float1", &[], Some(Value::Text(name.into()))));
    let literal = graph.add_constant(Value::Float(255.0));
    let twice = graph.add_constant(Value::Float(510.0));
    let two = graph.add_constant(Value::Float(2.0));
    let folded = graph.add("div", &[twice, two], None);
    let pixel = |graph: &FunctionGraph| compile(graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
    for (divisor, quotient_bits, sum_bits) in [(literal, 0xbf49_f299, 0xbfa4_f6f7), (folded, 0xbf49_f299, 0xbfa4_f6f7), (runtime, 0xbf49_f298, 0xbfa4_f6f6)] {
        let quotient = graph.add("div", &[numerator, divisor], None);
        graph.output = Some(quotient);
        assert_eq!(pixel(&graph), Value::Float(f32::from_bits(quotient_bits)));
        graph.output = Some(graph.add("add", &[quotient, offset], None));
        assert_eq!(pixel(&graph), Value::Float(f32::from_bits(sum_bits)));
    }
}

#[test]
fn measured_pixel_reciprocals_preserve_runtime_rounding() {
    for (input, expected) in [(0x3ecccccc, 0x40200000), (0x3ecc4ccd, 0x4020643f), (0xc2ff6f11, 0xbc0048a0), (0x038537b1, 0x7b75f93f)] {
        let x = f32::from_bits(input);
        let operands = [Value::Float(1.0), Value::Float(x)];
        assert_eq!(pixel_operation("div", &operands, false), Value::Float(f32::from_bits(expected)));
        assert_eq!(pixel_operation("div", &operands, true), Value::Float(1.0 / x));
    }
}

#[test]
fn pixel_branch_shared_draws_precede_the_selected_arm() {
    let mut graph = FunctionGraph::default();
    let test = graph.add("get_bool", &[], Some(Value::Text("first".into())));
    let one = graph.add_constant(Value::Float(1.0));
    let [a, b, next] = std::array::from_fn(|_| graph.add("rand", &[one], None));
    let left = graph.add("vector2", &[a, a], None);
    let right = graph.add("vector2", &[b, a], None);
    let branch = graph.add("ifelse", &[test, left, right], None);
    let tail = graph.add("vector2", &[next, next], None);
    graph.output = Some(graph.add("vector4", &[branch, tail], None));
    let values = BTreeMap::from([("first".into(), Value::Bool(false))]);
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        position: [24.5 / 32.0, 0.5 / 32.0],
        size: [32.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &values,
    };
    let program = compile(&graph, 137).unwrap();
    let result = program.evaluate(&context, &mut EvaluationState::default()).unwrap();
    assert_eq!(result, Value::Float4([0x3e87_bb00, 0x3f59_6540, 0x3f22_ab7c, 0x3f22_ab7c].map(f32::from_bits)));
}

#[test]
fn pattern_branch_draws_shared_with_a_later_channel_keep_their_order() {
    let mut graph = FunctionGraph::default();
    let condition = graph.add("get_bool", &[], Some(Value::Text("take".into())));
    let one = graph.add_constant(Value::Float(1.0));
    let [a, b, c, alternate, green] = std::array::from_fn(|_| graph.add("rand", &[one], None));
    let mixed = graph.add("lerp", &[a, b, c], None);
    let red = graph.add("ifelse", &[condition, mixed, alternate], None);
    let rg = graph.add("vector2", &[red, green], None);
    graph.output = Some(graph.add("vector3", &[rg, a], None));
    for (take, seed, bits) in [
        (true, 31, [0x3f51_c505, 0x3e9a_198c, 0x3f34_57e6]),
        (false, 31, [0x3f5a_33c2, 0x3f46_fab2, 0x3f34_57e6]),
        (true, 12071, [0x3f12_cb20, 0x3f48_974e, 0x3ee5_cd7c]),
        (false, 68047, [0x3c33_7c80, 0x3f58_d3c2, 0x3d46_3760]),
    ] {
        let values = BTreeMap::from([("take".into(), Value::Bool(take))]);
        let context = EvaluationContext {
            scope: super::FunctionScope::FxMap,
            position: [0.5; 2],
            size: [16.0; 2],
            images: &[],
            image_formats: &[],
            tile: true,
            variables: &values,
        };
        let program = compile(&graph, seed).unwrap();
        assert_eq!(program.evaluate(&context, &mut EvaluationState::default()).unwrap(), Value::Float3(bits.map(f32::from_bits)));
    }
}

#[test]
fn shared_pattern_draws_follow_the_order_of_the_conditional_arms() {
    let mut graph = FunctionGraph::default();
    let take = graph.add("get_bool", &[], Some(Value::Text("take".into())));
    let one = graph.add_constant(Value::Float(1.0));
    let [a, b, c, d, next] = std::array::from_fn(|_| graph.add("rand", &[one], None));
    let yes = graph.add("lerp", &[a, b, c], None);
    let no = graph.add("lerp", &[c, b, d], None);
    let branch = graph.add("ifelse", &[take, yes, no], None);
    let rg = graph.add("vector2", &[branch, next], None);
    graph.output = Some(graph.add("vector3", &[rg, d], None));
    let program = compile(&graph, 31).unwrap();
    for (take, bits) in [(true, [0x3f51_c505, 0x3f5b_5a9e, 0x3e9a_198c]), (false, [0x3f3c_c6a3, 0x3e9a_198c, 0x3f46_fab2])] {
        let variables = BTreeMap::from([("take".into(), Value::Bool(take))]);
        let context = EvaluationContext {
            scope: super::FunctionScope::FxMap,
            position: [0.5; 2],
            size: [16.0; 2],
            images: &[],
            image_formats: &[],
            tile: true,
            variables: &variables,
        };
        assert_eq!(program.evaluate(&context, &mut EvaluationState::default()).unwrap(), Value::Float3(bits.map(f32::from_bits)));
    }
}

#[test]
fn selecting_a_packed_pattern_channel_consumes_only_its_draw() {
    let mut graph = FunctionGraph::default();
    let one = graph.add_constant(Value::Float(1.0));
    let [a, b, c, d] = std::array::from_fn(|_| graph.add("rand", &[one], None));
    let left = graph.add("vector2", &[a, b], None);
    let right = graph.add("vector2", &[c, d], None);
    let packed = graph.add("vector4", &[left, right], None);
    let [last, third, first] = [3, 2, 0].map(|i| graph.add("swizzle1", &[packed], Some(Value::Int(i))));
    let pair = graph.add("vector2", &[last, third], None);
    graph.output = Some(graph.add("vector3", &[pair, first], None));
    let variables = BTreeMap::new();
    let context = EvaluationContext {
        scope: super::FunctionScope::FxMap,
        position: [0.5; 2],
        size: [16.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &variables,
    };
    let value = compile(&graph, 31).unwrap().evaluate(&context, &mut EvaluationState::default()).unwrap();
    assert_eq!(value, Value::Float3([0x3f34_57e6, 0x3f5a_33c2, 0x3f46_fab2].map(f32::from_bits)));
}

#[test]
fn pattern_call_reads_the_selected_components_of_a_packed_argument() {
    let mut body = FunctionGraph::default();
    let channels = body.add("get_float4", &[], Some(Value::Text("channels".into())));
    let [alpha, blue, red] = [3, 2, 0].map(|i| body.add("swizzle1", &[channels], Some(Value::Int(i))));
    let first = body.add("vector2", &[alpha, blue], None);
    body.output = Some(body.add("vector3", &[first, red], None));

    let mut graph = FunctionGraph::default();
    let one = graph.add_constant(Value::Float(1.0));
    let [r, g, b, a, next] = std::array::from_fn(|_| graph.add("rand", &[one], None));
    let rg = graph.add("vector2", &[r, g], None);
    let ba = graph.add("vector2", &[b, a], None);
    let rgba = graph.add("vector4", &[rg, ba], None);
    let result = graph.add("instance", &[rgba], None);
    let call = graph.nodes.last_mut().unwrap();
    call.type_code = 1024;
    call.instance = Some(materialcraft_core::GraphInstance {
        package: String::new(),
        graph: "pick_channels".into(),
        label: String::new(),
        inputs: vec![materialcraft_core::GraphPort { identifier: "channels".into(), type_code: 2048 }],
        outputs: Vec::new(),
        defaults: BTreeMap::from([("channels".into(), Value::Color([0.0, 0.0, 0.0, 1.0]))]),
    });
    call.source = Some(Ok(std::sync::Arc::new(body)));
    let variables = BTreeMap::new();
    let context = EvaluationContext {
        scope: super::FunctionScope::FxMap,
        position: [0.5; 2],
        size: [16.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &variables,
    };
    let run = |graph: &FunctionGraph| compile(graph, 31).unwrap().evaluate(&context, &mut EvaluationState::default()).unwrap();
    let draws = [0x3f34_57e6, 0x3f5a_33c2, 0x3f46_fab2].map(f32::from_bits);
    graph.output = Some(result);
    assert_eq!(run(&graph), Value::Float3(draws));

    let outside = graph.add("swizzle1", &[rgba], Some(Value::Int(3)));
    let inside = graph.add("swizzle1", &[result], Some(Value::Int(0)));
    let pair = graph.add("vector2", &[outside, inside], None);
    graph.output = Some(graph.add("vector3", &[pair, next], None));
    assert_eq!(run(&graph), Value::Float3([draws[0], draws[0], draws[1]]));
}

#[test]
fn pattern_call_arguments_follow_the_returned_channels() {
    let names = ["red", "green", "blue"];
    let mut body = FunctionGraph::default();
    let [red, green, blue] = names.map(|n| body.add("get_float1", &[], Some(Value::Text(n.into()))));
    let bg = body.add("vector2", &[blue, green], None);
    body.output = Some(body.add("vector3", &[bg, red], None));

    let mut graph = FunctionGraph::default();
    let one = graph.add_constant(Value::Float(1.0));
    let args: [_; 3] = std::array::from_fn(|_| graph.add("rand", &[one], None));
    let result = graph.add("instance", &args, None);
    let call = graph.nodes.last_mut().unwrap();
    call.type_code = 1024;
    call.instance = Some(materialcraft_core::GraphInstance {
        package: String::new(),
        graph: "reverse_channels".into(),
        label: String::new(),
        inputs: names.map(|n| materialcraft_core::GraphPort { identifier: n.into(), type_code: 256 }).into(),
        outputs: Vec::new(),
        defaults: names.map(|n| (n.into(), Value::Float(0.0))).into(),
    });
    call.source = Some(Ok(std::sync::Arc::new(body)));
    let variables = BTreeMap::new();
    let context = EvaluationContext {
        scope: super::FunctionScope::FxMap,
        position: [0.5; 2],
        size: [16.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &variables,
    };
    graph.output = Some(result);
    let run = |graph: &FunctionGraph| compile(graph, 31).unwrap().evaluate(&context, &mut EvaluationState::default()).unwrap();
    let draws = [0x3f34_57e6, 0x3f5a_33c2, 0x3f46_fab2].map(f32::from_bits);
    assert_eq!(run(&graph), Value::Float3(draws));

    let selected = graph.add("swizzle1", &[result], Some(Value::Int(0)));
    let next = graph.add("rand", &[one], None);
    let pair = graph.add("vector2", &[selected, next], None);
    graph.output = Some(graph.add("vector3", &[pair, args[2]], None));
    assert_eq!(run(&graph), Value::Float3([draws[0], draws[1], draws[0]]));
}

#[test]
fn packed_random_ranges_preserve_the_unrounded_draw_in_each_lane() {
    let mut graph = FunctionGraph::default();
    let [a, b, c, d] = [0.999, -1.731053, 0.001, 0.352111].map(|x| graph.add_constant(Value::Float(x)));
    let left = graph.add("rand", &[a], None);
    let right = graph.add("rand", &[b], None);
    let pair = graph.add("vector2", &[left, right], None);
    let offsets = graph.add("vector2", &[c, d], None);
    let sum = graph.add("add", &[pair, offsets], None);
    let shared = graph.add("vector4", &[sum, pair], None);
    let plus = graph.add("add", &[left, c], None);
    let minus = graph.add("sub", &[d, left], None);
    let twice = graph.add("vector2", &[plus, minus], None);
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        position: [1.5 / 16.0, 0.5 / 16.0],
        size: [16.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &BTreeMap::new(),
    };
    for (output, expected) in [
        (sum, Value::Float2([0x3ea1_2485, 0xbf63_be1d].map(f32::from_bits))),
        (shared, Value::Float4([0x3ea1_2485, 0xbf63_be1e, 0x3ea0_a173, 0xbf9e_f108].map(f32::from_bits))),
        (twice, Value::Float2([0x3ea1_2485, 0x3d1d_3394].map(f32::from_bits))),
    ] {
        graph.output = Some(output);
        assert_eq!(compile(&graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap(), expected);
    }
}

#[test]
fn a_single_use_pixel_random_range_contracts_with_its_offset() {
    let mut graph = FunctionGraph::default();
    let range = graph.add_constant(Value::Float(0.999));
    let offset = graph.add_constant(Value::Float(0.001));
    let draw = graph.add("rand", &[range], None);
    let sum = graph.add("add", &[draw, offset], None);
    let reverse = graph.add("sub", &[offset, draw], None);
    let shared = graph.add("vector2", &[sum, draw], None);
    for (output, x, expected) in [
        (sum, 2, Value::Float(f32::from_bits(0x3f35_63cb))),
        (reverse, 4, Value::Float(f32::from_bits(0xbeb1_9951))),
        (shared, 2, Value::Float2([0x3f35_63ca, 0x3f35_2241].map(f32::from_bits))),
    ] {
        graph.output = Some(output);
        let context = EvaluationContext {
            scope: super::FunctionScope::PixelProcessor,
            position: [(x as f32 + 0.5) / 16.0, 0.5 / 16.0],
            size: [16.0; 2],
            images: &[],
            image_formats: &[],
            tile: true,
            variables: &BTreeMap::new(),
        };
        let value = compile(&graph, 17).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
        assert_eq!(value, expected);
    }
}

#[test]
fn pixel_random_starts_at_the_normalized_texel_center_and_graph_seed() {
    let mut graph = FunctionGraph::default();
    let one = graph.add_constant(Value::Float(1.0));
    graph.output = Some(graph.add("rand", &[one], None));
    for (seed, width, height, x, y, first_draw) in
        [(0, 32, 32, 0, 0, 6_046_590), (0, 32, 32, 16, 1, 1_903_037), (1, 32, 32, 31, 31, 6_085_635), (42, 32, 16, 16, 1, 3_941_566)]
    {
        let context = EvaluationContext {
            scope: super::FunctionScope::PixelProcessor,
            position: [(x as f32 + 0.5) / width as f32, (y as f32 + 0.5) / height as f32],
            size: [width as f32, height as f32],
            images: &[],
            image_formats: &[],
            tile: true,
            variables: &BTreeMap::new(),
        };
        let program = compile(&graph, seed).unwrap();
        let value = program.evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
        assert_eq!(value, Value::Float(first_draw as f32 / 8_388_608.0));
    }
}

#[test]
fn measured_pixel_random_draws_share_one_stream() {
    let mut graph = FunctionGraph::default();
    let one = graph.add_constant(Value::Float(1.0));
    let negative = graph.add_constant(Value::Float(-2.0));
    let factor = graph.add_constant(Value::Float(0.375));
    let first = graph.add("rand", &[one], None);
    let second = graph.add("rand", &[one], None);
    let third = graph.add("rand", &[negative], None);
    let fourth = graph.add("rand", &[factor], None);
    let pair1 = graph.add("vector2", &[first, second], None);
    let pair2 = graph.add("vector2", &[third, fourth], None);
    graph.output = Some(graph.add("vector4", &[pair1, pair2], None));
    let variables = BTreeMap::new();
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [32.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &variables,
    };
    let program = compile(&graph, 0).unwrap();
    for (seed, bits) in [(9_091_073, [1060669180, 1057720276, 1034667712, 1054479120]), (3_342_178, [1058664790, 1044635448, 1061348922, 1062659162])] {
        let mut state = crate::random::FunctionRandom::image(seed);
        let [a, b, c, d] = bits.map(f32::from_bits);
        assert_eq!(program.evaluate_with_random(&context, &mut EvaluationState::default(), &mut state).unwrap(), Value::Float4([a, b, -2.0 * c, 0.375 * d]));
    }
}

#[test]
fn pixel_lerp_fuses_the_scaled_difference() {
    let operands = [0xc04928a6, 0xbfc9a1db, 0xbefff6a8].map(|bits| Value::Float(f32::from_bits(bits)));
    assert_eq!(pixel_operation("lerp", &operands, false), Value::Float(f32::from_bits(0xc07b52ad)));
    assert_eq!(pixel_operation("lerp", &operands, true), Value::Float(f32::from_bits(0xc07b52ae)));
}

#[test]
fn system_variables_resolve_with_their_declared_types() {
    for (operation, name, expected) in [
        ("get_integer1", "$pixelratio", Value::Int(0)),
        ("get_float3", "$physicalsize", Value::Float3([0.0; 3])),
        ("get_integer2", "$uvtile", Value::Int2([0; 2])),
        ("get_integer1", "$normalformat", Value::Int(0)),
    ] {
        let mut graph = FunctionGraph::default();
        graph.output = Some(graph.add(operation, &[], Some(Value::Text(name.into()))));

        assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), expected, "{name}");
    }
}

#[test]
fn every_native_function_executes_with_its_declared_ports() {
    let mut covered = BTreeSet::new();
    for definition in FUNCTIONS {
        let name = definition.id;

        assert!(covered.insert(name), "duplicate {name}");

        let mut graph = FunctionGraph::default();
        let mut variables = BTreeMap::new();
        let mut setting = default_value(name);
        let mut arguments = vec![Value::Float(0.5); definition.inputs.len()];
        match name {
            "get_bool" => {
                variables.insert("variable".into(), Value::Bool(true));
            }
            "get_string" => {
                variables.insert("variable".into(), Value::Text("test".into()));
            }
            "get_float1" => {
                variables.insert("variable".into(), Value::Float(1.0));
            }
            "get_float2" => {
                variables.insert("variable".into(), Value::Float2([1.0; 2]));
            }
            "get_float3" => {
                variables.insert("variable".into(), Value::Float3([1.0; 3]));
            }
            "get_float4" => {
                variables.insert("variable".into(), Value::Float4([1.0; 4]));
            }
            "get_integer1" => {
                variables.insert("variable".into(), Value::Int(1));
            }
            "get_integer2" => {
                variables.insert("variable".into(), Value::Int2([1; 2]));
            }
            "get_integer3" => {
                variables.insert("variable".into(), Value::Int3([1; 3]));
            }
            "get_integer4" => {
                variables.insert("variable".into(), Value::Int4([1; 4]));
            }
            "and" | "or" | "not" => arguments.fill(Value::Bool(true)),
            "atan2" | "samplecol" | "samplelum" => arguments = vec![Value::Float2([0.5; 2])],
            "vector3" => arguments = vec![Value::Float2([1.0; 2]), Value::Float(2.0)],
            "vector4" => arguments = vec![Value::Float3([1.0; 3]), Value::Float(2.0)],
            "ivector2" => arguments = vec![Value::Int(1), Value::Int(2)],
            "ivector3" => arguments = vec![Value::Int2([1; 2]), Value::Int(2)],
            "ivector4" => arguments = vec![Value::Int3([1; 3]), Value::Int(2)],
            "tofloat" => arguments = vec![Value::Int(1)],
            "tofloat2" => arguments = vec![Value::Int2([1; 2])],
            "tofloat3" => arguments = vec![Value::Int3([1; 3])],
            "tofloat4" => arguments = vec![Value::Int4([1; 4])],
            "toint2" => arguments = vec![Value::Float2([1.5; 2])],
            "toint3" => arguments = vec![Value::Float3([1.5; 3])],
            "toint4" => arguments = vec![Value::Float4([1.5; 4])],
            "ifelse" => arguments = vec![Value::Bool(false), Value::Float(1.0), Value::Float(2.0)],
            "while" => arguments = vec![Value::Float(0.0), Value::Bool(true), Value::Float(1.0)],
            _ if name.starts_with("iswizzle") => arguments = vec![Value::Int4([1, 2, 3, 4])],
            _ if name.starts_with("swizzle") => arguments = vec![Value::Float4([1.0, 2.0, 3.0, 4.0])],
            _ => {}
        }
        if name.starts_with("get_") || matches!(name, "set" | "isdefined") {
            setting = Some(Value::Text("variable".into()));
        }
        let inputs: Vec<_> = arguments.into_iter().map(|value| graph.add_constant(value)).collect();
        let node = graph.add(name, &inputs, setting);
        graph.output = Some(if name == "passthrough" { graph.add("neg", &[node], None) } else { node });
        evaluate(&graph, &variables).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    assert_eq!(covered.len(), 85);
}

#[test]
fn integer_math_preserves_all_32_bits_and_rejects_overflow() {
    assert_eq!(operation("add", &[Value::Int(16_777_217), Value::Int(1)], None).unwrap(), Value::Int(16_777_218));
    assert_eq!(operation("mul", &[Value::Int3([2, 3, 4]), Value::Int3([3, 4, 5])], None).unwrap(), Value::Int3([6, 12, 20]));
    assert!(operation("add", &[Value::Int(i32::MAX), Value::Int(1)], None).is_err());
    assert!(operation("div", &[Value::Int(i32::MIN), Value::Int(-1)], None).is_err());
    assert!(operation("div", &[Value::Float(1.0), Value::Float(0.0)], None).is_err());
}

#[test]
fn vectors_concatenate_swizzle_and_cast_without_broadcasting() {
    let vector = operation("vector4", &[Value::Float2([1.0, 2.0]), Value::Float2([3.0, 4.0])], None).unwrap();

    assert_eq!(vector, Value::Float4([1.0, 2.0, 3.0, 4.0]));
    assert_eq!(operation("swizzle3", &[vector], Some(Value::Int3([3, 0, 3]))).unwrap(), Value::Float3([4.0, 1.0, 4.0]));
    assert_eq!(operation("toint2", &[Value::Float2([-1.9, 2.9])], None).unwrap(), Value::Int2([-2, 2]));
    assert_eq!(operation("swizzle1", &[Value::Float2([0.0; 2])], Some(Value::Int(2))).unwrap(), Value::Float(0.0));
    assert_eq!(operation("pow2", &[Value::Float(3.0)], None).unwrap(), Value::Float(8.0));
}

#[test]
fn branches_are_lazy_and_sequence_orders_side_effects() {
    let mut graph = FunctionGraph::default();
    let condition = graph.add_constant(Value::Bool(true));
    let expected = graph.add_constant(Value::Int(17));
    let undefined = graph.add("get_integer1", &[], Some(Value::Text("missing".into())));
    graph.output = Some(graph.add("ifelse", &[condition, expected, undefined], None));

    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Int(17));

    let set = graph.add("set", &[expected], Some(Value::Text("stored".into())));
    let get = graph.add("get_integer1", &[], Some(Value::Text("stored".into())));
    graph.output = Some(graph.add("sequence", &[set, get], None));

    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Int(17));
}

#[test]
fn pixel_branches_share_an_assignment_across_nested_alternatives() {
    let mut graph = FunctionGraph::default();
    let name = Some(Value::Text("stored".into()));
    let initial = graph.add_constant(Value::Float(0.40625));
    let init = graph.add("set", &[initial], name.clone());
    let uv = graph.add("get_float2", &[], Some(Value::Text("$pos".into())));
    let x = graph.add("swizzle1", &[uv], Some(Value::Int(0)));
    let write = graph.add("set", &[x], name.clone());
    let after_write = graph.add("get_float1", &[], name.clone());
    let shared = graph.add("sequence", &[write, after_write], None);
    let one = graph.add_constant(Value::Float(1.0));
    let half = graph.add_constant(Value::Float(0.5));
    let upper = graph.add_constant(Value::Float(1.5));
    let first = graph.add("add", &[shared, one], None);
    let second = graph.add("mul", &[shared, half], None);
    let last = graph.add("get_float1", &[], name);
    let selector = graph.add("get_float1", &[], Some(Value::Text("selector".into())));
    let low = graph.add("lr", &[selector, half], None);
    let middle = graph.add("lr", &[selector, upper], None);
    let inner = graph.add("ifelse", &[middle, second, last], None);
    let across_arms = graph.add("ifelse", &[low, first, inner], None);
    let siblings = graph.add("ifelse", &[low, first, second], None);
    let within_arm = graph.add("ifelse", &[middle, siblings, last], None);
    let chosen = graph.add("ifelse", &[low, shared, last], None);
    let across_operands = graph.add("add", &[chosen, second], None);

    for (root, expected) in [(across_arms, [1.03125, 0.015625, 0.03125]), (within_arm, [1.03125, 0.015625, 0.40625]), (across_operands, [0.046875; 3])] {
        graph.output = Some(graph.add("sequence", &[init, root], None));
        let program = compile(&graph, 0).unwrap();
        for (selector, expected) in [0.0, 1.0, 2.0].into_iter().zip(expected) {
            let variables = BTreeMap::from([("selector".into(), Value::Float(selector))]);
            let context = EvaluationContext {
                scope: super::FunctionScope::PixelProcessor,
                position: [0.03125; 2],
                size: [16.0; 2],
                images: &[],
                image_formats: &[],
                tile: true,
                variables: &variables,
            };
            assert_eq!(program.evaluate_pixel(&context, &mut EvaluationState::default()), Ok(Value::Float(expected)), "root {root}, selector {selector}");
        }
    }
}

#[test]
fn time_and_tiling_variables_have_typed_defaults_and_host_overrides() {
    for (name, operation, default, supplied) in
        [("$time", "get_float1", Value::Float(0.0), Value::Float(2.5)), ("$tiling", "get_integer1", Value::Int(3), Value::Int(2))]
    {
        let mut graph = FunctionGraph::default();
        let get = graph.add(operation, &[], Some(Value::Text(name.into())));
        let defined = graph.add("isdefined", &[], Some(Value::Text(name.into())));
        graph.output = Some(get);

        for variables in [BTreeMap::new(), BTreeMap::from([(name.into(), supplied.clone())])] {
            let expected = variables.get(name).unwrap_or(&default);
            let context = EvaluationContext {
                image_formats: &[],
                scope: Default::default(),
                position: [0.5; 2],
                size: [1.0; 2],
                images: &[],
                tile: true,
                variables: &variables,
            };
            let program = compile(&graph, 0).unwrap();

            assert_eq!(program.evaluate(&context, &mut EvaluationState::default()).unwrap(), *expected);
            assert_eq!(program.evaluate_pixel(&context, &mut EvaluationState::default()).unwrap(), *expected);
        }

        assert!(evaluate(&graph, &BTreeMap::from([(name.into(), Value::Bool(false))])).unwrap_err().contains("wrong type"));

        graph.output = Some(defined);

        assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Bool(true));
    }

    let mut graph = FunctionGraph::default();
    graph.output = Some(graph.add("get_integer1", &[], Some(Value::Text("$tiling".into()))));
    let context = EvaluationContext {
        image_formats: &[],
        scope: Default::default(),
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: false,
        variables: &BTreeMap::new(),
    };

    assert_eq!(compile(&graph, 0).unwrap().evaluate(&context, &mut EvaluationState::default()).unwrap(), Value::Int(0));
}

#[test]
fn skipped_processor_loop_runs_its_initializer_and_returns_a_typed_zero() {
    let mut graph = FunctionGraph::default();
    let yes = graph.add_constant(Value::Bool(true));
    let initialize = graph.add("set", &[yes], Some(Value::Text("started".into())));
    let body = graph.add_constant(Value::Float2([4.0, 5.0]));
    let result = graph.add("while", &[initialize, yes, body], Some(Value::Int(2)));
    let started = graph.add("get_bool", &[], Some(Value::Text("started".into())));
    let after = graph.add("sequence", &[result, started], None);
    for scope in [super::FunctionScope::Node, super::FunctionScope::PixelProcessor, super::FunctionScope::ValueProcessor, super::FunctionScope::FxMap] {
        let context =
            EvaluationContext { scope, position: [0.0; 2], size: [16.0; 2], images: &[], image_formats: &[], tile: true, variables: &BTreeMap::new() };
        for (output, expected) in [(result, Value::Float2([0.0; 2])), (after, Value::Bool(true))] {
            graph.output = Some(output);
            assert_eq!(compile(&graph, 0).unwrap().evaluate(&context, &mut EvaluationState::default()).unwrap(), expected);
        }
    }
}

#[test]
fn loops_recompute_state_and_have_a_budget() {
    let mut graph = FunctionGraph::default();
    let zero = graph.add_constant(Value::Int(0));
    let one = graph.add_constant(Value::Int(1));
    let three = graph.add_constant(Value::Int(3));
    let init = graph.add("set", &[zero], Some(Value::Text("counter".into())));
    let get = graph.add("get_integer1", &[], Some(Value::Text("counter".into())));
    let condition = graph.add("gteq", &[get, three], None);
    let add = graph.add("add", &[get, one], None);
    let body = graph.add("set", &[add], Some(Value::Text("counter".into())));
    graph.output = Some(graph.add("while", &[init, condition, body], Some(Value::Int(-1))));

    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Int(3));

    let condition = graph.add_constant(Value::Bool(false));
    graph.output = Some(graph.add("while", &[zero, condition, one], Some(Value::Int(-1))));

    assert!(evaluate(&graph, &BTreeMap::new()).unwrap_err().contains("budget"));
}

#[test]
fn finite_loop_returns_the_settled_value_and_preserves_its_assignments() {
    let mut graph = FunctionGraph::default();
    let zero = graph.add_constant(Value::Int(0));
    let one = graph.add_constant(Value::Int(1));
    let limit = graph.add_constant(Value::Int(5));
    let never = graph.add_constant(Value::Bool(false));
    let counter = Some(Value::Text("counter".into()));
    let init = graph.add("set", &[zero], counter.clone());
    let read = graph.add("get_integer1", &[], counter.clone());
    let increment = graph.add("add", &[read, one], None);
    let bounded = graph.add("min", &[increment, limit], None);
    let write = graph.add("set", &[bounded], counter.clone());
    let result = graph.add("while", &[init, never, write], Some(Value::Int(65_536)));
    graph.output = Some(result);
    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Int(5));

    let after = graph.add("get_integer1", &[], counter);
    graph.output = Some(graph.add("sequence", &[result, after], None));
    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Int(5));
}

#[test]
fn finite_loop_distinguishes_the_sign_of_zero_in_its_state() {
    let mut graph = FunctionGraph::default();
    let zero = graph.add_constant(Value::Float(0.0));
    let never = graph.add_constant(Value::Bool(false));
    let sign = Some(Value::Text("sign".into()));
    let init = graph.add("set", &[zero], sign.clone());
    let value = graph.add("get_float1", &[], sign.clone());
    let negate = graph.add("neg", &[value], None);
    let write = graph.add("set", &[negate], sign);
    graph.output = Some(graph.add("while", &[init, never, write], Some(Value::Int(65))));
    let result = evaluate(&graph, &BTreeMap::new()).unwrap().as_f32().unwrap();
    assert_eq!(result.to_bits(), (-0.0_f32).to_bits());
}

#[test]
fn finite_loop_accumulates_a_repeating_sequence_at_float_precision() {
    let mut graph = FunctionGraph::default();
    let phase = Some(Value::Text("phase".into()));
    let sum = Some(Value::Text("sum".into()));
    let never = graph.add_constant(Value::Bool(false));
    let zero = graph.add_constant(Value::Float(0.0));
    let first = graph.add_constant(Value::Float(0.1));
    let second = graph.add_constant(Value::Float(0.3));
    let start_phase = graph.add("set", &[never], phase.clone());
    let start_sum = graph.add("set", &[zero], sum.clone());
    let init = graph.add("sequence", &[start_phase, start_sum], None);
    let previous_phase = graph.add("get_bool", &[], phase.clone());
    let flip = graph.add("not", &[previous_phase], None);
    let change_phase = graph.add("set", &[flip], phase.clone());
    let current_phase = graph.add("get_bool", &[], phase);
    let increment = graph.add("ifelse", &[current_phase, first, second], None);
    let current_sum = graph.add("get_float1", &[], sum.clone());
    let add = graph.add("add", &[increment, current_sum], None);
    let update = graph.add("set", &[add], sum.clone());
    let body = graph.add("sequence", &[change_phase, update], None);
    let result = graph.add("while", &[init, never, body], Some(Value::Int(65_537)));
    let expected = (0..65_537).fold(0.0_f32, |total, i| total + if i % 2 == 0 { 0.1 } else { 0.3 });
    graph.output = Some(result);
    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Float(expected));
    let after = graph.add("get_float1", &[], sum);
    graph.output = Some(graph.add("sequence", &[result, after], None));
    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Float(expected));
}

#[test]
fn finite_sum_checks_exit_and_increments_that_read_the_accumulator() {
    let mut graph = FunctionGraph::default();
    let sum = Some(Value::Text("sum".into()));
    let zero = graph.add_constant(Value::Float(0.0));
    let one = graph.add_constant(Value::Float(1.0));
    let four = graph.add_constant(Value::Float(4.0));
    let ten = graph.add_constant(Value::Float(10.0));
    let init = graph.add("set", &[zero], sum.clone());
    let total = graph.add("get_float1", &[], sum.clone());
    let small = graph.add("lr", &[total, four], None);
    let increment = graph.add("ifelse", &[small, one, four], None);
    let add = graph.add("add", &[total, increment], None);
    let body = graph.add("set", &[add], sum.clone());
    let read = graph.add("get_float1", &[], sum);
    let done = graph.add("gt", &[read, ten], None);
    graph.output = Some(graph.add("while", &[init, done, body], Some(Value::Int(65_537))));
    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Float(12.0));
}

#[test]
fn finite_loop_preserves_each_random_draw_with_a_constant_body_result() {
    let mut graph = FunctionGraph::default();
    let one = graph.add_constant(Value::Float(1.0));
    let never = graph.add_constant(Value::Bool(false));
    let draw = graph.add("rand", &[one], None);
    let body = graph.add("sequence", &[draw, one], None);
    let result = graph.add("while", &[one, never, body], Some(Value::Int(65)));
    let next_draw = graph.add("rand", &[one], None);
    graph.output = Some(graph.add("sequence", &[result, next_draw], None));
    let mut expected = crate::random::FunctionRandom::uniform(7);
    for _ in 0..65 {
        expected.draw();
    }
    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Float(expected.draw()));
}

#[test]
fn deep_graphs_shared_inputs_and_cycles_do_not_recurse() {
    let mut graph = FunctionGraph::constant(Value::Float(1.0));
    let mut previous = graph.output.unwrap();
    for _ in 0..3000 {
        previous = graph.add("passthrough", &[previous], None);
    }
    graph.output = Some(graph.add("abs", &[previous], None));

    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Float(1.0));

    graph.nodes[0].op = "passthrough".into();
    graph.nodes[0].inputs = vec![Some(previous)];

    assert!(compile(&graph, 0).err().unwrap().contains("cycle"));

    let mut graph = FunctionGraph::constant(Value::Float(1.0));
    let mut previous = graph.output.unwrap();
    for _ in 0..30 {
        previous = graph.add("add", &[previous, previous], None);
    }
    graph.output = Some(previous);

    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Float(1_073_741_824.0));
}

#[test]
fn pattern_expressions_read_inherited_values_and_keep_only_local_writes() {
    let mut expression = FunctionGraph::default();
    let counter = expression.add("get_float1", &[], Some(Value::Text("counter".into())));
    let one = expression.add_constant(Value::Float(1.0));
    let next = expression.add("add", &[counter, one], None);
    expression.output = Some(expression.add("set", &[next], Some(Value::Text("counter".into()))));
    let increment = compile(&expression, 17).unwrap();
    let inherited = compile(&FunctionGraph::input("counter", materialcraft_core::ValueType::Float), 17).unwrap();
    let mut state = EvaluationState::default();
    let mut random = crate::random::FunctionRandom::image(17);
    for initial in [0.5, -3.0] {
        let variables = BTreeMap::from([("counter".into(), Value::Float(initial)), ("label".into(), Value::Text("pattern".into()))]);
        let context = EvaluationContext {
            scope: super::FunctionScope::FxMap,
            position: [0.5; 2],
            size: [16.0; 2],
            images: &[],
            image_formats: &[],
            tile: true,
            variables: &variables,
        };
        let expected = Value::Float(initial + 1.0);
        assert_eq!(increment.evaluate_with_random(&context, &mut state, &mut random).unwrap(), expected);
        assert_eq!(state.assignments().get("counter"), Some(&expected));
        assert!(!state.variables().contains_key("label"));
        assert_eq!(inherited.evaluate_with_random(&context, &mut state, &mut random).unwrap(), Value::Float(initial));
        assert!(state.assignments().is_empty());
        assert!(state.variables().is_empty());
    }
}

#[test]
fn pixel_assignments_are_local_and_match_exported_evaluation() {
    let mut graph = FunctionGraph::default();
    let current = graph.add("get_float1", &[], Some(Value::Text("counter".into())));
    let one = graph.add_constant(Value::Float(1.0));
    let next = graph.add("add", &[current, one], None);
    let assign = graph.add("set", &[next], Some(Value::Text("counter".into())));
    let get = graph.add("get_float1", &[], Some(Value::Text("counter".into())));
    graph.output = Some(graph.add("sequence", &[assign, get], None));
    let program = compile(&graph, 7).unwrap();
    let mut pixel_state = EvaluationState::default();
    for initial in [3.0, 10.0, -2.0] {
        let variables = BTreeMap::from([("counter".into(), Value::Float(initial)), ("untouched".into(), Value::Text("keep".into()))]);
        let context = EvaluationContext {
            image_formats: &[],
            scope: Default::default(),
            position: [0.5; 2],
            size: [1.0; 2],
            images: &[],
            tile: true,
            variables: &variables,
        };
        let mut exported = EvaluationState::default();
        let expected = program.evaluate(&context, &mut exported).unwrap();
        for _ in 0..2 {
            assert_eq!(program.evaluate_pixel(&context, &mut pixel_state).unwrap(), expected);
        }
        assert_eq!(expected, Value::Float(initial + 1.0));
        assert_eq!(variables["counter"], Value::Float(initial));
        assert_eq!(exported.variables()["counter"], expected);
        assert_eq!(exported.variables()["untouched"], variables["untouched"]);
    }

    let read_only = compile(&FunctionGraph::constant(Value::Float(0.5)), 7).unwrap();
    let context = EvaluationContext {
        image_formats: &[],
        scope: Default::default(),
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: true,
        variables: &BTreeMap::new(),
    };

    assert_eq!(read_only.evaluate_pixel(&context, &mut pixel_state).unwrap(), Value::Float(0.5));
    assert!(pixel_state.variables().is_empty());
}

#[test]
fn pixel_random_seed_and_defined_variables_match_general_evaluation() {
    let mut graph = FunctionGraph::default();
    let maximum = graph.add_constant(Value::Float(1.0));
    let random = graph.add("rand", &[maximum], None);
    let defined = graph.add("isdefined", &[], Some(Value::Text("known".into())));
    let zero = graph.add_constant(Value::Float(0.0));
    graph.output = Some(graph.add("ifelse", &[defined, random, zero], None));
    let program = compile(&graph, 7).unwrap();
    let mut state = EvaluationState::default();
    for variables in [BTreeMap::from([("$randomseed".into(), Value::Int(42)), ("known".into(), Value::Bool(true))]), BTreeMap::new()] {
        let context = EvaluationContext {
            image_formats: &[],
            scope: Default::default(),
            position: [0.5; 2],
            size: [1.0; 2],
            images: &[],
            tile: true,
            variables: &variables,
        };
        let expected = program.evaluate(&context, &mut EvaluationState::default()).unwrap();

        assert_eq!(program.evaluate_pixel(&context, &mut state).unwrap(), expected);
    }
}

#[test]
fn function_calls_consume_the_callers_random_stream() {
    let mut random = FunctionGraph::default();
    let one = random.add_constant(Value::Float(1.0));
    let first = random.add("rand", &[one], None);
    random.output = Some(first);
    let source = std::sync::Arc::new(random.clone());
    let second = random.add("rand", &[one], None);
    random.output = Some(random.add("vector2", &[first, second], None));

    let mut caller = FunctionGraph::default();
    let mut calls = Vec::new();
    for _ in 0..2 {
        calls.push(caller.add("instance", &[], None));
        let node = caller.nodes.last_mut().unwrap();
        node.instance = Some(materialcraft_core::GraphInstance {
            label: String::new(),
            package: String::new(),
            graph: "random".into(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            defaults: BTreeMap::new(),
        });
        node.source = Some(Ok(source.clone()));
    }
    caller.output = Some(caller.add("vector2", &calls, None));
    let expected = evaluate(&random, &BTreeMap::new()).unwrap();
    let Value::Float2(values) = expected else { panic!("expected two random values") };

    assert_ne!(values[0], values[1]);
    assert_eq!(evaluate(&caller, &BTreeMap::new()).unwrap(), expected);
}

#[test]
fn shared_conditions_keep_the_value_computed_before_a_sequence_assignment() {
    let mut graph = FunctionGraph::default();
    let get = graph.add("get_integer1", &[], Some(Value::Text("counter".into())));
    let one = graph.add_constant(Value::Int(1));
    let condition = graph.add("lr", &[get, one], None);
    let assign = graph.add("set", &[one], Some(Value::Text("counter".into())));
    let after = graph.add("sequence", &[assign, condition], None);
    graph.output = Some(graph.add("sequence", &[condition, after], None));

    assert_eq!(evaluate(&graph, &BTreeMap::from([("counter".into(), Value::Int(0))])).unwrap(), Value::Bool(true));
}

#[test]
fn shared_assignment_nodes_run_once_per_function_evaluation() {
    let mut graph = FunctionGraph::default();
    let current = graph.add("get_integer1", &[], Some(Value::Text("counter".into())));
    let one = graph.add_constant(Value::Int(1));
    let next = graph.add("add", &[current, one], None);
    let assign = graph.add("set", &[next], Some(Value::Text("counter".into())));
    graph.output = Some(graph.add("ivector2", &[assign, assign], None));

    assert_eq!(evaluate(&graph, &BTreeMap::from([("counter".into(), Value::Int(0))])).unwrap(), Value::Int2([1, 1]));
}

#[test]
fn pixel_random_samples_vary_spatially_and_repeat_for_the_same_seed() {
    let mut graph = FunctionGraph::default();
    let one = graph.add_constant(Value::Float(1.0));
    graph.output = Some(graph.add("rand", &[one], None));
    let program = compile(&graph, 7).unwrap();
    let mut state = EvaluationState::default();
    let variables = BTreeMap::new();
    let samples = |state: &mut EvaluationState| {
        (0..16)
            .map(|x| {
                let context = EvaluationContext {
                    image_formats: &[],
                    scope: super::FunctionScope::PixelProcessor,
                    position: [(x as f32 + 0.5) / 16.0, 0.5],
                    size: [16.0; 2],
                    images: &[],
                    tile: true,
                    variables: &variables,
                };

                let value = program.evaluate_pixel(&context, state).unwrap();

                assert_eq!(value, program.evaluate(&context, &mut EvaluationState::default()).unwrap());

                value
            })
            .collect::<Vec<_>>()
    };

    let first = samples(&mut state);

    assert!(first.windows(2).all(|pair| pair[0] != pair[1]));
    assert_eq!(first, samples(&mut state));
}

#[test]
fn exposed_choices_are_integers_inside_functions() {
    let mut graph = FunctionGraph::default();
    let choice = graph.add("get_integer1", &[], Some(Value::Text("choice".into())));
    let one = graph.add_constant(Value::Int(1));
    graph.output = Some(graph.add("lr", &[choice, one], None));

    assert_eq!(evaluate(&graph, &BTreeMap::from([("choice".into(), Value::Enum(0))])).unwrap(), Value::Bool(true));

    graph.output = Some(graph.add("add", &[choice, one], None));

    assert_eq!(evaluate(&graph, &BTreeMap::from([("choice".into(), Value::Enum(2))])).unwrap(), Value::Int(3));
    assert!(evaluate(&graph, &BTreeMap::from([("choice".into(), Value::Enum(u32::MAX))])).is_err());
}

#[test]
fn a_constant_return_keeps_literal_precision_in_the_caller() {
    let variables = BTreeMap::from([("angle".into(), Value::Float(std::f32::consts::PI))]);
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &variables,
    };
    for (literal, expected) in [(true, 0x3c8efa35), (false, 0x3c8efa36)] {
        let mut inner = FunctionGraph::default();
        inner.output = Some(if literal {
            inner.add_constant(Value::Float(std::f32::consts::PI))
        } else {
            inner.add("get_float1", &[], Some(Value::Text("angle".into())))
        });
        let mut graph = FunctionGraph::default();
        let result = graph.add("instance", &[], None);
        let call = graph.nodes.last_mut().unwrap();
        call.instance = Some(materialcraft_core::GraphInstance {
            label: String::new(),
            package: String::new(),
            graph: "angle_value".into(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            defaults: BTreeMap::new(),
        });
        call.source = Some(Ok(std::sync::Arc::new(inner)));
        let divisor = graph.add_constant(Value::Float(180.0));
        graph.output = Some(graph.add("div", &[result, divisor], None));
        let value = compile(&graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
        assert_eq!(value, Value::Float(f32::from_bits(expected)));
    }
}

#[test]
fn nested_pixel_products_keep_the_callers_literal_arguments() {
    let mut inner = FunctionGraph::default();
    let names = ["a", "b", "c", "d"];
    let [a, b, c, d] = names.map(|name| inner.add("get_float1", &[], Some(Value::Text(name.into()))));
    let left = inner.add("mul", &[a, b], None);
    let right = inner.add("mul", &[c, d], None);
    inner.output = Some(inner.add("add", &[left, right], None));

    let mut graph = FunctionGraph::default();
    let a = graph.add_constant(Value::Float(0.1875));
    let b = graph.add_constant(Value::Float(1.25));
    let [c, d] = ["x", "y"].map(|name| graph.add("get_float1", &[], Some(Value::Text(name.into()))));
    graph.output = Some(graph.add("instance", &[a, b, c, d], None));
    let call = graph.nodes.last_mut().unwrap();
    call.instance = Some(materialcraft_core::GraphInstance {
        label: String::new(),
        package: String::new(),
        graph: "sum_products".into(),
        inputs: names.map(|name| materialcraft_core::GraphPort { identifier: name.into(), type_code: 256 }).into(),
        outputs: Vec::new(),
        defaults: names.into_iter().map(|name| (name.into(), Value::Float(0.0))).collect(),
    });
    call.source = Some(Ok(std::sync::Arc::new(inner)));
    let program = compile(&graph, 0).unwrap();
    for (x, y, expected) in [(0xbe7ff6a8, 0xbf68ddb9, 0x3eec6a9d), (0x3eccdf41, 0xbfaa12de, 0xbe9836b5)] {
        let variables = BTreeMap::from([("x".into(), Value::Float(f32::from_bits(x))), ("y".into(), Value::Float(f32::from_bits(y)))]);
        let context = EvaluationContext {
            scope: super::FunctionScope::PixelProcessor,
            position: [0.5; 2],
            size: [1.0; 2],
            images: &[],
            image_formats: &[],
            tile: true,
            variables: &variables,
        };
        assert_eq!(program.evaluate_pixel(&context, &mut EvaluationState::default()).unwrap(), Value::Float(f32::from_bits(expected)));
    }
}

#[test]
fn nested_function_assignments_escape_but_arguments_stay_local() {
    let mut inner = FunctionGraph::default();
    let argument = inner.add("get_integer1", &[], Some(Value::Text("argument".into())));
    inner.output = Some(inner.add("set", &[argument], Some(Value::Text("exported".into()))));
    let mut caller = FunctionGraph::default();
    let call = caller.add("instance", &[], None);
    let node = caller.nodes.last_mut().unwrap();
    node.instance = Some(materialcraft_core::GraphInstance {
        label: String::new(),
        package: String::new(),
        graph: "assign".into(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        defaults: BTreeMap::from([("argument".into(), Value::Int(42))]),
    });
    node.source = Some(Ok(std::sync::Arc::new(inner)));
    let exported = caller.add("get_integer1", &[], Some(Value::Text("exported".into())));
    let argument = caller.add("get_integer1", &[], Some(Value::Text("argument".into())));
    let pair = caller.add("ivector2", &[exported, argument], None);
    caller.output = Some(caller.add("sequence", &[call, pair], None));

    assert_eq!(evaluate(&caller, &BTreeMap::from([("argument".into(), Value::Int(7))])).unwrap(), Value::Int2([42, 7]));
}

#[test]
fn one_random_node_reuses_its_sample_across_vector_connections() {
    let mut graph = FunctionGraph::default();
    let one = graph.add_constant(Value::Float(1.0));
    let random = graph.add("rand", &[one], None);
    let pair = graph.add("vector2", &[random, random], None);
    graph.output = Some(graph.add("vector4", &[pair, pair], None));
    let Value::Float4(channels) = evaluate(&graph, &BTreeMap::new()).unwrap() else {
        panic!("expected color");
    };

    assert!(channels.iter().all(|value| *value == channels[0]), "{channels:?}");
}

#[test]
fn loops_refresh_random_samples_but_keep_each_iterations_fanout_equal() {
    let mut samples = FunctionGraph::default();
    let one = samples.add_constant(Value::Float(1.0));
    let draws: Vec<_> = (0..3).map(|_| samples.add("rand", &[one], None)).collect();
    let pair = samples.add("vector2", &draws[..2], None);
    samples.output = Some(samples.add("vector3", &[pair, draws[2]], None));
    let Value::Float3(expected) = evaluate(&samples, &BTreeMap::new()).unwrap() else {
        panic!("expected samples");
    };

    assert_ne!(expected[0], expected[2]);

    let mut graph = FunctionGraph::default();
    let zero = graph.add_constant(Value::Int(0));
    let one_int = graph.add_constant(Value::Int(1));
    let three = graph.add_constant(Value::Int(3));
    let one = graph.add_constant(Value::Float(1.0));
    let zero_pair = graph.add_constant(Value::Float2([0.0; 2]));
    let init = graph.add("set", &[zero], Some(Value::Text("counter".into())));
    let init = graph.add("sequence", &[init, zero_pair], None);
    let counter = graph.add("get_integer1", &[], Some(Value::Text("counter".into())));
    let exit = graph.add("gteq", &[counter, three], None);
    let next = graph.add("add", &[counter, one_int], None);
    let advance = graph.add("set", &[next], Some(Value::Text("counter".into())));
    let random = graph.add("rand", &[one], None);
    let pair = graph.add("vector2", &[random, random], None);
    let body = graph.add("sequence", &[advance, pair], None);
    graph.output = Some(graph.add("while", &[init, exit, body], Some(Value::Int(3))));

    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Float2([expected[2]; 2]));
}

#[test]
fn a_node_runs_once_so_readers_before_and_after_a_set_share_its_value() {
    let mut graph = FunctionGraph::default();
    let read = graph.add("get_integer1", &[], Some(Value::Text("count".into())));
    let one = graph.add_constant(Value::Int(1));
    let next = graph.add("add", &[read, one], None);
    let store = graph.add("set", &[next], Some(Value::Text("count".into())));
    graph.output = Some(graph.add("sequence", &[store, read], None));

    let variables = BTreeMap::from([("count".to_string(), Value::Int(4))]);

    assert_eq!(evaluate(&graph, &variables).unwrap(), Value::Int(4), "the Get node read 4 before the Set and keeps it");

    // A second Get node reads the variable when it runs, after the Set
    let later = graph.add("get_integer1", &[], Some(Value::Text("count".into())));
    graph.output = Some(graph.add("sequence", &[store, later], None));

    assert_eq!(evaluate(&graph, &variables).unwrap(), Value::Int(5));
}

#[test]
fn a_loop_evaluates_its_condition_and_body_again_on_every_iteration() {
    let mut graph = FunctionGraph::default();
    let zero = graph.add_constant(Value::Int(0));
    let start = graph.add("set", &[zero], Some(Value::Text("count".into())));
    let read = graph.add("get_integer1", &[], Some(Value::Text("count".into())));
    let limit = graph.add_constant(Value::Int(3));
    let reached = graph.add("gteq", &[read, limit], None);
    let one = graph.add_constant(Value::Int(1));
    let next = graph.add("add", &[read, one], None);
    let step = graph.add("set", &[next], Some(Value::Text("count".into())));
    graph.output = Some(graph.add("while", &[start, reached, step], Some(Value::Int(100))));

    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Int(3));
}

#[test]
fn writing_a_function_argument_keeps_its_wire_and_a_nested_free_assignment() {
    fn call(body: FunctionGraph, argument: &str, wire: Option<f32>) -> FunctionGraph {
        let mut graph = FunctionGraph::default();
        let input = wire.map(|value| graph.add_constant(Value::Float(value)));
        let result = graph.add("instance", &[], None);
        let node = graph.nodes.last_mut().unwrap();
        node.instance = Some(materialcraft_core::GraphInstance {
            label: String::new(),
            package: String::new(),
            graph: "assign_scalar".into(),
            inputs: vec![materialcraft_core::GraphPort { identifier: argument.into(), type_code: 256 }],
            outputs: vec![materialcraft_core::GraphPort { identifier: "output".into(), type_code: 256 }],
            defaults: BTreeMap::from([(argument.into(), Value::Float(0.0))]),
        });
        node.inputs = vec![input];
        node.source = Some(Ok(std::sync::Arc::new(body)));
        graph.output = Some(result);
        graph
    }

    let mut write = FunctionGraph::default();
    let value = write.add_constant(Value::Float(0.4375));
    write.output = Some(write.add("set", &[value], Some(Value::Text("amount".into()))));
    let direct = call(write.clone(), "amount", Some(0.625));
    assert_eq!(evaluate(&direct, &BTreeMap::new()), Ok(Value::Float(0.0)));

    let mut effect = FunctionGraph::default();
    let value = effect.add_constant(Value::Float(0.4375));
    let operand = effect.add("set", &[value], Some(Value::Text("effect".into())));
    effect.output = Some(effect.add("set", &[operand], Some(Value::Text("amount".into()))));
    let mut direct = call(effect, "amount", Some(0.625));
    let entered = direct.output.unwrap();
    let read = direct.add("get_float1", &[], Some(Value::Text("effect".into())));
    direct.output = Some(direct.add("sequence", &[entered, read], None));
    assert_eq!(evaluate(&direct, &BTreeMap::new()), Ok(Value::Float(0.0)));

    let mut parent = call(write, "unused", None);
    let write = parent.output.unwrap();
    let argument = parent.add("get_float1", &[], Some(Value::Text("amount".into())));
    parent.output = Some(parent.add("sequence", &[write, argument], None));
    let mut outer = call(parent, "amount", Some(0.625));
    let returned = outer.output.unwrap();
    let shared = outer.add("get_float1", &[], Some(Value::Text("amount".into())));
    outer.output = Some(outer.add("vector2", &[returned, shared], None));
    assert_eq!(evaluate(&outer, &BTreeMap::new()), Ok(Value::Float2([0.625, 0.4375])));
}

#[test]
fn a_partially_wired_call_reads_typed_zero_from_its_other_numeric_argument() {
    let mut body = FunctionGraph::default();
    body.output = Some(body.add("get_float3", &[], Some(Value::Text("position".into()))));
    let mut graph = FunctionGraph::default();
    let size = graph.add_constant(Value::Float(0.0));
    let position = graph.add_constant(Value::Float3([-0.25, 0.5, 0.75]));
    let call = graph.add("instance", &[], None);
    graph.output = Some(call);
    let node = graph.nodes.last_mut().unwrap();
    node.instance = Some(materialcraft_core::GraphInstance {
        label: String::new(),
        package: String::new(),
        graph: "read_position".into(),
        inputs: vec![
            materialcraft_core::GraphPort { identifier: "size".into(), type_code: 256 },
            materialcraft_core::GraphPort { identifier: "position".into(), type_code: 1024 },
        ],
        outputs: vec![materialcraft_core::GraphPort { identifier: "output".into(), type_code: 1024 }],
        defaults: BTreeMap::from([("size".into(), Value::Float(0.5)), ("position".into(), Value::Float3([1.0; 3]))]),
    });
    node.source = Some(Ok(std::sync::Arc::new(body)));
    node.inputs = vec![Some(size), None];
    let variables = BTreeMap::from([("position".into(), Value::Float3([0.875; 3]))]);
    assert_eq!(evaluate(&graph, &variables), Ok(Value::Float3([0.0; 3])));
    graph.nodes.last_mut().unwrap().inputs[1] = Some(position);
    assert_eq!(evaluate(&graph, &variables), Ok(Value::Float3([-0.25, 0.5, 0.75])));
    graph.nodes.last_mut().unwrap().inputs = vec![None, None];
    assert_eq!(evaluate(&graph, &variables), Ok(Value::Float3([1.0; 3])));
}

#[test]
fn defined_arguments_follow_wires_through_nested_calls() {
    fn caller(body: FunctionGraph, wired: bool) -> FunctionGraph {
        let mut graph = FunctionGraph::default();
        let zero = graph.add_constant(Value::Float(0.0));
        let result = graph.add("instance", &[], None);
        let call = graph.nodes.last_mut().unwrap();
        call.instance = Some(materialcraft_core::GraphInstance {
            label: String::new(),
            package: String::new(),
            graph: "probe".into(),
            inputs: vec![materialcraft_core::GraphPort { identifier: "amount".into(), type_code: 256 }],
            outputs: vec![materialcraft_core::GraphPort { identifier: "output".into(), type_code: 4 }],
            defaults: BTreeMap::from([("amount".into(), Value::Float(0.1875))]),
        });
        call.inputs = vec![wired.then_some(zero)];
        call.source = Some(Ok(std::sync::Arc::new(body)));
        graph.output = Some(result);
        graph
    }

    let mut body = FunctionGraph::default();
    let value = body.add_constant(Value::Float(0.4375));
    let set = body.add("set", &[value], Some(Value::Text("amount".into())));
    let defined = body.add("isdefined", &[], Some(Value::Text("amount".into())));
    body.output = Some(body.add("sequence", &[set, defined], None));
    let variables = BTreeMap::from([("amount".into(), Value::Float(0.625))]);
    for (outer, inner) in [(false, false), (true, false), (false, true), (true, true)] {
        let graph = caller(caller(body.clone(), inner), outer);
        assert_eq!(evaluate(&graph, &variables), Ok(Value::Bool(inner)));
    }

    let mut free = FunctionGraph::default();
    free.output = Some(free.add("isdefined", &[], Some(Value::Text("mark".into()))));
    let mut graph = caller(free, false);
    let call = graph.output.unwrap();
    let external = BTreeMap::from([("mark".into(), Value::Float(0.625))]);
    assert_eq!(evaluate(&graph, &external), Ok(Value::Bool(false)));
    let value = graph.add_constant(Value::Float(0.4375));
    let set = graph.add("set", &[value], Some(Value::Text("mark".into())));
    graph.output = Some(graph.add("sequence", &[set, call], None));
    assert_eq!(evaluate(&graph, &BTreeMap::new()), Ok(Value::Bool(true)));

    let mut graph = caller(body, false);
    let call = graph.output.unwrap();
    let assigned = graph.add("isdefined", &[], Some(Value::Text("amount".into())));
    graph.output = Some(graph.add("sequence", &[call, assigned], None));
    assert_eq!(evaluate(&graph, &BTreeMap::new()), Ok(Value::Bool(false)));
}

#[test]
fn variables_a_called_graph_sets_reach_its_caller_but_its_inputs_do_not() {
    let mut called = FunctionGraph::default();
    let input = called.add("get_integer1", &[], Some(Value::Text("amount".into())));
    called.output = Some(called.add("set", &[input], Some(Value::Text("shared".into()))));

    let mut caller = FunctionGraph::default();
    let call = caller.add("instance", &[], None);
    let node = caller.nodes.last_mut().unwrap();
    node.instance = Some(materialcraft_core::GraphInstance {
        label: String::new(),
        package: String::new(),
        graph: "called".into(),
        inputs: vec![materialcraft_core::GraphPort { identifier: "amount".into(), type_code: 16 }],
        outputs: vec![materialcraft_core::GraphPort { identifier: "output".into(), type_code: 16 }],
        defaults: BTreeMap::from([("amount".to_string(), Value::Int(9))]),
    });
    node.inputs = vec![None];
    node.source = Some(Ok(std::sync::Arc::new(called)));
    let shared = caller.add("get_integer1", &[], Some(Value::Text("shared".into())));
    caller.output = Some(caller.add("sequence", &[call, shared], None));

    assert_eq!(evaluate(&caller, &BTreeMap::new()).unwrap(), Value::Int(9));

    let leaked = caller.add("get_integer1", &[], Some(Value::Text("amount".into())));
    caller.output = Some(caller.add("sequence", &[call, leaked], None));

    assert_eq!(evaluate(&caller, &BTreeMap::new()).unwrap(), Value::Int(0));
}

#[test]
fn a_drop_down_parameter_reads_as_the_integer_of_its_option() {
    let mut graph = FunctionGraph::default();
    let option = graph.add("get_integer1", &[], Some(Value::Text("mode".into())));
    let two = graph.add_constant(Value::Int(2));
    graph.output = Some(graph.add("gteq", &[option, two], None));

    assert_eq!(evaluate(&graph, &BTreeMap::from([("mode".to_string(), Value::Enum(3))])).unwrap(), Value::Bool(true));
}

#[test]
fn an_unconnected_input_computes_with_zero_and_a_loop_without_a_condition_runs_its_limit() {
    let mut graph = FunctionGraph::default();
    let high = graph.add_constant(Value::Float(8.0));
    let half = graph.add_constant(Value::Float(0.5));
    graph.output = Some(graph.add("lerp", &[high, high, half], None));
    graph.nodes.last_mut().unwrap().inputs = vec![None, Some(high), Some(half)];

    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Float(4.0), "halfway between zero and 8");

    let mut graph = FunctionGraph::default();
    let zero = graph.add_constant(Value::Int(0));
    let start = graph.add("set", &[zero], Some(Value::Text("count".into())));
    let read = graph.add("get_integer1", &[], Some(Value::Text("count".into())));
    let one = graph.add_constant(Value::Int(1));
    let next = graph.add("add", &[read, one], None);
    let step = graph.add("set", &[next], Some(Value::Text("count".into())));
    graph.output = Some(graph.add("while", &[start, start, step], Some(Value::Int(3))));
    graph.nodes.last_mut().unwrap().inputs = vec![Some(start), None, Some(step)];

    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Int(3));
}

#[test]
fn booleans_compare_in_order() {
    assert_eq!(operation("gt", &[Value::Bool(true), Value::Bool(false)], None).unwrap(), Value::Bool(true));
    assert_eq!(operation("lreq", &[Value::Bool(true), Value::Bool(false)], None).unwrap(), Value::Bool(false));
}

#[test]
fn every_pixel_of_a_processor_draws_its_own_random_numbers() {
    let mut function = FunctionGraph::default();
    let range = function.add_constant(Value::Float(1.0));
    function.output = Some(function.add("rand", &[range], None));
    let definition = crate::find("pixel_processor").unwrap();
    let parameters = crate::resolve_params(definition, &BTreeMap::from([("function".into(), Value::Function(Box::new(function)))]));
    let variables = BTreeMap::new();
    let context = crate::CookContext {
        input_formats: &[],
        format: materialcraft_core::OutputFormat::F32,
        w: 8,
        h: 8,
        seed: 3,
        tile: true,
        inputs: &[],
        params: &parameters,
        resource: &|_| None,
        vars: &variables,
    };

    let image = (definition.cook)(&context).unwrap().remove(0);

    let first = image.get(0, 0, false)[0];
    let different = (0..8).flat_map(|y| (0..8).map(move |x| (x, y))).filter(|(x, y)| (image.get(*x, *y, false)[0] - first).abs() > 1e-4).count();

    assert!(different > 56, "{different} of 64 pixels differ from the first");
}

#[test]
fn unconnected_ifelse_branches_use_the_value_type_not_the_condition() {
    for (value, zero, type_code) in
        [(Value::Float(8.0), Value::Float(0.0), 256), (Value::Int(8), Value::Int(0), 16), (Value::Float2([8.0; 2]), Value::Float2([0.0; 2]), 512)]
    {
        for declared_type in [0, type_code] {
            for missing in [1, 2] {
                for condition_value in [false, true] {
                    let mut graph = FunctionGraph::default();
                    let condition = graph.add_constant(Value::Bool(condition_value));
                    let branch = graph.add_constant(value.clone());
                    graph.output = Some(graph.add("ifelse", &[condition, branch, branch], None));
                    let node = graph.nodes.last_mut().unwrap();
                    node.inputs[missing] = None;
                    node.type_code = declared_type;
                    let selected = if condition_value { 1 } else { 2 };
                    let expected = if selected == missing { zero.clone() } else { value.clone() };

                    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), expected);
                }
            }
        }
    }
}

#[test]
fn unconnected_ifelse_condition_stays_boolean_and_empty_branches_keep_declared_type() {
    let mut graph = FunctionGraph::default();
    let condition = graph.add_constant(Value::Bool(true));
    let branch = graph.add_constant(Value::Float(8.0));
    graph.output = Some(graph.add("ifelse", &[condition, branch, branch], None));
    let node = graph.nodes.last_mut().unwrap();
    node.inputs = vec![None, Some(branch), None];
    node.type_code = 0;

    assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Float(0.0));

    for condition_value in [false, true] {
        let mut graph = FunctionGraph::default();
        let condition = graph.add_constant(Value::Bool(condition_value));
        graph.output = Some(graph.add("ifelse", &[condition, condition, condition], None));
        let node = graph.nodes.last_mut().unwrap();
        node.inputs = vec![Some(condition), None, None];
        node.type_code = 512;

        assert_eq!(evaluate(&graph, &BTreeMap::new()).unwrap(), Value::Float2([0.0; 2]));
    }
}

#[test]
fn packed_loop_products_keep_the_factors_from_the_current_iteration() {
    let [r, g, b, weight] = [21.0_f32, 89.0, 149.0, 99.0].map(|code| code / 255.0);
    let variables = BTreeMap::from([("color".into(), Value::Float3([r, g, b])), ("weight".into(), Value::Float(weight))]);
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &variables,
    };
    for nested_read in [false, true] {
        let mut graph = FunctionGraph::default();
        let name = Some(Value::Text("total".into()));
        let zero = graph.add_constant(Value::Float(0.0));
        let never = graph.add_constant(Value::Bool(false));
        let start = graph.add_constant(Value::Float4([0.0; 4]));
        let initialize = graph.add("set", &[start], name.clone());
        let color = graph.add("get_float3", &[], Some(Value::Text("color".into())));
        let scale = graph.add("get_float1", &[], Some(Value::Text("weight".into())));
        let weighted = graph.add("mulscalar", &[color, scale], None);
        let term = graph.add("vector4", &[weighted, scale], None);
        let mut previous = graph.add("get_float4", &[], name.clone());
        if nested_read {
            // This inner loop clears the scale register after the vector has been packed.
            let inner = graph.add("while", &[zero, never, scale], Some(Value::Int(1)));
            previous = graph.add("sequence", &[inner, previous], None);
        }
        let sum = graph.add("add", &[term, previous], None);
        let store = graph.add("set", &[sum], name.clone());
        let body = graph.add("sequence", &[store, zero], None);
        let repeat = graph.add("while", &[initialize, never, body], Some(Value::Int(256)));
        let total = graph.add("get_float4", &[], name);
        graph.output = Some(graph.add("sequence", &[repeat, total], None));
        let Value::Float4(actual) = compile(&graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap() else {
            panic!("expected a four-component sum");
        };
        let [x, y, z] =
            [r, g, b].map(|component| (0..256).fold(0.0_f32, |sum, _| if nested_read { component * weight + sum } else { component.mul_add(weight, sum) }));
        let alpha = (0..256).fold(0.0_f32, |sum, _| sum + weight);
        assert_eq!(actual.map(f32::to_bits), [x, y, z, alpha].map(f32::to_bits), "nested read {nested_read}");
    }
}

#[test]
fn pixel_division_by_a_literal_unit_reciprocal_uses_the_original_factor() {
    let variables = BTreeMap::from([
        ("a".into(), Value::Float(f32::from_bits(0x4076_3400))),
        ("b".into(), Value::Float(f32::from_bits(0x3f68_c2ba))),
        ("unit".into(), Value::Float(1.0)),
    ]);
    let context = EvaluationContext {
        image_formats: &[],
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        tile: true,
        variables: &variables,
    };
    for (literal, expected) in [(true, 0x405f_da66), (false, 0x405f_da65)] {
        let mut graph = FunctionGraph::default();
        let [a, b, unit] = ["a", "b", "unit"].map(|name| graph.add("get_float1", &[], Some(Value::Text(name.into()))));
        let one = if literal { graph.add_constant(Value::Float(1.0)) } else { unit };
        let reciprocal = graph.add("div", &[one, b], None);
        let quotient = graph.add("div", &[a, reciprocal], None);
        graph.output = Some(graph.add("vector2", &[quotient, reciprocal], None));
        let value = compile(&graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
        assert_eq!(value, Value::Float2([expected, 0x3f8c_c7a2].map(f32::from_bits)), "literal unit {literal}");
    }
}

#[test]
fn pixel_division_keeps_the_original_length_across_vector_and_scalar_calls() {
    fn call(graph: &mut FunctionGraph, body: FunctionGraph, input: u32, kind: u32, default: Value) -> u32 {
        let id = graph.add("instance", &[input], None);
        let node = graph.nodes.last_mut().unwrap();
        node.instance = Some(materialcraft_core::GraphInstance {
            graph: "operation".into(),
            label: String::new(),
            package: String::new(),
            inputs: vec![materialcraft_core::GraphPort { identifier: "v".into(), type_code: kind }],
            outputs: Vec::new(),
            defaults: BTreeMap::from([("v".into(), default)]),
        });
        node.source = Some(Ok(std::sync::Arc::new(body)));
        id
    }

    let variables = ["a", "b", "c", "z"]
        .map(str::to_owned)
        .into_iter()
        .zip([0x3f80_d400, 0x3fb2_a18e, 0x3e74_0000, 0x3f80_0000].map(|bits| Value::Float(f32::from_bits(bits))))
        .collect();
    let context = EvaluationContext {
        scope: super::FunctionScope::PixelProcessor,
        position: [0.5; 2],
        size: [1.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &variables,
    };
    let mut normalize = FunctionGraph::default();
    let vector = normalize.add("get_float3", &[], Some(Value::Text("v".into())));
    let squared = normalize.add("dot", &[vector, vector], None);
    let length = normalize.add("sqrt", &[squared], None);
    let one = normalize.add_constant(Value::Float(1.0));
    let reciprocal = normalize.add("div", &[one, length], None);
    normalize.output = Some(normalize.add("mulscalar", &[vector, reciprocal], None));

    let mut quotient = FunctionGraph::default();
    let [a, v] = ["a", "v"].map(|name| quotient.add("get_float1", &[], Some(Value::Text(name.into()))));
    quotient.output = Some(quotient.add("div", &[a, v], None));
    for (constant_z, expected) in [(true, 0x3fdf_4c75), (false, 0x3fdf_4c76)] {
        let mut graph = FunctionGraph::default();
        let [c, b, z] = ["c", "b", "z"].map(|name| graph.add("get_float1", &[], Some(Value::Text(name.into()))));
        let xy = graph.add("vector2", &[c, b], None);
        let z = if constant_z { graph.add_constant(Value::Float(1.0)) } else { z };
        let xyz = graph.add("vector3", &[xy, z], None);
        let direction = call(&mut graph, normalize.clone(), xyz, 1024, Value::Float3([0.0; 3]));
        let den = graph.add("swizzle1", &[direction], Some(Value::Int(2)));
        let result = call(&mut graph, quotient.clone(), den, 256, Value::Float(0.0));
        graph.output = Some(graph.add("vector2", &[result, den], None));
        let value = compile(&graph, 0).unwrap().evaluate_pixel(&context, &mut EvaluationState::default()).unwrap();
        assert_eq!(value, Value::Float2([expected, 0x3f13_b1cf].map(f32::from_bits)), "constant Z {constant_z}");
    }
}
