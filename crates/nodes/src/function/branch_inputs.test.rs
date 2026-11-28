use crate::function::{EvaluationContext, EvaluationState, FunctionScope, compile};
use materialcraft_core::{FunctionGraph, Value};
use std::collections::BTreeMap;

fn constant_call(graph: &mut FunctionGraph, value: f32) -> u32 {
    let id = graph.add("instance", &[], None);
    let node = graph.nodes.last_mut().unwrap();
    node.instance = Some(materialcraft_core::GraphInstance {
        graph: "constant".into(),
        label: String::new(),
        package: String::new(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        defaults: BTreeMap::new(),
    });
    node.source = Some(Ok(std::sync::Arc::new(FunctionGraph::constant(Value::Float(value)))));
    id
}

#[test]
fn a_closed_function_call_preserves_shared_draw_order_in_a_pattern_assignment() {
    let mut graph = FunctionGraph::default();
    let first = graph.add("get_bool", &[], Some(Value::Text("first".into())));
    let unit = graph.add_constant(Value::Float(1.0));
    let shared = graph.add("rand", &[unit], None);
    let other = graph.add("rand", &[unit], None);
    let chosen = graph.add("ifelse", &[first, shared, other], None);
    let published = graph.add("set", &[chosen], Some(Value::Text("published".into())));
    let quarter = graph.add_constant(Value::Float(0.25));
    let weighted = graph.add("mul", &[shared, quarter], None);
    let total = graph.add("add", &[published, weighted], None);
    let angle = constant_call(&mut graph, std::f32::consts::PI);
    let eighth = graph.add_constant(Value::Float(0.125));
    let offset = graph.add("mul", &[angle, eighth], None);
    graph.output = Some(graph.add("add", &[total, offset], None));

    // Raw F32 tint words for the two choices on three independently measured stamp seeds.
    for (seed, expected) in [(11_038_009, [0x3f00baf2, 0x3f1faf8a]), (9_766_418, [0x3fa910f3, 0x3f3da6cd]), (22_858_730, [0x3f878418, 0x3fcc33e4])] {
        let program = compile(&graph, seed).unwrap();
        for (first, word) in [false, true].into_iter().zip(expected) {
            let variables = BTreeMap::from([("first".into(), Value::Bool(first))]);
            let context = EvaluationContext {
                scope: FunctionScope::FxMap,
                position: [0.5; 2],
                size: [16.0; 2],
                images: &[],
                image_formats: &[],
                tile: true,
                variables: &variables,
            };
            assert_eq!(program.evaluate(&context, &mut EvaluationState::default()).unwrap(), Value::Float(f32::from_bits(word)), "{seed}, {first}");
        }
    }
}

#[test]
fn reading_one_packed_channel_before_a_pure_call_consumes_only_its_draw() {
    for lane in [0, 1] {
        let mut graph = FunctionGraph::default();
        let unit = graph.add_constant(Value::Float(1.0));
        let [a, b, next] = std::array::from_fn(|_| graph.add("rand", &[unit], None));
        let pair = graph.add("vector2", &[a, b], None);
        let selected = graph.add("swizzle1", &[pair], Some(Value::Int(lane)));
        let saved = graph.add("set", &[selected], Some(Value::Text("published".into())));
        let quarter = graph.add_constant(Value::Float(0.25));
        let weighted = graph.add("mul", &[next, quarter], None);
        let sum = graph.add("add", &[saved, weighted], None);
        let offset = constant_call(&mut graph, 0.125);
        graph.output = Some(graph.add("add", &[sum, offset], None));
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
        for (seed, word) in [(11_038_009, 0x3ea6d4ee), (9_766_418, 0x3f1e3da6), (22_858_730, 0x3f98c3fa)] {
            let value = compile(&graph, seed).unwrap().evaluate(&context, &mut EvaluationState::default()).unwrap();
            assert_eq!(value, Value::Float(f32::from_bits(word)), "{seed}, lane {lane}");
        }
    }
}

#[test]
fn shared_conditional_draws_follow_their_arm_and_keep_their_original_position() {
    let mut graph = FunctionGraph::default();
    let choice = graph.add("get_bool", &[], Some(Value::Text("first".into())));
    let one = graph.add_constant(Value::Float(1.0));
    let quarter = graph.add_constant(Value::Float(0.25));
    let [a, b, c, next] = std::array::from_fn(|_| graph.add("rand", &[one], None));
    let left = graph.add("add", &[b, a], None);
    let right = graph.add("add", &[c, a], None);
    let either = graph.add("ifelse", &[choice, left, right], None);
    let false_only = graph.add("ifelse", &[choice, b, a], None);
    let sum = graph.add("add", &[next, a], None);
    let weighted = graph.add("mul", &[sum, quarter], None);
    let both_result = graph.add("add", &[either, weighted], None);
    let right_result = graph.add("add", &[false_only, weighted], None);
    let pair = graph.add("vector2", &[a, b], None);
    let control = graph.add("vector3", &[pair, c], None);

    for scope in [FunctionScope::Node, FunctionScope::PixelProcessor, FunctionScope::ValueProcessor, FunctionScope::FxMap] {
        for first in [true, false] {
            let values = BTreeMap::from([("first".into(), Value::Bool(first))]);
            let ctx = EvaluationContext { scope, size: [16.0; 2], position: [0.03125; 2], images: &[], image_formats: &[], tile: true, variables: &values };
            for seed in [3, 20] {
                graph.output = Some(control);
                let Value::Float3([r1, r2, r3]) = compile(&graph, seed).unwrap().evaluate(&ctx, &mut EvaluationState::default()).unwrap() else {
                    panic!("three draws")
                };
                let expected_both = r1 + r2 + (r3 + if first { r2 } else { r1 }) * 0.25;
                let expected_right = if first { r1 + (r3 + r2) * 0.25 } else { r1 + (r2 + r1) * 0.25 };
                for (output, expected) in [(both_result, expected_both), (right_result, expected_right)] {
                    graph.output = Some(output);
                    assert_eq!(
                        compile(&graph, seed).unwrap().evaluate(&ctx, &mut EvaluationState::default()).unwrap(),
                        Value::Float(expected),
                        "{scope:?}, {first}, {output}"
                    );
                }
            }
        }
    }
}

#[test]
fn an_assignment_shared_only_with_the_later_arm_keeps_the_earlier_arms_read() {
    let mut graph = FunctionGraph::default();
    let name = Some(Value::Text("tone".into()));
    let choice = graph.add("get_bool", &[], Some(Value::Text("first".into())));
    let start = graph.add_constant(Value::Float(0.125));
    let changed = graph.add_constant(Value::Float(0.625));
    let quarter = graph.add_constant(Value::Float(0.25));
    let init = graph.add("set", &[start], name.clone());
    let set = graph.add("set", &[changed], name.clone());
    let read = graph.add("get_float1", &[], name);
    let left = graph.add("ifelse", &[choice, set, read], None);
    let right = graph.add("ifelse", &[choice, read, set], None);
    let weighted = graph.add("mul", &[set, quarter], None);
    let left_sum = graph.add("add", &[left, weighted], None);
    let right_sum = graph.add("add", &[right, weighted], None);
    let left_result = graph.add("sequence", &[init, left_sum], None);
    let right_result = graph.add("sequence", &[init, right_sum], None);
    for scope in [FunctionScope::Node, FunctionScope::PixelProcessor, FunctionScope::ValueProcessor, FunctionScope::FxMap] {
        for first in [false, true] {
            let values = BTreeMap::from([("first".into(), Value::Bool(first))]);
            let ctx = EvaluationContext { scope, size: [16.0; 2], position: [0.03125; 2], images: &[], image_formats: &[], tile: true, variables: &values };
            for (root, expected) in [(left_result, 0.78125), (right_result, if first { 0.28125 } else { 0.78125 })] {
                graph.output = Some(root);
                assert_eq!(
                    compile(&graph, 0).unwrap().evaluate(&ctx, &mut EvaluationState::default()).unwrap(),
                    Value::Float(expected),
                    "{scope:?}, {first}, {root}"
                );
            }
        }
    }
}

#[test]
fn a_later_random_call_preserves_the_draws_in_a_sequence_assignment() {
    // The second sequence input calls a separate graph that draws from the same stamp stream.
    for (vector, expected) in [
        (true, [[0x3e5c760a, 0x3ee8063f], [0x3f1b5b11, 0x3f297df4], [0x3f4c65e0, 0x3edef0a6]]),
        (false, [[0x3e1fa354, 0x3e74cd9a], [0x3f846c61, 0x3ee92f7e], [0x3f377cfc, 0x3fa0bebe]]),
    ] {
        let mut graph = FunctionGraph::default();
        let first = graph.add("get_bool", &[], Some(Value::Text("first".into())));
        let unit = graph.add_constant(Value::Float(1.0));
        let [a, b, c, d] = std::array::from_fn(|_| graph.add("rand", &[unit], None));
        let name = Some(Value::Text("firstValue".into()));
        let (saved, read) = if vector {
            let left = graph.add("vector2", &[a, b], None);
            let right = graph.add("vector2", &[c, d], None);
            let factor = graph.add_constant(Value::Float(0.375));
            let value = graph.add("lerp", &[left, right, factor], None);
            let saved = graph.add("set", &[value], name.clone());
            let read = graph.add("get_float2", &[], name.clone());
            let x = graph.add("swizzle1", &[read], Some(Value::Int(0)));
            let y = graph.add("swizzle1", &[read], Some(Value::Int(1)));
            (saved, graph.add("ifelse", &[first, y, x], None))
        } else {
            let value = graph.add("ifelse", &[first, a, b], None);
            let saved = graph.add("set", &[value], name.clone());
            let read = graph.add("get_float1", &[], name);
            let quarter = graph.add_constant(Value::Float(0.25));
            let shared = graph.add("mul", &[a, quarter], None);
            (saved, graph.add("add", &[read, shared], None))
        };
        let mut sampler = FunctionGraph::default();
        let range = sampler.add_constant(Value::Float(1.0));
        sampler.output = Some(sampler.add("rand", &[range], None));
        let sampled = graph.add("instance", &[], None);
        let node = graph.nodes.last_mut().unwrap();
        node.instance = Some(materialcraft_core::GraphInstance {
            graph: "next_sample".into(),
            label: String::new(),
            package: String::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            defaults: BTreeMap::new(),
        });
        node.source = Some(Ok(std::sync::Arc::new(sampler)));
        let eighth = graph.add_constant(Value::Float(0.125));
        let offset = graph.add("mul", &[sampled, eighth], None);
        let result = graph.add("add", &[read, offset], None);
        graph.output = Some(graph.add("sequence", &[saved, result], None));
        for (seed, words) in [11_038_009, 9_766_418, 22_858_730].into_iter().zip(expected) {
            let program = compile(&graph, seed).unwrap();
            for (first, word) in [false, true].into_iter().zip(words) {
                let variables = BTreeMap::from([("first".into(), Value::Bool(first))]);
                let context = EvaluationContext {
                    scope: FunctionScope::FxMap,
                    position: [0.5; 2],
                    size: [16.0; 2],
                    images: &[],
                    image_formats: &[],
                    tile: true,
                    variables: &variables,
                };
                assert_eq!(
                    program.evaluate(&context, &mut EvaluationState::default()).unwrap(),
                    Value::Float(f32::from_bits(word)),
                    "{seed}, {first}, {vector}"
                );
            }
        }
    }
}

#[test]
fn a_pattern_conditional_with_a_literal_test_leaves_unused_draws_for_later() {
    // A stored choice shares an expression with a later use. Only the inner switch is exposed.
    for (reversed, unwired) in [(false, false), (true, false), (false, true)] {
        let mut graph = FunctionGraph::default();
        let name = Some(Value::Text("chosen".into()));
        let inner = graph.add("get_bool", &[], Some(Value::Text("inner".into())));
        let one = graph.add_constant(Value::Float(1.0));
        let [a, t, e] = std::array::from_fn(|_| graph.add("rand", &[one], None));
        let fallback = graph.add_constant(Value::Float(0.3125));
        let theta = graph.add("ifelse", &[inner, t, fallback], None);
        let shared = graph.add("add", &[a, theta], None);
        let fixed = graph.add_constant(Value::Float(0.1875));
        let choice = if reversed {
            let yes = graph.add_constant(Value::Bool(true));
            graph.add("ifelse", &[yes, fixed, shared], None)
        } else {
            let low = if unwired { graph.add("get_float1", &[], Some(Value::Text("outer".into()))) } else { graph.add_constant(Value::Float(0.25)) };
            let high = graph.add_constant(Value::Float(0.5));
            let no = graph.add("gt", &[low, high], None);
            graph.add("ifelse", &[no, shared, fixed], None)
        };
        let saved = graph.add("set", &[choice], name.clone());
        let read = graph.add("get_float1", &[], name);
        let half = graph.add_constant(Value::Float(0.5));
        let quarter = graph.add_constant(Value::Float(0.25));
        let eighth = graph.add_constant(Value::Float(0.125));
        let next = graph.add("mul", &[e, half], None);
        let later = graph.add("mul", &[shared, quarter], None);
        let value = graph.add("mul", &[read, eighth], None);
        let sum = graph.add("add", &[next, later], None);
        let result = graph.add("add", &[sum, value], None);
        graph.output = Some(graph.add("sequence", &[saved, result], None));
        if unwired {
            let callee = std::mem::take(&mut graph);
            let inner = graph.add("get_bool", &[], Some(Value::Text("inner".into())));
            graph.output = Some(graph.add("instance", &[inner], None));
            let call = graph.nodes.last_mut().unwrap();
            call.inputs.insert(0, None);
            call.instance = Some(materialcraft_core::GraphInstance {
                graph: "nested_choice".into(),
                label: String::new(),
                package: String::new(),
                inputs: vec![
                    materialcraft_core::GraphPort { identifier: "outer".into(), type_code: 256 },
                    materialcraft_core::GraphPort { identifier: "inner".into(), type_code: 4 },
                ],
                outputs: Vec::new(),
                defaults: BTreeMap::from([("outer".into(), Value::Float(0.0)), ("inner".into(), Value::Bool(false))]),
            });
            call.source = Some(Ok(std::sync::Arc::new(callee)));
        }
        for (seed, words) in [(11_038_009, [0x3e570414, 0x3e64b294]), (9_766_418, [0x3ee92f65, 0x3f171545]), (22_858_730, [0x3f30619c, 0x3f365b11])] {
            let program = compile(&graph, seed).unwrap();
            for (flag, word) in [false, true].into_iter().zip(words) {
                let variables = BTreeMap::from([("inner".into(), Value::Bool(flag))]);
                let context = EvaluationContext {
                    scope: FunctionScope::FxMap,
                    position: [0.5; 2],
                    size: [16.0; 2],
                    images: &[],
                    image_formats: &[],
                    tile: true,
                    variables: &variables,
                };
                assert_eq!(
                    program.evaluate(&context, &mut EvaluationState::default()).unwrap(),
                    Value::Float(f32::from_bits(word)),
                    "{seed}, {flag}, {reversed}, {unwired}"
                );
            }
        }
    }
}

fn pattern_sample(graph: &FunctionGraph, seed: u32, variables: &BTreeMap<String, Value>) -> Value {
    let context =
        EvaluationContext { scope: FunctionScope::FxMap, position: [0.5; 2], size: [16.0; 2], images: &[], image_formats: &[], tile: true, variables };
    compile(graph, seed).unwrap().evaluate(&context, &mut EvaluationState::default()).unwrap()
}

fn sample_function(graph: &mut FunctionGraph, body: FunctionGraph, argument: Option<(&str, u32)>) -> u32 {
    let wires: Vec<_> = argument.iter().map(|(_, id)| *id).collect();
    let result = graph.add("instance", &wires, None);
    let node = graph.nodes.last_mut().unwrap();
    node.instance = Some(materialcraft_core::GraphInstance {
        graph: "sample".into(),
        package: String::new(),
        label: String::new(),
        outputs: Vec::new(),
        inputs: argument.iter().map(|(s, _)| materialcraft_core::GraphPort { identifier: (*s).into(), type_code: 256 }).collect(),
        defaults: argument.iter().map(|(s, _)| ((*s).into(), Value::Float(0.0))).collect(),
    });
    node.source = Some(Ok(std::sync::Arc::new(body)));
    result
}

#[test]
fn pattern_draws_in_a_shared_inner_choice_stay_conditional() {
    for called in [false, true] {
        let mut graph = FunctionGraph::default();
        let first = graph.add("get_bool", &[], Some(Value::Text("outer".into())));
        let second = graph.add("get_bool", &[], Some(Value::Text("inner".into())));
        let one = graph.add_constant(Value::Float(1.0));
        let a = graph.add("rand", &[one], None);
        let t = if called {
            let mut body = FunctionGraph::default();
            let unit = body.add_constant(Value::Float(1.0));
            body.output = Some(body.add("rand", &[unit], None));
            sample_function(&mut graph, body, None)
        } else {
            graph.add("rand", &[one], None)
        };
        let fallback = graph.add_constant(Value::Float(0.3125));
        let inner = graph.add("ifelse", &[second, t, fallback], None);
        let shared = graph.add("add", &[a, inner], None);
        let constant = graph.add_constant(Value::Float(0.1875));
        let choice = graph.add("ifelse", &[first, shared, constant], None);
        let name = Some(Value::Text("chosen".into()));
        let saved = graph.add("set", &[choice], name.clone());
        let read = graph.add("get_float1", &[], name);
        let next = graph.add("rand", &[one], None);
        let half = graph.add_constant(Value::Float(0.5));
        let quarter = graph.add_constant(Value::Float(0.25));
        let eighth = graph.add_constant(Value::Float(0.125));
        let offset = graph.add("mul", &[next, half], None);
        let later = graph.add("mul", &[shared, quarter], None);
        let stored = graph.add("mul", &[read, eighth], None);
        let sum = graph.add("add", &[offset, later], None);
        let result = graph.add("add", &[sum, stored], None);
        graph.output = Some(graph.add("sequence", &[saved, result], None));
        for (seed, words) in [
            (11038009, [0x3e380f7c, 0x3e898718, 0x3e5fb8ee, 0x3e8d7364]),
            (9766418, [0x3f19b678, 0x3f3bbfde, 0x3f269ff5, 0x3f5a223a]),
            (22858730, [0x3f0e09b6, 0x3f12c15a, 0x3f30d34c, 0x3f392892]),
        ] {
            for ((outer, inner), word) in [(false, false), (false, true), (true, false), (true, true)].into_iter().zip(words) {
                let variables = BTreeMap::from([("outer".into(), Value::Bool(outer)), ("inner".into(), Value::Bool(inner))]);
                assert_eq!(pattern_sample(&graph, seed, &variables), Value::Float(f32::from_bits(word)), "{seed}, {outer}, {inner}, {called}");
            }
        }
    }
}

#[test]
fn pattern_draws_with_computed_arguments_keep_their_place_across_a_choice() {
    for (called, expected) in [
        (false, [(11038009, [0x3f0468bc, 0x3ec2006a]), (9766418, [0x3f94855e, 0x3f70a852]), (22858730, [0x402f3ff5, 0x3ff5be5e])]),
        (true, [(11038009, [0xbeafa1b4, 0xbe474368]), (9766418, [0x3f4de37f, 0x3f33e37f]), (22858730, [0x3f16768b, 0x3ef8ed16])]),
    ] {
        let mut graph = FunctionGraph::default();
        let flag = graph.add("get_bool", &[], Some(Value::Text("switch".into())));
        let unit = graph.add_constant(Value::Float(1.0));
        let half = graph.add_constant(Value::Float(0.5));
        let sample = if called {
            let weight = graph.add("get_float1", &[], Some(Value::Text("weight".into())));
            let chance = graph.add("mul", &[weight, half], None);
            let mut body = FunctionGraph::default();
            let one = body.add_constant(Value::Float(1.0));
            let u = body.add("rand", &[one], None);
            let p = body.add("get_float1", &[], Some(Value::Text("chance".into())));
            let test = body.add("lr", &[u, p], None);
            let minus_one = body.add_constant(Value::Float(-1.0));
            body.output = Some(body.add("ifelse", &[test, minus_one, one], None));
            sample_function(&mut graph, body, Some(("chance", chance)))
        } else {
            let count = graph.add("get_integer1", &[], Some(Value::Text("count".into())));
            let pair = graph.add("ivector2", &[count, count], None);
            let floats = graph.add("tofloat2", &[pair], None);
            let scale = graph.add("swizzle1", &[floats], Some(Value::Int(0)));
            let zero = graph.add_constant(Value::Float(0.0));
            let bound = graph.add("max", &[zero, scale], None);
            graph.add("rand", &[bound], None)
        };
        let fixed = graph.add_constant(Value::Float(0.1875));
        let choice = graph.add("ifelse", &[flag, fixed, sample], None);
        let name = Some(Value::Text("picked".into()));
        let saved = graph.add("set", &[choice], name.clone());
        let read = graph.add("get_float1", &[], name);
        let next = graph.add("rand", &[unit], None);
        let quarter = graph.add_constant(Value::Float(0.25));
        let eighth = graph.add_constant(Value::Float(0.125));
        let offset = graph.add("mul", &[next, half], None);
        let weighted = graph.add("mul", &[sample, quarter], None);
        let old = graph.add("mul", &[read, eighth], None);
        let sum = graph.add("add", &[offset, weighted], None);
        let result = graph.add("add", &[sum, old], None);
        graph.output = Some(graph.add("sequence", &[saved, result], None));
        for (seed, words) in expected {
            for (flag, word) in [false, true].into_iter().zip(words) {
                let variables = BTreeMap::from([("switch".into(), Value::Bool(flag)), ("weight".into(), Value::Float(0.5)), ("count".into(), Value::Int(7))]);
                assert_eq!(pattern_sample(&graph, seed, &variables), Value::Float(f32::from_bits(word)), "{seed}, {flag}, {called}");
            }
        }
    }
}
