use super::{Program, function_call::PatternAccess, instruction::Instruction, vector::PatternComponent};
use materialcraft_core::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

pub(super) struct BranchInputs {
    shared: Vec<usize>,
    true_count: usize,
}

impl BranchInputs {
    // The true arm occurs first. A skipped arm still evaluates effects shared with another consumer.
    pub(super) fn skipped(&self, take_true: bool) -> &[usize] {
        let (through_true, through_false_only) = self.shared.split_at(self.true_count);
        if take_true { through_false_only } else { through_true }
    }
}

/// Schedule shared writes and draws from a skipped arm. Private expressions stay lazy.
pub(super) fn identify(program: &mut Program, pattern_tail: Range<usize>) {
    let instructions = &program.instructions;
    if !instructions.iter().any(|op| op.definition.id == "ifelse")
        || !instructions.iter().any(|op| matches!(op.definition.id, "rand" | "set"))
        || instructions.iter().any(|op| op.definition.id == "while" || op.call.as_ref().is_some_and(|call| call.exports))
    {
        return;
    }

    let written: BTreeSet<_> = instructions
        .iter()
        .filter_map(|op| match (&op.value, op.definition.id) {
            (Some(Value::Text(name)), "set") => Some(name.as_str()),
            _ => None,
        })
        .collect();
    // A range depending on an assignment or image sample must stay at its original read.
    let mut independent_range = Vec::with_capacity(instructions.len());
    for op in instructions {
        let fixed_input = op.definition.id == "get_float1" && matches!(&op.value, Some(Value::Text(name)) if !written.contains(name.as_str()));
        independent_range
            .push(op.type_code == 256 && (op.literal || fixed_input || (op.definition.id == "rand" && op.inputs.iter().all(|&i| independent_range[i]))));
    }
    // A called graph can observe the caller's assignments without listing them among its inputs.
    let has_calls_and_writes = !written.is_empty() && instructions.iter().any(|op| op.call.is_some());
    if !has_calls_and_writes {
        program.processor_branches = shared_inputs(program, &independent_range, None, &[], 0..0);
    }
    if pattern_tail.start > 0 {
        let draws = pattern_draws(instructions, &written, &independent_range);
        program.pattern_branches = shared_inputs(program, &independent_range, Some(&program.pattern_components), &draws, pattern_tail);
    }
}

struct PatternDraw {
    direct: bool,
    in_expression: bool,
}

fn pattern_draws(instructions: &[Instruction], written: &BTreeSet<&str>, ranges: &[bool]) -> Vec<PatternDraw> {
    let mut fixed = Vec::with_capacity(instructions.len());
    let mut draws: Vec<PatternDraw> = Vec::with_capacity(instructions.len());
    for (i, op) in instructions.iter().enumerate() {
        let id = op.definition.id;
        let read = id.starts_with("get_") && matches!(&op.value, Some(Value::Text(name)) if !written.contains(name.as_str()));
        let access = op.call.as_ref().map(|call| call.pattern_access);
        let pure = op.depends_only_on_operands() || access == Some(PatternAccess::Arguments);
        fixed.push(op.literal || read || (pure && op.inputs.iter().all(|&input| fixed[input])));
        // Bounds and call arguments can be computed from immutable parameters. A read
        // taken from a Set or an image must still occur at its original position.
        let direct = (id == "rand" && op.inputs.first().is_some_and(|&range| ranges[i] || fixed[range]))
            || (access == Some(PatternAccess::Random) && op.inputs.iter().all(|&arg| ranges[arg] || fixed[arg]));
        let in_expression = direct || op.inputs.iter().any(|&input| draws[input].in_expression);
        draws.push(PatternDraw { direct, in_expression });
    }
    draws
}

fn operands<'a>(instructions: &'a [Instruction], index: usize, components: Option<&'a BTreeMap<usize, PatternComponent>>) -> &'a [usize] {
    let node = &instructions[index];
    match components.and_then(|channels| channels.get(&index)) {
        Some(PatternComponent::Register(source)) => std::slice::from_ref(source),
        Some(PatternComponent::Zero) => &[],
        None => node
            .call
            .as_ref()
            .and_then(|call| call.pattern_arguments.as_ref())
            .filter(|plan| components.is_some() && !matches!(plan.target, super::function_call::PatternTarget::Arguments(_)))
            .map_or(&node.inputs, |plan| &plan.registers),
    }
}

fn shared_inputs(
    program: &Program,
    ranges: &[bool],
    components: Option<&BTreeMap<usize, PatternComponent>>,
    draws: &[PatternDraw],
    excluded: Range<usize>,
) -> BTreeMap<usize, BranchInputs> {
    let instructions = &program.instructions;
    let mut paths = vec![0_u8; instructions.len()];
    let mut stack = Vec::new();
    let mut effects = Vec::new();
    let mut plans = BTreeMap::new();
    for (branch, op) in instructions.iter().enumerate().filter(|(i, op)| op.definition.id == "ifelse" && !excluded.contains(i)) {
        let &[condition, yes, no] = op.inputs.as_slice() else { continue };
        // A branch folded before a stamp runs cannot schedule draws from its discarded arm.
        if components.is_some() && instructions[condition].pattern_literal {
            continue;
        }
        paths.fill(0);
        effects.clear();
        // Record the order of first use through each arm.
        for (root, path) in [(yes, 1), (no, 2), (program.output, 4)] {
            if path == 4 && effects.is_empty() {
                break;
            }
            stack.push(root);
            while let Some(index) = stack.pop() {
                if index == branch || paths[index] & path != 0 {
                    continue;
                }
                let node = &instructions[index];
                let shared_write = op.control_registers.binary_search(&index).is_ok();
                let draw = draws.get(index).map_or_else(|| node.definition.id == "rand" && ranges[index] && !node.inputs.is_empty(), |draw| draw.direct);
                if paths[index] == 0 && path != 4 && (shared_write || draw) {
                    effects.push(index);
                }
                paths[index] |= path;
                // Evaluating a shared expression performs any conditional writes within it in their own scope.
                if shared_write {
                    continue;
                }
                stack.extend(operands(instructions, index, components).iter().rev().copied());
            }
        }
        let shared: Vec<_> = if draws.is_empty() {
            effects.iter().copied().filter(|&i| paths[i].count_ones() > 1).collect()
        } else {
            // Evaluate the outermost shared conditional once. Pulling its individual
            // draws into the parent branch would consume a draw from its inactive arm.
            const ORDERED: u8 = 8;
            let mut shared = Vec::new();
            stack.extend([no, yes]);
            while let Some(i) = stack.pop() {
                if i == branch || paths[i] & ORDERED != 0 {
                    continue;
                }
                let common = paths[i].count_ones() > 1;
                paths[i] |= ORDERED;
                let write = op.control_registers.binary_search(&i).is_ok();
                let conditional = instructions[i].definition.id == "ifelse" && draws[i].in_expression;
                if common && (write || draws[i].direct || conditional) {
                    shared.push(i);
                } else if !write {
                    stack.extend(operands(instructions, i, components).iter().rev().copied());
                }
            }
            shared
        };
        if !shared.is_empty() {
            let true_count = shared.iter().take_while(|&&i| paths[i] & 1 != 0).count();
            plans.insert(branch, BranchInputs { shared, true_count });
        }
    }
    plans
}

#[cfg(test)]
#[path = "branch_inputs.test.rs"]
mod tests;
