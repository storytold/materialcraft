use super::{EvaluationState, Program, arithmetic::flush_subnormal, instruction::Instruction, product_sum::ProductSum, reciprocal::Factor, value_type};
use materialcraft_core::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Registers and components of the factors in the call that evaluated a product.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Source {
    calls: Vec<usize>,
    factors: [Factor; 2],
    components: [u8; 2],
    operation: Operation,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Operation {
    Multiply,
    Divide { literal_divisor: bool },
}

impl Source {
    pub(super) fn local(factors: [Factor; 2]) -> Self {
        Self { calls: Vec::new(), factors, components: [0, 0], operation: Operation::Multiply }
    }

    fn live_for_sum(&self, program: &Program, loops: &[usize], sum: usize) -> bool {
        if loops.is_empty() {
            return true;
        }
        if !self.calls.is_empty() {
            return false;
        }
        let mut inside = false;
        for &index in loops {
            let members = &program.instructions[index].control_registers;
            if members.binary_search(&sum).is_ok() {
                inside = true;
            } else if self.factors.iter().any(|factor| match factor {
                Factor::Register(i) => members.binary_search(i).is_ok(),
                _ => true,
            }) {
                // A separate loop can clear a factor while the packed vector still holds its old value.
                return false;
            }
        }
        inside
    }

    pub(super) fn factors(&self, mut state: &EvaluationState) -> Result<[f32; 2], String> {
        for call in &self.calls {
            state = state.calls.get(call).ok_or("missing evaluated product call")?;
        }
        let read = |factor: Factor, component| match factor {
            Factor::Register(i) => {
                let value = state.registers.get(i).and_then(Option::as_ref).ok_or("missing evaluated product factor")?;
                value_type::floats(value)?.get(usize::from(component)).copied().ok_or_else(|| "invalid product component".to_owned())
            }
            factor => factor.read(state).ok_or_else(|| "missing reciprocal factor".to_owned()),
        };
        Ok([read(self.factors[0], self.components[0])?, read(self.factors[1], self.components[1])?])
    }
}

pub(super) struct Term {
    source: Source,
    on_right: bool,
}

pub(super) type Sum = [Option<Term>; 4];

pub(super) fn evaluate_sum(terms: &Sum, arguments: &[Value], subtract: bool, state: &EvaluationState) -> Result<Value, String> {
    let (left, right, subtract, weight) = match arguments {
        [left, right] => (left, right, subtract, None),
        // Interpolation first forms the difference between its second and first endpoints.
        [first, second, Value::Float(weight)] => (second, first, true, Some(*weight)),
        _ => return Err("invalid product sum operands".into()),
    };
    let width = value_type::dimensions(left);
    if width != value_type::dimensions(right) {
        return Err("float operands must have the same dimensions".into());
    }
    let left = value_type::floats(left)?;
    let right = value_type::floats(right)?;
    let mut result = [0.0; 4];
    for (((output, term), left), right) in result.iter_mut().zip(terms).zip(left).zip(right).take(width) {
        if let Some(weight) = weight
            && term.is_none()
        {
            *output = (left - right).mul_add(weight, right);
            continue;
        }
        let (left, right) = (flush_subnormal(left), flush_subnormal(right));
        let sum = if let Some(term) = term {
            let [a, b] = term.source.factors(state)?.map(flush_subnormal);
            let a = if subtract && term.on_right { -a } else { a };
            let addend = if term.on_right {
                left
            } else if subtract {
                -right
            } else {
                right
            };
            match term.source.operation {
                Operation::Multiply => a.mul_add(b, addend),
                Operation::Divide { literal_divisor } => super::reciprocal::divide_add(a, b, addend, literal_divisor),
            }
        } else if subtract {
            left - right
        } else {
            left + right
        };
        let sum = flush_subnormal(sum);
        *output = weight.map_or(sum, |weight| flush_subnormal(sum.mul_add(weight, right)));
    }
    value_type::intermediate_float_value(result.get(..width).ok_or("invalid sum dimensions")?)
}

pub(super) type Lanes = [Option<Source>; 4];
type Origins = [Option<usize>; 4];

#[derive(Default)]
struct Reads {
    total: usize,
    sole_product_in_sum: usize,
}

pub(super) fn float_width(code: u32) -> usize {
    match code {
        256 => 1,
        512 => 2,
        1024 => 3,
        2048 => 4,
        _ => 0,
    }
}

/// Packing and component selection forward a lane without observing its rounded value.
pub(super) fn forwarded<T: Copy>(op: &Instruction, instructions: &[Instruction], origins: &[[Option<T>; 4]], padding: Option<T>) -> Option<[Option<T>; 4]> {
    let mut out = [None; 4];
    match (op.definition.id, op.inputs.as_slice()) {
        ("vector2" | "vector3" | "vector4", [left, right]) => {
            let left_width = float_width(instructions.get(*left)?.type_code);
            let right_width = float_width(instructions.get(*right)?.type_code);
            if left_width == 0 || right_width == 0 || left_width + right_width != float_width(op.type_code) {
                return None;
            }
            let lanes = op.inputs.iter().flat_map(|i| {
                let width = instructions.get(*i).map_or(0, |v| float_width(v.type_code));
                origins.get(*i).into_iter().flatten().take(width)
            });
            for (dst, src) in out.iter_mut().zip(lanes) {
                *dst = *src;
            }
        }
        ("swizzle1" | "swizzle2" | "swizzle3" | "swizzle4", [input]) => {
            let indices = value_type::integers(op.value.as_ref()?).ok()?;
            let width = float_width(instructions.get(*input)?.type_code);
            if width == 0 {
                return None;
            }
            let input = origins.get(*input)?;
            for (dst, i) in out.iter_mut().take(float_width(op.type_code)).zip(indices) {
                let i = usize::try_from(i).ok()?;
                let source = *input.get(i)?;
                *dst = if i < width { source } else { padding };
            }
        }
        _ => return None,
    }
    Some(out)
}

pub(super) fn identify(program: &mut Program, mut returned: BTreeMap<usize, Lanes>, root: bool) -> Lanes {
    let empty = || std::array::from_fn(|_| None);
    if returned.is_empty()
        && !program
            .instructions
            .iter()
            .any(|op| op.reciprocal_factor.is_some() || op.product_operation().is_some() && float_width(op.type_code) > 0 && !op.literal)
    {
        return empty();
    }

    let loops: Vec<_> = program.instructions.iter().enumerate().filter_map(|(i, op)| (op.definition.id == "while").then_some(i)).collect();
    let mut origins: Vec<Origins> = Vec::with_capacity(program.instructions.len());
    let mut products: Vec<(Source, Reads)> = Vec::new();
    let mut sums = Vec::new();
    // With straight-line evaluation the instructions visit each assignment before its subsequent reads.
    // A nested call can also expose the rounded assignment to its caller.
    let mut assigned = (root
        && program.instructions.iter().any(|op| op.definition.id == "set")
        && program.instructions.iter().all(|op| op.call.is_none() && !matches!(op.definition.id, "ifelse" | "while")))
    .then(BTreeMap::new);
    let mut unit_products = BTreeSet::new();
    for (index, op) in program.instructions.iter().enumerate() {
        if let Some(lanes) = forwarded(op, &program.instructions, &origins, None) {
            origins.push(lanes);
            continue;
        }
        if let Some(assigned) = &mut assigned {
            let source = |input: usize| origins.get(input).copied().unwrap_or([None; 4]);
            let lanes = match (op.definition.id, op.inputs.as_slice(), &op.value) {
                ("set", &[input], Some(Value::Text(name))) => {
                    let lanes = source(input);
                    if lanes.iter().flatten().all(|&id| products.get(id).is_some_and(|(p, _)| p.operation == Operation::Multiply)) {
                        assigned.insert(name.as_str(), lanes);
                        Some(lanes)
                    } else {
                        assigned.remove(name.as_str());
                        None
                    }
                }
                ("get_float1" | "get_float2" | "get_float3" | "get_float4", [], Some(Value::Text(name))) => assigned.get(name.as_str()).copied(),
                ("sequence", &[_, last], _) | ("passthrough", &[last], _) => Some(source(last)),
                ("mulscalar", &[input, scale], _)
                    if program.instructions.get(scale).is_some_and(|op| op.definition.id == "const_float1" && op.value == Some(Value::Float(1.0))) =>
                {
                    let lanes = source(input);
                    if lanes.iter().any(Option::is_some) {
                        unit_products.insert(index);
                        Some(lanes)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(lanes) = lanes {
                origins.push(lanes);
                continue;
            }
        }

        for source in op.inputs.iter().filter_map(|i| origins.get(*i)).flatten().flatten() {
            if let Some((_, uses)) = products.get_mut(*source) {
                uses.total += 1;
            }
        }
        let width = float_width(op.type_code);
        let lanes_of = |i: usize| origins.get(i).copied().unwrap_or([None; 4]);
        let sides = match (op.definition.id, op.inputs.as_slice()) {
            ("add" | "sub", &[left, right]) => Some([lanes_of(left), lanes_of(right)]),
            ("lerp", &[_, second, weight]) if program.instructions.get(weight).is_some_and(|w| w.type_code == 256) => Some([lanes_of(second), [None; 4]]),
            _ => None,
        };
        if width > 0
            && let Some(sides) = sides
            && sides.iter().flatten().any(Option::is_some)
            && (op.pixel_product_sum.is_none()
                || op.inputs.iter().take(2).any(|input| unit_products.contains(input)) && sides.iter().any(|side| side.iter().all(Option::is_none)))
        {
            for (left, right) in sides[0].iter().zip(sides[1]) {
                if let Some(id) = left.xor(right)
                    && let Some((_, reads)) = products.get_mut(id)
                {
                    reads.sole_product_in_sum += 1;
                }
            }
            sums.push((index, sides));
        }

        let mut lanes = [None; 4];
        if let Some(factor) = op.reciprocal_factor
            && let &[a, _] = op.inputs.as_slice()
        {
            lanes[0] = Some(products.len());
            products.push((Source::local([Factor::Register(a), factor]), Reads::default()));
        } else if let Some(operation) = op.product_operation()
            && !op.literal
            && let &[a, b] = op.inputs.as_slice()
        {
            let operation = if operation == "div" { Operation::Divide { literal_divisor: op.literal_divisor } } else { Operation::Multiply };
            for (i, lane) in lanes.iter_mut().take(width).enumerate() {
                *lane = Some(products.len());
                let j = if op.definition.id == "mulscalar" { 0 } else { i };
                products.push((Source { components: [i as u8, j as u8], operation, ..Source::local([a, b].map(Factor::Register)) }, Reads::default()));
            }
        } else if let Some(exports) = returned.remove(&index) {
            // Two returned lanes can reference the same product. Count their downstream reads together.
            let mut distinct = BTreeMap::new();
            for (lane, source) in lanes.iter_mut().zip(exports) {
                if let Some(mut source) = source {
                    source.calls.insert(0, index);
                    *lane = Some(*distinct.entry(source.clone()).or_insert_with(|| {
                        let id = products.len();
                        products.push((source, Reads::default()));
                        id
                    }));
                }
            }
        }
        origins.push(lanes);
    }

    let output = origins.get(program.output).copied().unwrap_or([None; 4]);
    let exported = output.map(|id| {
        let (source, uses) = products.get(id?)?;
        (uses.total == 0).then(|| source.clone())
    });
    // A caller cannot retain registers that the callee's last loop iteration may have cleared.
    let exported = if loops.is_empty() { exported } else { empty() };
    for id in output.into_iter().flatten() {
        if let Some((_, uses)) = products.get_mut(id) {
            uses.total += 1;
        }
    }
    for (index, sides) in sums {
        let terms: Sum = std::array::from_fn(|i| {
            // Each sum can contract its only product until a separate reader observes the rounded lane.
            sides.iter().enumerate().find_map(|(side, lanes)| {
                let id = lanes.get(i).copied().flatten()?;
                let (source, uses) = products.get(id)?;
                let available = uses.total == 1 || (loops.is_empty() && uses.total == uses.sole_product_in_sum);
                (available && source.live_for_sum(program, &loops, index)).then(|| Term { source: source.clone(), on_right: side == 1 })
            })
        });
        if terms.iter().any(Option::is_some)
            && let Some(op) = program.instructions.get_mut(index)
        {
            op.pixel_product_sum = Some(ProductSum::returned(&op.inputs, terms));
        }
    }
    exported
}
