use super::{FunctionScope, Program, arithmetic, function_call::PatternTarget, instruction::Instruction, sequence_prefix::Visit, value_type, vector};
use crate::random::FunctionRandom;
use materialcraft_core::Value;
use std::collections::{BTreeMap, BTreeSet};

struct Step<'a> {
    instruction: &'a Instruction,
    inputs: Vec<usize>,
    output: usize,
}

/// Arithmetic in a drawing property whose operands are fixed for this node and
/// depth or come from its random stream. Shared registers draw only once.
pub(crate) struct PatternExpression<'a> {
    steps: Vec<Step<'a>>,
    slots: Vec<Value>,
    result: usize,
    assignment: Option<usize>,
    ordinary_steps: usize,
}

struct Builder<'a, 'v> {
    variables: &'v BTreeMap<String, Value>,
    assigned: &'v BTreeSet<&'v str>,
    steps: Vec<Step<'a>>,
    slots: Vec<Value>,
    fixed: Vec<bool>,
    visits: usize,
    ordinary_steps: usize,
}

fn apply(instruction: &Instruction, inputs: &[Value], random: &mut FunctionRandom) -> Result<Value, String> {
    if instruction.definition.id == "vector2" {
        vector::evaluate(instruction.definition.id, inputs, instruction.value.as_ref())
    } else {
        arithmetic::evaluate(instruction, inputs, random, FunctionScope::FxMap)
    }
}

impl<'a> PatternExpression<'a> {
    /// The optional tint is the common scalar register and the selected branch.
    /// Its Sequence publishes a vector before evaluating that scalar.
    pub(crate) fn bind(program: &'a Program, variables: &BTreeMap<String, Value>, assigned: &BTreeSet<&str>, tint: Option<(usize, bool)>) -> Option<Self> {
        // Large and control-dependent expressions stay with the bounded stack evaluator.
        if program.instructions.len() > 96 {
            return None;
        }
        let mut builder = Builder { variables, assigned, steps: Vec::new(), slots: Vec::new(), fixed: Vec::new(), visits: 0, ordinary_steps: 0 };
        let mut registers = vec![None; program.instructions.len()];
        let arguments = BTreeMap::new();
        let (result, assignment) = if let Some((base, scaled)) = tint {
            let &[prefix, branch] = program.instructions.get(program.output)?.inputs.as_slice() else { return None };
            let set = program.instructions.get(prefix)?;
            let &[written] = set.inputs.as_slice() else { return None };
            if !matches!(&set.value, Some(Value::Text(name)) if !name.is_empty() && !name.starts_with('$'))
                || !matches!(program.sequence_prefixes.get(prefix), None | Some(Visit::Value))
            {
                return None;
            }
            let offset = builder.bind(program, written, &mut registers, &arguments, 0)?;
            let shared = program.pattern_branches.get(&branch).map_or(&[][..], |plan| plan.skipped(scaled));
            // The VM visits shared inputs of the unselected branch before a false
            // result and after a true result. They can advance the random stream.
            if !scaled {
                for &input in shared {
                    builder.bind(program, input, &mut registers, &arguments, 0)?;
                }
            }
            let value = builder.bind(program, base, &mut registers, &arguments, 0)?;
            if scaled {
                for &input in shared {
                    builder.bind(program, input, &mut registers, &arguments, 0)?;
                }
            }
            if !matches!(builder.slots.get(offset), Some(Value::Float2(_))) || !matches!(builder.slots.get(value), Some(Value::Float(_))) {
                return None;
            }
            (value, Some(offset))
        } else {
            (builder.bind(program, program.output, &mut registers, &arguments, 0)?, None)
        };
        (value_type::type_code(builder.slots.get(result)?) == program.output_type()).then_some(Self {
            steps: builder.steps,
            slots: builder.slots,
            result,
            assignment,
            ordinary_steps: builder.ordinary_steps,
        })
    }

    /// Each ordinary register visit enters the VM. A new operation also
    /// applies and stores its result. The bound program runs just its steps.
    pub(crate) fn work(&self) -> (usize, usize) {
        (self.steps.len() + 1, self.ordinary_steps)
    }

    pub(crate) fn evaluate(&mut self, random: &mut FunctionRandom) -> Result<(Value, Option<Value>), String> {
        let missing = "invalid bound pattern register";
        let mut arguments = [Value::Float(0.0), Value::Float(0.0), Value::Float(0.0)];
        for step in &self.steps {
            let inputs = arguments.get_mut(..step.inputs.len()).ok_or(missing)?;
            for (argument, &slot) in inputs.iter_mut().zip(&step.inputs) {
                *argument = self.slots.get(slot).ok_or(missing)?.clone();
            }
            let value = apply(step.instruction, inputs, random)?;
            *self.slots.get_mut(step.output).ok_or(missing)? = value;
        }
        let assignment = self.assignment.map(|slot| self.slots.get(slot).cloned().ok_or(missing)).transpose()?;
        Ok((self.slots.get(self.result).ok_or(missing)?.clone(), assignment))
    }
}

impl<'a> Builder<'a, '_> {
    fn fixed(&mut self, value: &Value) -> Option<usize> {
        match value {
            Value::Float(v) => self.push(Value::Float(*v), true),
            Value::Float2(v) => self.push(Value::Float2(*v), true),
            _ => None,
        }
    }

    fn push(&mut self, value: Value, fixed: bool) -> Option<usize> {
        if self.slots.len() >= 256 || !matches!(value, Value::Float(_) | Value::Float2(_)) {
            return None;
        }
        let slot = self.slots.len();
        self.slots.push(value);
        self.fixed.push(fixed);
        Some(slot)
    }

    fn bind(
        &mut self,
        program: &'a Program,
        index: usize,
        registers: &mut [Option<usize>],
        arguments: &BTreeMap<&'a str, usize>,
        depth: usize,
    ) -> Option<usize> {
        if self.visits >= 512 || depth > 64 {
            return None;
        }
        self.visits += 1;
        self.ordinary_steps += 1;
        if let Some(slot) = *registers.get(index)? {
            return Some(slot);
        }
        let op = program.instructions.get(index)?;
        self.ordinary_steps += 1; // Store the result of a newly entered register.
        let slot = if let Some(component) = program.pattern_components.get(&index) {
            match component {
                vector::PatternComponent::Register(input) => self.bind(program, *input, registers, arguments, depth + 1)?,
                vector::PatternComponent::Zero => self.push(Value::Float(0.0), true)?,
            }
        } else if let Some(call) = &op.call {
            if call.exports || call.program.instructions.len() > 64 || call.defaults.len() > 64 {
                return None;
            }
            self.ordinary_steps += 1;
            // Nested calls inherit outer formals, then shadow them with their own defaults and wires.
            let mut bindings = arguments.clone();
            for (name, value) in &call.defaults {
                bindings.insert(name, self.fixed(value)?);
            }
            if let Some(plan) = &call.pattern_arguments {
                let PatternTarget::Arguments(positions) = &plan.target else { return None };
                if positions.len() != plan.registers.len() {
                    return None;
                }
                for (&position, &input) in positions.iter().zip(&plan.registers) {
                    let value = self.bind(program, input, registers, arguments, depth + 1)?;
                    bindings.insert(call.inputs.get(position)?, value);
                }
            } else {
                if call.inputs.len() != op.inputs.len() {
                    return None;
                }
                for (name, &input) in call.inputs.iter().zip(&op.inputs) {
                    let value = self.bind(program, input, registers, arguments, depth + 1)?;
                    bindings.insert(name, value);
                }
            }
            let mut child = vec![None; call.program.instructions.len()];
            self.bind(&call.program, call.program.output, &mut child, &bindings, depth + 1)?
        } else {
            self.ordinary_steps += 1;
            match op.definition.id {
                "const_float1" | "const_float2" => self.fixed(op.value.as_ref()?)?,
                "get_float1" | "get_float2" => {
                    let Value::Text(name) = op.value.as_ref()? else { return None };
                    if name.starts_with('$') {
                        if !matches!(name.as_str(), "$depth" | "$depthpow2") {
                            return None;
                        }
                        self.fixed(self.variables.get(name)?)?
                    } else if let Some(&argument) = arguments.get(name.as_str()) {
                        argument
                    } else {
                        if self.assigned.contains(name.as_str()) {
                            return None;
                        }
                        self.fixed(self.variables.get(name)?)?
                    }
                }
                "add" | "sub" | "mul" | "mulscalar" | "div" | "mod" | "dot" | "rand" | "neg" | "sqrt" | "sin" | "cos" | "vector2" | "cartesian" | "min"
                | "max" | "lerp" => {
                    if op.inputs.len() > 3 {
                        return None;
                    }
                    let mut inputs = Vec::with_capacity(op.inputs.len());
                    let mut values = Vec::with_capacity(op.inputs.len());
                    let mut fixed = op.definition.id != "rand" || op.inputs.len() == 2;
                    for &child in &op.inputs {
                        let slot = self.bind(program, child, registers, arguments, depth + 1)?;
                        inputs.push(slot);
                        fixed &= *self.fixed.get(slot)?;
                        values.push(self.slots.get(slot)?.clone());
                    }
                    // A private stream checks the shape of a varying result without consuming a draw.
                    let output = self.push(apply(op, &values, &mut FunctionRandom::default()).ok()?, fixed)?;
                    if !fixed {
                        self.steps.push(Step { instruction: op, inputs, output });
                    }
                    output
                }
                _ => return None,
            }
        };
        if value_type::type_code(self.slots.get(slot)?) != op.type_code {
            return None;
        }
        *registers.get_mut(index)? = Some(slot);
        Some(slot)
    }
}
