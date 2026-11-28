use super::{EvaluationState, arithmetic::flush_subnormal, instruction::Instruction, reciprocal::Factor, returned_product::Source, value_type};
use materialcraft_core::Value;
use std::collections::BTreeMap;

#[derive(Clone)]
enum Addend {
    Argument(usize),
    Literal(Value),
}

enum Retained {
    Argument { source: Source, addend: Addend, subtract: bool },
    PassedArgument,
    Assigned(super::assigned_product::Binding),
    Lanes(super::returned_product::Sum),
}

pub(super) struct ProductSum {
    // Visit operands in their original order, including reads with side effects.
    pub(super) inputs: Vec<usize>,
    product_on_right: bool,
    negated: bool,
    operation: &'static str,
    literal_divisor: bool,
    rounded_components: u8,
    retained: Option<Box<Retained>>,
}

impl ProductSum {
    /// Evaluate the original call and vector arguments before reading the factors they used.
    pub(super) fn returned(inputs: &[usize], terms: super::returned_product::Sum) -> Self {
        Self {
            inputs: inputs.to_vec(),
            product_on_right: false,
            negated: false,
            operation: "mul",
            literal_divisor: false,
            rounded_components: 0,
            retained: Some(Box::new(Retained::Lanes(terms))),
        }
    }

    pub(super) fn assigned(inputs: &[usize], on_right: bool, source: super::assigned_product::Binding) -> Self {
        Self {
            inputs: inputs.to_vec(),
            product_on_right: on_right,
            negated: false,
            operation: "mulscalar",
            literal_divisor: false,
            rounded_components: source.rounded_components,
            retained: Some(Box::new(Retained::Assigned(source))),
        }
    }

    pub(super) fn evaluate(&self, arguments: &[Value], subtract: bool, state: &EvaluationState) -> Result<Value, String> {
        if let Some(Retained::Lanes(terms)) = self.retained.as_deref() {
            return super::returned_product::evaluate_sum(terms, arguments, subtract, state);
        }
        let passed;
        let (a, b, c, blend, subtract) = if let Some(Retained::Argument { source, addend, subtract }) = self.retained.as_deref() {
            passed = source.factors(state)?.map(Value::Float);
            let c = match addend {
                Addend::Argument(i) => arguments.get(*i).ok_or("missing product addend")?,
                Addend::Literal(value) => value,
            };
            (&passed[0], &passed[1], c, None, *subtract)
        } else if let Some(Retained::Assigned(source)) = self.retained.as_deref() {
            let [left, right] = arguments else { return Err("missing product sum operands".into()) };
            let (product, addend) = if self.product_on_right { (right, left) } else { (left, right) };
            // An untaken assignment leaves the getter's ordinary value in effect.
            passed = source.read(state).unwrap_or_else(|| [product.clone(), Value::Float(1.0)]);
            (&passed[0], &passed[1], addend, None, subtract)
        } else if matches!(self.retained.as_deref(), Some(Retained::PassedArgument)) {
            let [left, right] = arguments else { return Err("missing scalar sum operands".into()) };
            passed = state.argument_factors.ok_or("missing scalar argument factors")?.map(Value::Float);
            (&passed[0], &passed[1], if self.product_on_right { left } else { right }, None, subtract)
        } else {
            let [first, second, third, rest @ ..] = arguments else { return Err("missing product-sum operands".into()) };
            let (a, b, c) = if self.product_on_right { (second, third, first) } else { (first, second, third) };
            (a, b, c, rest.first(), subtract)
        };
        let width = value_type::dimensions(a);
        if width != value_type::dimensions(c) || (width != value_type::dimensions(b) && !(self.operation == "mulscalar" && value_type::dimensions(b) == 1)) {
            return Err("float operands must have the same dimensions".into());
        }
        if blend.is_some_and(|v| value_type::dimensions(v) != width && value_type::dimensions(v) != 1) {
            return Err("interpolation factor must be a scalar or match its operands".into());
        }
        let blend = blend.map(value_type::floats).transpose()?;
        let a = value_type::floats(a)?.map(flush_subnormal);
        let b = value_type::floats(b)?.map(flush_subnormal);
        let c = value_type::floats(c)?.map(flush_subnormal);
        let negate_product = self.negated ^ (subtract && self.product_on_right);
        let negate_addend = (subtract && !self.product_on_right) || blend.is_some();
        let result: [f32; 4] = std::array::from_fn(|i| {
            let factor = if negate_product { -a[i] } else { a[i] };
            let addend = if negate_addend { -c[i] } else { c[i] };
            let sum = if self.rounded_components & (1 << i) != 0 {
                let product = if self.operation == "div" { super::reciprocal::divide_by(factor, b[i], self.literal_divisor) } else { factor * b[i] };
                flush_subnormal(product) + addend
            } else if self.operation == "div" {
                super::reciprocal::divide_add(factor, b[i], addend, self.literal_divisor)
            } else {
                factor.mul_add(b[i], addend)
            };
            blend.map_or(sum, |t| flush_subnormal(sum).mul_add(t[i], c[i]))
        })
        .map(flush_subnormal);
        value_type::intermediate_float_value(result.get(..width).ok_or("invalid product-sum dimensions")?)
    }
}

#[derive(Clone, Copy)]
struct ProductOperand {
    index: usize,
    negated: bool,
}

#[derive(Clone, Copy, Default)]
struct ProductUsage {
    count: usize,
    rounded: bool,
    swizzles: usize,
    selected_components: u8,
    repeated: bool,
}

pub(super) fn selected_components(op: &Instruction) -> Option<u8> {
    let count = match op.definition.id {
        "swizzle1" => 1,
        "swizzle2" => 2,
        "swizzle3" => 3,
        "swizzle4" => 4,
        _ => return None,
    };
    let components = value_type::integers(op.value.as_ref()?).ok()?;
    components.iter().take(count).try_fold(0, |mask, &i| (0..4).contains(&i).then(|| mask | 1 << i))
}

fn scalar_product(op: &Instruction) -> Option<(&'static str, [Factor; 2])> {
    let &[a, b] = op.inputs.as_slice() else { return None };
    if op.literal || op.type_code != 256 {
        return None;
    }
    match (op.definition.id, op.reciprocal_factor) {
        ("div", Some(factor)) => Some(("mul", [Factor::Register(a), factor])),
        (operation @ ("mul" | "div"), _) => Some((operation, [a, b].map(Factor::Register))),
        _ => None,
    }
}

/// A scalar argument can contract with a callee's addition when that addition is its only use.
fn called_sum(op: &Instruction, instructions: &[Instruction], usage: &[ProductUsage]) -> Option<ProductSum> {
    let call = op.call.as_ref()?;
    let body = &call.program;
    let sum = body.instructions.get(body.output)?;
    if sum.type_code != 256 || !matches!(sum.definition.id, "add" | "sub") {
        return None;
    }
    let leaf = |index: usize| {
        let node = body.instructions.get(index)?;
        match (node.definition.id, node.value.as_ref()?) {
            ("get_float1", Value::Text(name)) if !name.starts_with('$') => call.inputs.iter().position(|p| p == name).map(Addend::Argument),
            ("const_float1", value @ Value::Float(_)) => Some(Addend::Literal(value.clone())),
            _ => None,
        }
    };
    let &[left, right] = sum.inputs.as_slice() else { return None };
    if left == right {
        return None;
    }
    let (left, right) = (leaf(left)?, leaf(right)?);
    if matches!((&left, &right), (Addend::Argument(a), Addend::Argument(b)) if a == b) {
        return None;
    }
    let mut terms = [(&left, &right, false), (&right, &left, true)].into_iter().filter_map(|(operand, addend, product_on_right)| {
        let Addend::Argument(slot) = operand else { return None };
        let index = *op.inputs.get(*slot)?;
        let product = instructions.get(index)?;
        if usage.get(index)?.count != 1 {
            return None;
        }
        let (operation, factors) = scalar_product(product)?;
        let retained = Retained::Argument { source: Source::local(factors), addend: addend.clone(), subtract: sum.definition.id == "sub" };
        Some(ProductSum {
            inputs: op.inputs.clone(),
            product_on_right,
            negated: false,
            operation,
            literal_divisor: product.literal_divisor,
            rounded_components: 0,
            retained: Some(Box::new(retained)),
        })
    });
    let term = terms.next()?;
    terms.next().is_none().then_some(term)
}

fn pass_into_sums(call: &mut super::function_call::FunctionCall, inputs: &[usize], previous: &[Instruction], usage: &[ProductUsage]) {
    let mut candidates = call.inputs.iter().zip(inputs).filter_map(|(name, &index)| {
        let product = previous.get(index)?;
        let (operation, factors) = scalar_product(product)?;
        (usage.get(index)?.count == 1).then_some((name, product.literal_divisor, operation, factors))
    });
    let Some((name, literal_divisor, operation, factors)) = candidates.next() else { return };
    let body = &mut call.program.instructions;
    if candidates.next().is_some() || call.exports || body.iter().any(|op| op.call.is_some() || matches!(op.definition.id, "ifelse" | "sequence" | "while")) {
        return;
    }

    let reads: Vec<_> = body.iter().map(|op| op.definition.id == "get_float1" && matches!(&op.value, Some(Value::Text(id)) if id == name)).collect();
    let mut sums = Vec::new();
    for (index, op) in body.iter().enumerate() {
        let mut uses = op.inputs.iter().enumerate().filter(|(_, input)| reads.get(**input) == Some(&true));
        let Some((slot, _)) = uses.next() else { continue };
        // Any other reader observes the rounded argument and makes it common to these sums.
        if uses.next().is_some() || op.type_code != 256 || !matches!(op.definition.id, "add" | "sub") || op.pixel_product_sum.is_some() {
            return;
        }
        sums.push((index, slot == 1));
    }
    if sums.is_empty() {
        return;
    }
    call.argument_factors = Some(factors);
    for (index, product_on_right) in sums {
        if let Some(sum) = body.get_mut(index) {
            sum.pixel_product_sum = Some(ProductSum {
                inputs: sum.inputs.clone(),
                product_on_right,
                negated: false,
                operation,
                literal_divisor,
                rounded_components: 0,
                retained: Some(Box::new(Retained::PassedArgument)),
            });
        }
    }
}

pub(super) fn identify(instructions: &mut [Instruction], live_readers: Option<&[bool]>) {
    // A sign change can be applied to a factor inside the fused operation. Follow inputs in
    // compilation order so even a long chain of negations takes a single bounded pass.
    let mut operands: Vec<Option<ProductOperand>> = Vec::with_capacity(instructions.len());
    for (index, op) in instructions.iter().enumerate() {
        let product = if op.product_operation().is_some() && !op.literal && matches!(op.type_code, 256 | 512 | 1024 | 2048) {
            Some(ProductOperand { index, negated: false })
        } else if op.definition.id == "neg"
            && let [input] = op.inputs.as_slice()
        {
            operands.get(*input).copied().flatten().map(|p| ProductOperand { negated: !p.negated, ..p })
        } else {
            None
        };
        operands.push(product);
    }
    let operand = |index: usize| operands.get(index).copied().flatten();
    let mut usage = vec![ProductUsage::default(); instructions.len()];
    let has_loop = instructions.iter().any(|op| op.definition.id == "while");
    for (index, op) in instructions.iter().enumerate() {
        if op.definition.id == "while" {
            for &member in &op.control_registers {
                if let Some(reader) = usage.get_mut(member) {
                    reader.repeated = true;
                }
            }
        }
        if op.definition.id == "neg" || live_readers.is_some_and(|readers| readers.get(index) == Some(&false)) {
            continue;
        }
        let sum = match (op.definition.id, op.inputs.as_slice()) {
            ("add" | "sub", [a, b]) => operand(*a).zip(operand(*b)).is_none_or(|(p, q)| p.index != q.index),
            _ => false,
        };
        for (slot, &input) in op.inputs.iter().enumerate() {
            if let Some(p) = operand(input) {
                usage[p.index].count += 1;
                if let Some(components) = selected_components(op) {
                    usage[p.index].swizzles += 1;
                    usage[p.index].selected_components |= components;
                }
                // In lerp(a,b,t), b can contract into b-a. Both uses of a need its rounded value.
                usage[p.index].rounded |= !(sum || (op.definition.id == "lerp" && slot == 1));
            }
        }
    }

    // If a sum consumes a single-use product, the shared product on its other side is a
    // rounded addend. Its other readers must see the same value.
    for op in instructions.iter().filter(|op| matches!(op.definition.id, "add" | "sub")) {
        if let [left, right] = op.inputs.as_slice()
            && let (Some(left), Some(right)) = (operand(*left), operand(*right))
        {
            for (single, shared) in [(left.index, right.index), (right.index, left.index)] {
                if usage[single].count == 1 && !usage[single].rounded && usage[shared].count > 1 {
                    usage[shared].rounded = true;
                }
            }
        }
    }

    let mut shared_pairs = BTreeMap::new();
    for index in 0..instructions.len() {
        let Some(op) = instructions.get(index) else { continue };
        if !matches!(op.type_code, 256 | 512 | 1024 | 2048) {
            continue;
        }
        if op.call.is_some() {
            if !has_loop {
                let (previous, current) = instructions.split_at_mut(index);
                if let Some(op) = current.first_mut() {
                    op.pixel_product_sum = called_sum(op, previous, &usage);
                    if op.pixel_product_sum.is_none()
                        && let Some(call) = &mut op.call
                    {
                        pass_into_sums(call, &op.inputs, previous, &usage);
                    }
                }
            }
            continue;
        }
        let (left, right, blend) = match (op.definition.id, op.inputs.as_slice()) {
            ("add" | "sub", &[left, right]) => (left, right, None),
            ("lerp", &[left, right, factor]) => (left, right, Some(factor)),
            _ => continue,
        };
        let mut sides = [(left, right, false), (right, left, true)];
        if op.type_code == 256
            && blend.is_none()
            && let (Some(p), Some(q)) = (operand(left), operand(right))
            && p.index != q.index
            && [p.index, q.index].iter().all(|&i| usage[i].count > 1 && instructions[i].product_operation() == Some("mul"))
        {
            // Sums sharing both products reuse the rounded addend chosen by the first sum.
            let pair = (p.index.min(q.index), p.index.max(q.index));
            if *shared_pairs.entry(pair).or_insert(p.index) == q.index {
                sides.swap(0, 1);
            }
        }
        let product = sides
            .into_iter()
            .filter(|(_, _, on_right)| blend.is_none() || *on_right)
            .filter_map(|(source, other, product_on_right)| {
                let source = operand(source)?;
                let product = instructions.get(source.index)?;
                let used = usage[source.index];
                let &[a, b] = product.inputs.as_slice() else { return None };
                // A swizzle observes only its selected components. The other components may still contract with their sole sum.
                let vector_mask = match product.type_code {
                    512 => 0b0011,
                    1024 => 0b0111,
                    2048 => 0b1111,
                    _ => 0,
                };
                let partial = used.swizzles > 0 && used.count == used.swizzles + 1 && vector_mask & !used.selected_components != 0 && blend.is_none();
                if (used.rounded && !partial) || product.type_code != op.type_code {
                    return None;
                }
                // Loop iterations can invalidate an operand independently of a shared product.
                if has_loop && used.count > 1 && (!partial || used.repeated || usage.get(a)?.repeated || usage.get(b)?.repeated) {
                    return None;
                }
                let mut inputs = if product_on_right { vec![other, a, b] } else { vec![a, b, other] };
                inputs.extend(blend);
                Some((
                    used.count,
                    ProductSum {
                        inputs,
                        product_on_right,
                        negated: source.negated,
                        operation: product.product_operation()?,
                        literal_divisor: product.literal_divisor,
                        rounded_components: used.selected_components,
                        retained: None,
                    },
                ))
            })
            .min_by_key(|(count, _)| *count)
            .map(|(_, sum)| sum);
        if let Some(op) = instructions.get_mut(index) {
            op.pixel_product_sum = product;
        }
    }
}
