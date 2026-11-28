use materialcraft_core::{BaseParameter, Value, ValueType};
use materialcraft_nodes::function::{EvaluationContext, EvaluationState, FunctionScope, Program};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(crate) enum ParameterMode {
    Opaque,
    RandomSeed,
    Broadcast,
    UniformColor,
    GrayPatternOpacity,
    Levels { normalized: bool },
}

impl ParameterMode {
    pub(crate) fn for_base(parameter: BaseParameter) -> Self {
        if parameter == BaseParameter::RandomSeed { Self::RandomSeed } else { Self::Opaque }
    }
}

pub(crate) fn evaluate(
    program: &Program,
    expected: ValueType,
    variables: &mut BTreeMap<String, Value>,
    size: [u32; 2],
    mode: ParameterMode,
    fallback: Option<&Value>,
    assigned_integers: Option<&mut Vec<String>>,
) -> Result<(Value, bool), String> {
    let widened = expected == ValueType::Float4 && [ValueType::Float as u32, ValueType::Float3 as u32].contains(&program.output_type());
    if program.output_type() != expected as u32 && !widened {
        return Err(format!("output must be {}", expected.label()));
    }
    // A direct link with a missing user input leaves the property's stored setting in effect.
    if let Some(value) = fallback
        && program.input_identifier().is_some_and(|name| !name.starts_with('$') && !variables.contains_key(name))
    {
        return Ok((value.clone(), matches!(mode, ParameterMode::UniformColor)));
    }

    let context = EvaluationContext {
        image_formats: &[],
        scope: if matches!(mode, ParameterMode::RandomSeed) { FunctionScope::RandomSeed } else { FunctionScope::Node },
        position: [0.5; 2],
        size: size.map(|side| side as f32),
        images: &[],
        tile: true,
        variables,
    };
    let mut state = EvaluationState::default();
    let value = match mode {
        ParameterMode::Levels { normalized: true } => program.evaluate_normalized_color(&context, &mut state)?,
        ParameterMode::GrayPatternOpacity => Value::Float(program.evaluate_pattern_opacity(&context, &mut state)?),
        _ => program.evaluate(&context, &mut state)?,
    };
    // A scalar level is already luminance. Its RGB expansion would introduce a second rounding.
    let scalar_level = matches!(mode, ParameterMode::Levels { .. }) && program.output_type() == ValueType::Float as u32;
    let value = match value {
        value if matches!(mode, ParameterMode::UniformColor) => value,
        Value::Float(level) | Value::Float4([level, _, _, _]) if scalar_level => Value::Float(level),
        Value::Float(level) if widened && matches!(mode, ParameterMode::Broadcast) => Value::Float4([level; 4]),
        Value::Float(level) if widened => Value::Float4([level, level, level, 1.0]),
        Value::Float3([red, green, blue]) if widened => Value::Float4([red, green, blue, 1.0]),
        value => value,
    };

    let fixed = match mode {
        ParameterMode::UniformColor => program.uses_fixed_uniform_color(variables),
        _ => program.output_is_literal(),
    };
    if let Some(names) = assigned_integers {
        for (name, value) in state.assignments() {
            if matches!(value, Value::Int(_)) && !names.contains(name) {
                names.push(name.clone());
            }
        }
    }
    variables.extend(state.assignments().clone());
    Ok((value, fixed))
}
