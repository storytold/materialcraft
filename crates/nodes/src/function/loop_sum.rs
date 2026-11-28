use super::{EvaluationState, Program, flush_subnormal, instruction::Instruction};
use materialcraft_core::Value;

/// A scalar sum returned by the loop body. Only its final addition may read the accumulated value.
pub(super) struct LoopSum {
    name: String,
    term: usize,
}

impl LoopSum {
    pub(super) fn identify(program: &Program, instruction: &Instruction) -> Option<Self> {
        if instruction.definition.id != "while" {
            return None;
        }
        let &[_, condition, body] = instruction.inputs.as_slice() else { return None };
        let ops = &program.instructions;
        let result = ops.get(body)?;
        let write = if result.definition.id == "sequence" { *result.inputs.get(1)? } else { body };
        let assignment = ops.get(write)?;
        let Some(Value::Text(name)) = &assignment.value else { return None };
        if assignment.definition.id != "set" || name.starts_with('$') || name.is_empty() {
            return None;
        }
        let &[add] = assignment.inputs.as_slice() else { return None };
        let addition = ops.get(add)?;
        if addition.definition.id != "add" || addition.type_code != 256 || addition.literal || addition.pixel_product_sum.is_some() {
            return None;
        }
        let &[a, b] = addition.inputs.as_slice() else { return None };
        let reads_sum = |index| ops.get(index).is_some_and(|op: &Instruction| op.definition.id == "get_float1" && op.value == assignment.value);
        let (read, term) = match (reads_sum(a), reads_sum(b)) {
            (true, false) => (a, b),
            (false, true) => (b, a),
            _ => return None,
        };
        if [read, add, write, body].contains(&condition) || ops.get(term)?.type_code != 256 {
            return None;
        }
        for &index in &instruction.control_registers {
            let op = ops.get(index)?;
            // A shared read, a second write or a callee that sees the sum can change a later term or the exit.
            if (index != read && index != write && refers_to(op, name))
                || op.call.as_ref().is_some_and(|call| call_uses(&call.program, name))
                || op
                    .inputs
                    .iter()
                    .any(|&input| (input == read && index != add) || (input == add && index != write) || (input == write && index != body) || input == body)
            {
                return None;
            }
        }
        Some(Self { name: name.clone(), term })
    }

    pub(super) fn start(&self, state: &EvaluationState) -> Option<SumCycle> {
        let Value::Float(value) = state.variables.get(&self.name)? else { return None };
        let Value::Float(written) = state.assignments.get(&self.name)? else { return None };
        (value.is_finite() && value.to_bits() == written.to_bits()).then(|| SumCycle {
            name: self.name.clone(),
            term: self.term,
            previous: *value,
            increments: Vec::new(),
        })
    }
}

fn refers_to(op: &Instruction, name: &str) -> bool {
    (op.definition.id.starts_with("get_") || matches!(op.definition.id, "set" | "isdefined"))
        && matches!(&op.value, Some(Value::Text(variable)) if variable == name)
}

fn call_uses(program: &Program, name: &str) -> bool {
    // Calls have already passed the compiler's depth and expanded instruction limits.
    program.instructions.iter().any(|op| refers_to(op, name) || op.call.as_ref().is_some_and(|call| call_uses(&call.program, name)))
}

pub(super) struct SumCycle {
    pub(super) name: String,
    term: usize,
    previous: f32,
    increments: Vec<f32>,
}

impl SumCycle {
    pub(super) fn record(&mut self, state: &EvaluationState, last: &Value) -> bool {
        let (Value::Float(last), Some(term)) = (last, state.scalar_register(self.term)) else { return false };
        // Limit the speculative history. Longer cycles can continue through the ordinary evaluator.
        if self.increments.len() >= 64 || !term.is_finite() || !last.is_finite() {
            return false;
        }
        let term = flush_subnormal(term);
        if flush_subnormal(flush_subnormal(self.previous) + term).to_bits() != last.to_bits() {
            return false;
        }
        self.increments.push(term);
        self.previous = *last;
        true
    }

    pub(super) fn replay(&self, state: &mut EvaluationState, last: &mut Value, remaining: u32, budget: &mut usize) -> u32 {
        let Value::Float(value) = last else { return 0 };
        let (Some(Value::Float(visible)), Some(Value::Float(written))) = (state.variables.get_mut(&self.name), state.assignments.get_mut(&self.name)) else {
            return 0;
        };
        let period = self.increments.len();
        if period == 0 || [*value, *visible, *written].iter().any(|v| v.to_bits() != self.previous.to_bits()) {
            return 0;
        }
        let cycles = remaining as usize / period;
        let mut completed = 0;
        for _ in 0..cycles.min(*budget / period) {
            let before = value.to_bits();
            for &increment in &self.increments {
                *value = flush_subnormal(flush_subnormal(*value) + increment);
            }
            *budget -= period;
            completed += 1;
            if value.to_bits() == before {
                completed = cycles;
                break;
            }
        }
        *visible = *value;
        *written = *value;
        (completed * period) as u32
    }
}
