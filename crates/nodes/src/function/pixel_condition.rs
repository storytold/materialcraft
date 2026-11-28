use super::{EvaluationContext, Program, SystemVariable, operation};
use crate::random::FunctionRandom;
use materialcraft_core::Value;
use std::collections::BTreeMap;

/// Count reads for arithmetic contraction without changing the evaluation of the condition.
pub(super) fn live_readers(program: &Program, context: &EvaluationContext<'_>) -> Option<Vec<bool>> {
    let instructions = &program.instructions;
    if !instructions.iter().any(|op| matches!(op.definition.id, "and" | "or"))
        || instructions.iter().any(|op| op.call.is_some() || matches!(op.definition.id, "set" | "rand" | "while" | "sequence"))
    {
        return None;
    }

    let mut known: Vec<Option<Value>> = Vec::with_capacity(instructions.len());
    let mut fixed = vec![false; instructions.len()];
    let mut variables = BTreeMap::new();
    let mut random = FunctionRandom::uniform(0);
    for (index, op) in instructions.iter().enumerate() {
        let id = op.definition.id;
        let decisive = match id {
            "and" => Some(false),
            "or" => Some(true),
            _ => None,
        };
        let constant = if let Some(value) = decisive
            && op.inputs.iter().any(|&input| known.get(input) == Some(&Some(Value::Bool(value))))
        {
            fixed[index] = true;
            Some(Value::Bool(value))
        } else if matches!(id, "const_bool" | "const_int1") {
            op.value.clone()
        } else if id == "get_integer1" && matches!(&op.value, Some(Value::Text(name)) if name == "$tiling") {
            SystemVariable::Tiling.resolve(context).ok().flatten()
        } else if matches!(op.type_code, 4 | 16) && op.depends_only_on_operands() {
            let inputs: Option<Vec<_>> = op.inputs.iter().map(|&input| known.get(input)?.clone()).collect();
            inputs.and_then(|args| operation::evaluate(op, &args, &mut variables, &mut random, context, None).ok())
        } else {
            None
        };
        known.push(constant);
    }
    if !fixed.contains(&true) {
        return None;
    }

    // A product shared with a live comparison still rounds before an addition.
    // A condition fixed by either operand cannot observe that product's precision.
    let mut live = vec![false; instructions.len()];
    *live.get_mut(program.output)? = true;
    for (index, op) in instructions.iter().enumerate().rev() {
        if live[index] && !fixed[index] {
            for &input in &op.inputs {
                *live.get_mut(input)? = true;
            }
        }
    }
    Some(live)
}
