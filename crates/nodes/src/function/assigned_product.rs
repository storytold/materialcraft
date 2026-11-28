use super::{EvaluationState, Program, instruction::Instruction, product_sum::ProductSum, returned_product::float_width};
use materialcraft_core::Value;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
enum Registers {
    Local { left: usize, right: usize, assignment: Option<usize> },
    Argument(usize),
}

/// Factors from this frame or its caller, and the components also read as a rounded product.
#[derive(Clone, Copy)]
pub(super) struct Binding {
    registers: Registers,
    pub(super) rounded_components: u8,
}

impl Binding {
    pub(super) fn read(self, state: &EvaluationState) -> Option<[Value; 2]> {
        match self.registers {
            Registers::Argument(slot) => state.assigned_arguments.get(slot)?.clone(),
            Registers::Local { left, right, assignment } => {
                if let Some(index) = assignment {
                    state.registers.get(index)?.as_ref()?;
                }
                Some([state.registers.get(left)?.as_ref()?.clone(), state.registers.get(right)?.as_ref()?.clone()])
            }
        }
    }

    fn import(self, arguments: &mut Vec<Self>) -> Self {
        let slot = arguments.len();
        arguments.push(self);
        Self { registers: Registers::Argument(slot), ..self }
    }
}

fn variable(op: &Instruction) -> Option<&str> {
    match &op.value {
        Some(Value::Text(name)) if !name.starts_with('$') => Some(name),
        _ => None,
    }
}

fn float_getter(op: &Instruction) -> bool {
    matches!(op.definition.id, "get_float1" | "get_float2" | "get_float3" | "get_float4")
}

fn product(op: &Instruction) -> Option<(usize, usize)> {
    let &[a, b] = op.inputs.as_slice() else { return None };
    (!op.literal && float_width(op.type_code) > 0 && matches!(op.definition.id, "mul" | "mulscalar")).then_some((a, b))
}

fn unit(op: &Instruction, instructions: &[Instruction]) -> Option<usize> {
    let &[input, scalar] = op.inputs.as_slice() else { return None };
    (op.definition.id == "mulscalar" && instructions.get(scalar).is_some_and(|i| i.definition.id == "const_float1" && i.value == Some(Value::Float(1.0))))
        .then_some(input)
}

type ReadMasks = BTreeMap<String, u8>;

#[derive(Default)]
struct Uses {
    reads: ReadMasks,
    writes: BTreeMap<String, usize>,
    rounded: Vec<u8>,
    live: Vec<bool>,
    ordered: bool,
    loops: bool,
    calls: BTreeMap<usize, Uses>,
}

impl Uses {
    fn analyze(program: &Program, caller_reads: &dyn Fn(&str) -> u8) -> Self {
        let len = program.instructions.len();
        let mut uses = Self { rounded: vec![0; len], live: vec![false; len], ordered: true, ..Self::default() };
        if let (Some(mask), Some(live)) = (uses.rounded.get_mut(program.output), uses.live.get_mut(program.output)) {
            *mask = 15;
            *live = true;
        }
        for (index, op) in program.instructions.iter().enumerate().rev() {
            if !uses.live[index] {
                continue;
            }
            let observed = uses.rounded[index];
            let mut mark = |input: usize, mask: u8| {
                if let (Some(bits), Some(live)) = (uses.rounded.get_mut(input), uses.live.get_mut(input)) {
                    *bits |= mask;
                    *live = true;
                }
            };
            if let Some(call) = &op.call {
                let child = Self::analyze(&call.program, &|name| {
                    if call.defaults.contains_key(name) { 0 } else { caller_reads(name) | uses.reads.get(name).copied().unwrap_or(0) }
                });
                for (name, register) in call.inputs.iter().zip(&op.inputs) {
                    mark(*register, child.reads.get(name).copied().unwrap_or(0));
                }
                for (name, &mask) in &child.reads {
                    if !call.defaults.contains_key(name) {
                        *uses.reads.entry(name.clone()).or_default() |= mask;
                    }
                }
                for (name, count) in &child.writes {
                    if !call.defaults.contains_key(name) {
                        *uses.writes.entry(name.clone()).or_default() += count;
                    }
                }
                uses.calls.insert(index, child);
            } else if let Some(input) = unit(op, &program.instructions) {
                mark(input, observed);
            } else if let [input] = op.inputs.as_slice()
                && let Some(mask) = super::product_sum::selected_components(op)
            {
                mark(*input, mask);
            } else {
                match (op.definition.id, op.inputs.as_slice()) {
                    ("add" | "sub", &[left, right]) => {
                        mark(left, 0);
                        mark(right, 0);
                    }
                    ("sequence", &[first, last]) => {
                        mark(first, 0);
                        mark(last, observed);
                    }
                    ("passthrough", &[last]) => mark(last, observed),
                    ("ifelse", &[condition, yes, no]) => {
                        mark(condition, 15);
                        for &shared in &op.control_registers {
                            mark(shared, 0);
                        }
                        let test = program.instructions.get(condition);
                        if let Some(Instruction { value: Some(Value::Bool(value)), .. }) = test.filter(|op| op.definition.id == "const_bool") {
                            mark(if *value { yes } else { no }, observed);
                        } else {
                            uses.ordered = false;
                            mark(yes, observed);
                            mark(no, observed);
                        }
                    }
                    ("set", &[input]) => {
                        let source = program.instructions.get(input);
                        // Assigning a getter copies its value. Its rounded lanes are the ones read from either name.
                        let mask = if source.is_some_and(float_getter)
                            && let Some(name) = variable(op)
                        {
                            observed | caller_reads(name) | uses.reads.get(name).copied().unwrap_or(0)
                        } else if source.and_then(product).is_some() {
                            observed
                        } else {
                            15
                        };
                        mark(input, mask);
                        if let Some(name) = variable(op) {
                            *uses.writes.entry(name.to_owned()).or_default() += 1;
                        }
                    }
                    ("get_float1" | "get_float2" | "get_float3" | "get_float4", []) => {
                        if let Some(name) = variable(op) {
                            *uses.reads.entry(name.to_owned()).or_default() |= observed;
                        }
                    }
                    _ => {
                        if op.definition.id == "while" {
                            uses.ordered = false;
                            uses.loops = true;
                        }
                        for &input in &op.inputs {
                            mark(input, 15);
                        }
                    }
                }
            }
        }
        uses
    }
}

fn has_assignment(program: &Program) -> bool {
    program.instructions.iter().any(|op| {
        (op.definition.id == "set" && op.inputs.first().and_then(|&i| program.instructions.get(i)).and_then(product).is_some())
            || op.call.as_ref().is_some_and(|call| has_assignment(&call.program))
    })
}

pub(super) fn identify(program: &mut Program) {
    if program.instructions.iter().any(|op| op.call.is_some()) && has_assignment(program) {
        let uses = Uses::analyze(program, &|_| 0);
        install(program, &uses, &uses.reads, BTreeMap::new());
    }
}

fn install(program: &mut Program, uses: &Uses, observed: &ReadMasks, mut names: BTreeMap<String, Binding>) -> BTreeMap<String, Binding> {
    let instructions = &program.instructions;
    // A dynamic branch can skip a write. Only a unique, executed assignment is eligible in that case.
    if !uses.ordered {
        names.retain(|name, _| !uses.writes.contains_key(name));
    }
    let mut sources = vec![None; instructions.len()];
    let mut setters = BTreeMap::new();
    let mut products_written = BTreeMap::<usize, usize>::new();
    if !uses.loops {
        for (op, &live) in instructions.iter().zip(&uses.live) {
            if live && let ("set", &[input]) = (op.definition.id, op.inputs.as_slice()) {
                *products_written.entry(input).or_default() += 1;
            }
        }
        for (index, setter) in instructions.iter().enumerate() {
            if uses.live[index]
                && let ("set", &[input], Some(name)) = (setter.definition.id, setter.inputs.as_slice(), variable(setter))
                && (uses.ordered || uses.writes.get(name) == Some(&1))
                && products_written.get(&input) == Some(&1)
                && let Some(product_op) = instructions.get(input)
                && let Some((left, right)) = product(product_op)
            {
                let mask = (1_u8 << float_width(product_op.type_code)) - 1;
                let rounded_components = observed.get(name).copied().unwrap_or(0) | uses.reads.get(name).copied().unwrap_or(0) | uses.rounded[input];
                if mask & !rounded_components == 0 {
                    continue;
                }
                let binding = Binding { registers: Registers::Local { left, right, assignment: Some(index) }, rounded_components };
                setters.insert(index, binding);
                if !uses.ordered {
                    names.insert(name.to_owned(), binding);
                }
                sources[input] = Some(Binding { registers: Registers::Local { left, right, assignment: None }, ..binding });
            }
        }
    }
    for index in 0..program.instructions.len() {
        if !uses.live[index] {
            continue;
        }
        let (previous, current) = program.instructions.split_at_mut(index);
        let Some(op) = current.first_mut() else { continue };
        let source = |input: usize| sources.get(input).copied().flatten();
        if let Some(call) = &mut op.call {
            let Some(child) = uses.calls.get(&index) else { continue };
            let mut imported = BTreeMap::new();
            for (name, binding) in &names {
                if !call.defaults.contains_key(name) && child.reads.contains_key(name) && (child.ordered || !child.writes.contains_key(name)) {
                    imported.insert(name.clone(), binding.import(&mut call.assigned_products));
                }
            }
            for (name, register) in call.inputs.iter().zip(&op.inputs) {
                if child.reads.contains_key(name)
                    && let Some(binding) = source(*register)
                {
                    imported.insert(name.clone(), binding.import(&mut call.assigned_products));
                }
            }
            let returned = install(&mut call.program, child, observed, imported);
            if uses.ordered {
                names.retain(|name, _| !child.writes.contains_key(name) || call.defaults.contains_key(name));
                // A free name assigned a caller's step can carry the same factors into a later call.
                if child.ordered {
                    for (name, binding) in returned {
                        if child.writes.contains_key(&name)
                            && !call.defaults.contains_key(&name)
                            && let Registers::Argument(slot) = binding.registers
                            && let Some(&argument) = call.assigned_products.get(slot)
                        {
                            names.insert(name, argument);
                        }
                    }
                }
            }
            continue;
        }
        if uses.ordered
            && op.definition.id == "set"
            && let Some(name) = variable(op)
        {
            let copied = op.inputs.first().and_then(|&i| previous.get(i).filter(|op| float_getter(op)).and_then(|_| source(i)));
            if let Some(binding) = setters.get(&index).copied().or(copied) {
                names.insert(name.to_owned(), binding);
            } else {
                names.remove(name);
            }
        }
        let forwarded = match (op.definition.id, op.inputs.as_slice()) {
            ("get_float1" | "get_float2" | "get_float3" | "get_float4", []) => {
                variable(op).and_then(|name| names.get(name)).copied().filter(|binding| uses.ordered || matches!(binding.registers, Registers::Argument(_)))
            }
            ("passthrough", &[i]) => source(i),
            _ => unit(op, previous).and_then(source),
        };
        if let Some(binding) = forwarded {
            sources[index] = Some(binding);
        }
        if let ("add" | "sub", &[left, right]) = (op.definition.id, op.inputs.as_slice()) {
            let a = sources.get(left).copied().flatten();
            let b = sources.get(right).copied().flatten();
            let Some(binding) = a.xor(b) else { continue };
            let operand = if a.is_some() { left } else { right };
            let other = if a.is_some() { right } else { left };
            if op.pixel_product_sum.is_none()
                || previous.get(operand).and_then(|op| unit(op, previous)).is_some() && previous.get(other).and_then(product).is_none()
            {
                op.pixel_product_sum = Some(ProductSum::assigned(&op.inputs, b.is_some(), binding));
            }
        }
    }
    names
}
