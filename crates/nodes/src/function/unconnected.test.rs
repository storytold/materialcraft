use super::{EvaluationContext, EvaluationState, compile, definition};
use materialcraft_core::{FunctionGraph, Value};

#[test]
fn unconnected_typed_operations_use_their_operand_types() {
    for (operation, expected) in [
        ("vector3", Value::Float3([0.0; 3])),
        ("vector4", Value::Float4([0.0; 4])),
        ("ivector2", Value::Int2([0; 2])),
        ("ivector3", Value::Int3([0; 3])),
        ("ivector4", Value::Int4([0; 4])),
        ("iswizzle1", Value::Int(0)),
        ("iswizzle4", Value::Int4([0; 4])),
        ("tofloat", Value::Float(0.0)),
        ("tofloat2", Value::Float2([0.0; 2])),
        ("tofloat3", Value::Float3([0.0; 3])),
        ("tofloat4", Value::Float4([0.0; 4])),
        ("toint2", Value::Int2([0; 2])),
        ("toint3", Value::Int3([0; 3])),
        ("toint4", Value::Int4([0; 4])),
        ("and", Value::Bool(false)),
        ("or", Value::Bool(false)),
        ("not", Value::Bool(true)),
        ("atan2", Value::Float(0.0)),
        ("samplelum", Value::Float(0.0)),
        ("samplecol", Value::Float4([0.0; 4])),
    ] {
        let mut graph = FunctionGraph::default();
        let id = graph.add(operation, &[], None);
        graph.nodes.first_mut().unwrap().inputs.resize(definition(operation).unwrap().inputs.len(), None);
        graph.output = Some(id);
        let variables = Default::default();
        let context = EvaluationContext {
            image_formats: &[],
            scope: Default::default(),
            position: [0.5; 2],
            size: [16.0; 2],
            images: &[],
            tile: true,
            variables: &variables,
        };

        let value = compile(&graph, 0).unwrap().evaluate(&context, &mut EvaluationState::default());

        assert_eq!(value, Ok(expected), "{operation}");
    }
}

#[test]
fn vector_constructors_place_a_single_connected_operand_in_the_leading_lanes() {
    for (operation, input, expected) in [
        ("vector2", Value::Float(-0.375), Value::Float2([-0.375, 0.0])),
        ("vector3", Value::Float2([0.25, 0.75]), Value::Float3([0.25, 0.75, 0.0])),
        ("vector4", Value::Float2([0.25, 0.75]), Value::Float4([0.25, 0.75, 0.0, 0.0])),
        ("vector4", Value::Float3([0.375, -0.6875, 0.25]), Value::Float4([0.375, -0.6875, 0.25, 0.0])),
        ("ivector2", Value::Int(-23), Value::Int2([-23, 0])),
        ("ivector3", Value::Int(17), Value::Int3([17, 0, 0])),
        ("ivector4", Value::Int(7), Value::Int4([7, 0, 0, 0])),
        ("ivector4", Value::Int2([17, -23]), Value::Int4([17, -23, 0, 0])),
    ] {
        let mut graph = FunctionGraph::default();
        let source = graph.add_constant(input);
        graph.output = Some(graph.add(operation, &[], None));
        let variables = Default::default();
        let context = EvaluationContext {
            image_formats: &[],
            scope: Default::default(),
            position: [0.5; 2],
            size: [16.0; 2],
            images: &[],
            tile: true,
            variables: &variables,
        };

        for inputs in [[None, Some(source)], [Some(source), None]] {
            graph.nodes.last_mut().unwrap().inputs = inputs.to_vec();
            let value = compile(&graph, 0).unwrap().evaluate(&context, &mut EvaluationState::default()).unwrap();
            assert_eq!(value, expected, "{operation} {inputs:?}");
        }
    }
}

#[test]
fn partially_connected_dot_uses_the_connected_vector_width() {
    for vector in [Value::Float3([1.0, 2.0, 3.0]), Value::Float4([1.0, 2.0, 3.0, 4.0])] {
        for connected_slot in 0..2 {
            let mut graph = FunctionGraph::default();
            let source = graph.add_constant(vector.clone());
            let dot = graph.add("dot", &[], None);
            graph.nodes.last_mut().unwrap().inputs = if connected_slot == 0 { vec![Some(source), None] } else { vec![None, Some(source)] };
            graph.output = Some(dot);
            let variables = Default::default();
            let context = EvaluationContext {
                image_formats: &[],
                scope: Default::default(),
                position: [0.5; 2],
                size: [16.0; 2],
                images: &[],
                tile: true,
                variables: &variables,
            };

            assert_eq!(compile(&graph, 0).unwrap().evaluate(&context, &mut EvaluationState::default()), Ok(Value::Float(0.0)));
        }
    }
}
