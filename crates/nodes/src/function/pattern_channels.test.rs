use crate::function::{EvaluationContext, EvaluationState, FunctionScope, compile};
use materialcraft_core::{FunctionGraph, Value};
use std::collections::BTreeMap;

fn random_color(graph: &mut FunctionGraph, one: u32) -> u32 {
    let channels = [(); 4].map(|()| graph.add("rand", &[one], None));
    let lower = graph.add("vector2", &channels[..2], None);
    let upper = graph.add("vector2", &channels[2..], None);
    graph.add("vector4", &[lower, upper], None)
}

#[test]
fn selecting_a_computed_pattern_channel_preserves_the_next_random_draw() {
    for operation in ["ifelse", "mul", "lerp"] {
        for lane in [0, 3] {
            let mut graph = FunctionGraph::default();
            let one = graph.add_constant(Value::Float(1.0));
            let zero = graph.add_constant(Value::Float(0.0));
            let left = random_color(&mut graph, one);
            let right = random_color(&mut graph, one);
            let condition = graph.add("get_bool", &[], Some(Value::Text("choice".into())));
            let selected = match operation {
                "ifelse" => graph.add(operation, &[condition, left, right], None),
                "lerp" => {
                    let factor = graph.add_constant(Value::Float(0.375));
                    graph.add(operation, &[left, right, factor], None)
                }
                _ => graph.add(operation, &[left, right], None),
            };
            let channel = graph.add("swizzle1", &[selected], Some(Value::Int(lane)));
            let observed = graph.add("mul", &[channel, zero], None);
            let next = graph.add("rand", &[one], None);
            graph.output = Some(graph.add("add", &[observed, next], None));

            for choice in [true, false] {
                let variables = BTreeMap::from([("choice".into(), Value::Bool(choice))]);
                let context = EvaluationContext {
                    scope: FunctionScope::FxMap,
                    position: [0.5; 2],
                    size: [16.0; 2],
                    images: &[],
                    image_formats: &[],
                    tile: true,
                    variables: &variables,
                };
                for (seed, after_one, after_two) in [(0, 0x3f08_683c, 0x3f41_3194), (17, 0x3f4e_0964, 0x3ec2_8a10)] {
                    let graph = compile(&graph, seed).unwrap();
                    let expected = f32::from_bits(if operation == "ifelse" { after_one } else { after_two });
                    assert_eq!(graph.evaluate(&context, &mut EvaluationState::default()).unwrap(), Value::Float(expected), "{operation} channel {lane}");
                }
            }
        }
    }
}

#[test]
fn assigning_a_computed_pattern_vector_draws_the_operands_of_each_channel_together() {
    let variables = BTreeMap::new();
    let context = EvaluationContext {
        scope: FunctionScope::FxMap,
        position: [0.5; 2],
        size: [16.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &variables,
    };
    // Read each channel back from the assigned vector at three measured stamp seeds.
    for (operation, expected) in [
        ("product", [[0x3c41a72f, 0x3e1491a6], [0x3e74d565, 0x3d31859f], [0x3ed198e7, 0x3e1fd4d9]]),
        ("sum", [[0x3e7ec4c0, 0x3f432dc0], [0x3f918972, 0x3f66fe54], [0x3fb19ce2, 0x3f4a5a34]]),
        ("lerp", [[0x3e0edcac, 0x3ec13990], [0x3efdf41e, 0x3f0d1cf2], [0x3f42c8d5, 0x3ecbb690]]),
        ("scalar", [[0x3c41a72f, 0x3cbfad3c], [0x3e74d565, 0x3f3b1f1e], [0x3ed198e7, 0x3e30d4b5]]),
        ("multiply_shared", [[0x3c41a72f, 0x3d8a8af7], [0x3e74d565, 0x3e72cfb6], [0x3ed198e7, 0x3ec7ebed]]),
    ] {
        let mut graph = FunctionGraph::default();
        let unit = graph.add_constant(Value::Float(1.0));
        let [a, b, c, d] = std::array::from_fn(|_| graph.add("rand", &[unit], None));
        let left = graph.add("vector2", &[a, b], None);
        let right = graph.add("vector2", &[c, if operation == "multiply_shared" { a } else { d }], None);
        let result = match operation {
            "lerp" => {
                let factor = graph.add_constant(Value::Float(0.375));
                graph.add("lerp", &[left, right, factor], None)
            }
            "scalar" => graph.add("mulscalar", &[left, c], None),
            "sum" => graph.add("add", &[left, right], None),
            _ => graph.add("mul", &[left, right], None),
        };
        let name = Some(Value::Text("recorded".into()));
        let assigned = graph.add("set", &[result], name.clone());
        let read = graph.add("get_float2", &[], name);
        for lane in [0, 1] {
            let selected = graph.add("swizzle1", &[read], Some(Value::Int(lane as i32)));
            graph.output = Some(graph.add("sequence", &[assigned, selected], None));
            for (seed, words) in [11_038_009, 9_766_418, 22_858_730].into_iter().zip(expected) {
                let result = compile(&graph, seed).unwrap().evaluate(&context, &mut EvaluationState::default()).unwrap();
                assert_eq!(result, Value::Float(f32::from_bits(words[lane])), "{operation}, {seed}, channel {lane}");
            }
        }
    }
}

fn weighted_pattern_argument(width: usize, order: Option<[i32; 3]>, repeat_swizzle: bool) -> FunctionGraph {
    let (getter, default) = match width {
        2 => ("get_float2", Value::Float2([0.0; 2])),
        3 => ("get_float3", Value::Float3([0.0; 3])),
        4 => ("get_float4", Value::Float4([0.0; 4])),
        _ => panic!("unsupported fixture width"),
    };
    let mut body = FunctionGraph::default();
    let point = body.add(getter, &[], Some(Value::Text("point".into())));
    body.output = Some(if let Some(order) = order {
        let mut terms = Vec::new();
        for (lane, weight) in order.into_iter().zip([0.5, 0.25, 0.125]) {
            let channel = if repeat_swizzle {
                let repeated = body.add("swizzle3", &[point], Some(Value::Int3([lane; 3])));
                body.add("swizzle1", &[repeated], Some(Value::Int(0)))
            } else {
                body.add("swizzle1", &[point], Some(Value::Int(lane)))
            };
            let weight = body.add_constant(Value::Float(weight));
            terms.push(body.add("mul", &[channel, weight], None));
        }
        let pair = body.add("add", &terms[..2], None);
        body.add("add", &[pair, terms[2]], None)
    } else {
        body.add_constant(Value::Float(0.375))
    });

    let mut graph = FunctionGraph::default();
    let unit = graph.add_constant(Value::Float(1.0));
    let draws: Vec<_> = (0..width).map(|_| graph.add("rand", &[unit], None)).collect();
    let next = graph.add("rand", &[unit], None);
    let mut packed = graph.add("vector2", &draws[..2], None);
    for (&draw, operation) in draws[2..].iter().zip(["vector3", "vector4"]) {
        packed = graph.add(operation, &[packed, draw], None);
    }
    let sample = graph.add("instance", &[packed], None);
    let node = graph.nodes.last_mut().unwrap();
    node.instance = Some(materialcraft_core::GraphInstance {
        graph: "weighted_components".into(),
        label: String::new(),
        package: String::new(),
        inputs: vec![materialcraft_core::GraphPort { identifier: "point".into(), type_code: crate::function::value_type::type_code(&default) }],
        outputs: vec![],
        defaults: BTreeMap::from([("point".into(), default)]),
    });
    node.source = Some(Ok(std::sync::Arc::new(body)));
    let weight = graph.add_constant(Value::Float(0.0625));
    let tail = graph.add("mul", &[next, weight], None);
    graph.output = Some(graph.add("add", &[sample, tail], None));
    graph
}

fn measured_pattern_argument(graph: &FunctionGraph, seed: u32) -> Value {
    let variables = BTreeMap::new();
    let context = EvaluationContext {
        scope: FunctionScope::FxMap,
        position: [0.5; 2],
        size: [16.0; 2],
        images: &[],
        image_formats: &[],
        tile: true,
        variables: &variables,
    };
    compile(graph, seed).unwrap().evaluate(&context, &mut EvaluationState::default()).unwrap()
}

#[test]
fn a_returned_pattern_offset_finishes_each_component_in_read_order() {
    let mut graph = FunctionGraph::default();
    let one = graph.add_constant(Value::Float(1.0));
    let [x, y, scale] = std::array::from_fn(|_| graph.add("rand", &[one], None));
    let direction = graph.add("vector2", &[x, y], None);
    let scaled = graph.add("mulscalar", &[direction, scale], None);
    let origin = graph.add_constant(Value::Float2([0.0, 0.0]));
    graph.output = Some(graph.add("add", &[origin, scaled], None));

    for (seed, words) in [(11_038_009, [0x3c41_a72f, 0x3cbf_ad3c]), (9_766_418, [0x3e74_d565, 0x3f3b_1f1e]), (22_858_730, [0x3ed1_98e7, 0x3e30_d4b5])] {
        let expected = Value::Float2(words.map(f32::from_bits));
        assert_eq!(measured_pattern_argument(&graph, seed), expected, "returned offset at seed {seed}");
    }
}

#[test]
fn arithmetic_in_a_called_pattern_function_reads_packed_components_on_first_use() {
    let all = [0x3e37_3b24, 0x3eed_4df9, 0x3f29_859c];
    for (order, repeat_swizzle, words) in [
        ([0, 1, 2], false, all),
        ([2, 1, 0], false, all),
        ([1, 2, 0], false, all),
        ([2, 0, 0], false, [0x3e0e_9eda, 0x3f03_aff7, 0x3f2a_7d9c]),
        ([1, 1, 2], false, [0x3e2d_9372, 0x3ebd_2261, 0x3f4c_d582]),
        ([2, 1, 0], true, all),
    ] {
        let graph = weighted_pattern_argument(3, Some(order), repeat_swizzle);
        for (seed, word) in [11_038_009, 9_766_418, 22_858_730].into_iter().zip(words) {
            assert_eq!(measured_pattern_argument(&graph, seed), Value::Float(f32::from_bits(word)), "{order:?}, {repeat_swizzle}, {seed}");
        }
    }
}

#[test]
fn padded_and_unused_components_of_pattern_arguments_consume_no_draw() {
    for (width, order, word) in [
        (2, Some([1, 0, 1]), 0x3e1e_1926),
        (2, Some([3, 1, 3]), 0x3d4d_a9dc),
        (2, Some([3, 2, 3]), 0x3c3d_4b90),
        (3, Some([3, 0, 2]), 0x3d9d_db54),
        (3, None, 0x3ec5_ea5c),
        (4, Some([3, 1, 0]), 0x3e37_3b24),
        (4, Some([2, 3, 2]), 0x3e1e_1926),
    ] {
        let graph = weighted_pattern_argument(width, order, false);
        assert_eq!(measured_pattern_argument(&graph, 11_038_009), Value::Float(f32::from_bits(word)), "{width}, {order:?}");
    }
}
