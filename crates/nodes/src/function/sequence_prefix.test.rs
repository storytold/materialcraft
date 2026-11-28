use crate::function::{EvaluationContext, EvaluationState, FunctionScope, compile};
use materialcraft_core::{FunctionGraph, Value};
use std::collections::BTreeMap;

#[test]
fn value_processor_random_draws_use_the_scalar_evaluation_position() {
    let mut graph = FunctionGraph::default();
    let one = graph.add_constant(Value::Float(1.0));
    let first = graph.add("rand", &[one], None);
    let second = graph.add("rand", &[one], None);
    graph.output = Some(graph.add("vector2", &[first, second], None));
    let variables = BTreeMap::new();
    for size in [[16.0; 2], [128.0, 32.0]] {
        let context = EvaluationContext {
            scope: FunctionScope::ValueProcessor,
            position: [0.0; 2],
            size,
            images: &[],
            image_formats: &[],
            tile: true,
            variables: &variables,
        };
        for (seed, pair) in [(3, [280133, 3213511]), (20, [233407, 5314947])] {
            let expected = Value::Float2(pair.map(|level| level as f32 / 8388608.0));
            assert_eq!(compile(&graph, seed).unwrap().evaluate(&context, &mut EvaluationState::default()).unwrap(), expected, "seed {seed}");
        }
    }
}

#[test]
fn discarded_sequence_values_preserve_assignments_and_shared_random_draws() {
    let mut graph = FunctionGraph::default();
    let one = graph.add_constant(Value::Float(1.0));
    let zero = graph.add_constant(Value::Float(0.0));
    let choice = graph.add("get_bool", &[], Some(Value::Text("choice".into())));
    let first = graph.add("rand", &[one], None);
    let unused = graph.add("rand", &[one], None);
    let last = graph.add("rand", &[one], None);
    let write = graph.add("set", &[first], Some(Value::Text("tone".into())));
    let read = graph.add("get_float1", &[], Some(Value::Text("tone".into())));
    let observe = graph.add("mul", &[read, zero], None);
    let following = graph.add("add", &[observe, last], None);
    let sum = graph.add("add", &[write, unused], None);
    let branch = graph.add("ifelse", &[choice, write, unused], None);
    let shared_zero = graph.add("mul", &[first, zero], None);
    let shared = graph.add("add", &[last, shared_zero], None);
    let pure = graph.add("sequence", &[unused, last], None);
    let assigned = graph.add("sequence", &[sum, following], None);
    let conditional = graph.add("sequence", &[branch, following], None);
    let reused = graph.add("sequence", &[first, shared], None);
    let vertices = [pure, assigned, conditional, reused];
    let control = graph.add("vector2", &[first, last], None);

    for scope in [FunctionScope::Node, FunctionScope::FxMap, FunctionScope::PixelProcessor, FunctionScope::ValueProcessor] {
        for choice in [true, false] {
            let variables = BTreeMap::from([("choice".into(), Value::Bool(choice)), ("tone".into(), Value::Float(0.25))]);
            let context = EvaluationContext { scope, position: [0.0; 2], size: [16.0; 2], images: &[], image_formats: &[], tile: true, variables: &variables };
            for seed in [0, 17] {
                // An ordinary vector observes the first two draws from this particular scope.
                graph.output = Some(control);
                let reference = compile(&graph, seed).unwrap().evaluate(&context, &mut EvaluationState::default()).unwrap();
                let Value::Float2([a, b]) = reference else { panic!("two draws") };
                for (root, value) in vertices.into_iter().zip([a, b, if choice { b } else { a }, b]) {
                    graph.output = Some(root);
                    let mut state = EvaluationState::default();
                    let result = compile(&graph, seed).unwrap().evaluate(&context, &mut state).unwrap();
                    assert_eq!(result, Value::Float(value), "scope {scope:?}, root {root}, choice {choice}");
                    if root == assigned || (root == conditional && choice) {
                        assert_eq!(state.assignments().get("tone"), Some(&Value::Float(a)));
                    }
                }
            }
        }
    }
}
