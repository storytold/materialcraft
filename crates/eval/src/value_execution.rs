use craft_texture::Image;
use materialcraft_core::{OutputFormat, Value};
use std::collections::BTreeMap;

pub(crate) fn evaluate_value(
    parameters: &BTreeMap<String, Value>,
    variables: &dyn materialcraft_nodes::GraphVariables,
    seed: u32,
    size: [u32; 2],
    images: &[Option<Image>],
    image_formats: &[OutputFormat],
    tile: bool,
) -> Result<Value, String> {
    use materialcraft_nodes::function::{EvaluationContext, EvaluationState, FunctionScope, compile_with_literal_inputs};

    let Some(Value::Function(graph)) = parameters.get("function") else {
        return Err("missing value function".into());
    };

    let program = compile_with_literal_inputs(graph, seed, variables.literal_inputs())?;
    let context = EvaluationContext {
        image_formats,
        scope: FunctionScope::ValueProcessor,
        position: [0.0; 2],
        size: size.map(|side| side as f32),
        images,
        tile,
        variables: variables.graph_values(),
    };

    let value = program.evaluate(&context, &mut EvaluationState::default())?;
    Ok(match value {
        Value::Int(v) => Value::Int(stored_integer(v)),
        Value::Int2(v) => Value::Int2(v.map(stored_integer)),
        Value::Int3(v) => Value::Int3(v.map(stored_integer)),
        Value::Int4(v) => Value::Int4(v.map(stored_integer)),
        other => other,
    })
}

fn stored_integer(value: i32) -> i32 {
    // The value port rounds to binary32 first. The positive endpoint can round up to 2^31.
    i32::try_from(value as f32 as i64).unwrap_or(i32::MIN)
}
