use super::instruction::Instruction;

/// The part of a pattern expression whose branches and packed channels can be planned
/// without moving a draw or assignment across an opaque call.
pub(super) fn pattern_planning_end(program: &super::Program) -> usize {
    use super::function_call::PatternAccess;
    let instructions = &program.instructions;
    // A closed sampler advances the same stamp stream as a direct draw. Its call site
    // participates in branch planning even when the surrounding expression assigns variables.
    if !instructions.iter().any(|op| op.definition.id == "set")
        || instructions.iter().filter_map(|op| op.call.as_ref()).all(|call| call.pattern_access != PatternAccess::Context)
    {
        return instructions.len();
    }
    let Some(first_call) = instructions.iter().position(|op| op.call.as_ref().is_some_and(|call| call.pattern_access != PatternAccess::Arguments)) else {
        return instructions.len();
    };
    let Some(root) = instructions.get(program.output) else { return 0 };
    let ("sequence", &[first, _]) = (root.definition.id, root.inputs.as_slice()) else { return 0 };
    // The compiler visits a Sequence's first input and its dependencies before its
    // second input. A call in the result cannot observe an intermediate prefix value.
    let end = first.saturating_add(1);
    if end <= first_call && instructions[end..].iter().all(|op| op.definition.id != "set" && op.call.as_ref().is_none_or(|call| !call.exports)) {
        end
    } else {
        0
    }
}

/// How to evaluate a Sequence's first input when its return value is discarded.
#[derive(Clone, Copy)]
pub(super) enum Visit {
    Skip,
    Value,
    Inputs,
    Branch,
}

pub(super) fn identify(instructions: &[Instruction], output: usize) -> Vec<Visit> {
    if !instructions.iter().any(|op| op.definition.id == "sequence") || !instructions.iter().any(|op| op.definition.id == "rand") {
        return Vec::new();
    }
    // A loop can read the same register at several points with different assignments and draws.
    // Keep its existing evaluation and invalidation until each iteration's reads are accounted for.
    if instructions.iter().any(|op| op.definition.id == "while") {
        return Vec::new();
    }

    let mut readers = vec![Vec::new(); instructions.len()];
    let mut needed = vec![false; instructions.len()];
    let mut effects = vec![false; instructions.len()];
    enum Mark {
        Value(usize),
        Effect(usize),
    }
    use Mark::*;
    let mut pending = vec![Value(output)];
    for (index, op) in instructions.iter().enumerate() {
        for &input in &op.inputs {
            readers[input].push(index);
        }
        // A call may read or export a variable without a wire to the caller.
        if op.definition.id == "set" || op.call.is_some() {
            pending.push(Value(index));
            pending.push(Effect(index));
        }
    }
    while let Some(mark) = pending.pop() {
        match mark {
            Value(i) if !needed[i] => {
                needed[i] = true;
                let op = &instructions[i];
                if op.definition.id == "rand" || op.definition.id.starts_with("get_") {
                    pending.push(Effect(i));
                }
                if let ("sequence", &[_, last]) = (op.definition.id, op.inputs.as_slice()) {
                    pending.push(Value(last));
                } else {
                    pending.extend(op.inputs.iter().copied().map(Value));
                }
            }
            Effect(i) if !effects[i] => {
                effects[i] = true;
                for &reader in &readers[i] {
                    pending.push(Effect(reader));
                    let op = &instructions[reader];
                    if let ("ifelse", &[condition, yes, no]) = (op.definition.id, op.inputs.as_slice())
                        && (i == yes || i == no)
                    {
                        pending.push(Value(condition));
                    }
                }
            }
            _ => {}
        }
    }

    instructions
        .iter()
        .enumerate()
        .map(|(i, op)| {
            if !effects[i] {
                Visit::Skip
            } else if needed[i] {
                Visit::Value
            } else if let ("ifelse", &[_, yes, no]) = (op.definition.id, op.inputs.as_slice())
                && (effects[yes] || effects[no])
            {
                Visit::Branch
            } else {
                Visit::Inputs
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "sequence_prefix.test.rs"]
mod tests;
