use super::{FunctionDefinition, Program};
use materialcraft_core::Value;
use std::collections::BTreeMap;

pub(crate) const DEFINITION: FunctionDefinition = FunctionDefinition { id: "instance", label: "Function instance", inputs: &[], category: "Graph" };

pub(crate) struct FunctionCall {
    pub program: Program,
    pub inputs: Vec<String>,
    pub defaults: BTreeMap<String, Value>,
    /// Parent registers for an argument used only in scalar sums within the call.
    pub(super) argument_factors: Option<[super::reciprocal::Factor; 2]>,
    pub(super) assigned_products: Vec<super::assigned_product::Binding>,
    pub(super) reciprocals: Option<Box<super::reciprocal::CallFactors>>,
    pub(super) pattern_arguments: Option<PatternArguments>,
    pub(super) pattern_access: PatternAccess,
    /// The called graph sets variables, directly or in a graph it calls; the caller reads them after the call
    pub exports: bool,
}

/// Which state an inlined pattern function needs in addition to its arguments.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PatternAccess {
    Arguments,
    Random,
    Context,
}

impl PatternAccess {
    pub(super) fn identify(program: &Program, defaults: &BTreeMap<String, Value>) -> Self {
        let mut access = Self::Arguments;
        for op in &program.instructions {
            let id = op.definition.id;
            let reads = if let Some(call) = &op.call {
                call.pattern_access
            } else if id == "rand" {
                Self::Random
            } else if id.starts_with("get_") || id == "isdefined" {
                if matches!(&op.value, Some(Value::Text(name)) if defaults.contains_key(name)) { Self::Arguments } else { Self::Context }
            } else if id.starts_with("const_") || op.depends_only_on_operands() {
                Self::Arguments
            } else {
                Self::Context
            };
            match reads {
                Self::Context => return Self::Context,
                Self::Random => access = Self::Random,
                Self::Arguments => {}
            }
        }
        access
    }
}

/// First uses of scalar arguments in a straight-line pattern function, including directly returned channels.
pub(super) struct PatternArguments {
    pub registers: Vec<usize>,
    pub target: PatternTarget,
    pub channels: [Option<usize>; 4],
}

pub(super) enum PatternTarget {
    Arguments(Vec<usize>),
    ReturnedChannels,
    PackedArgument { width: usize, lanes: [Option<usize>; 4] },
}

impl PatternArguments {
    pub(super) fn identify(program: &Program, names: &[String], defaults: &BTreeMap<String, Value>, registers: &[usize]) -> Option<Self> {
        if names.is_empty() || !defaults.values().all(|v| matches!(v, Value::Float(_))) {
            return None;
        }
        let slots_by_name: BTreeMap<_, _> = names.iter().enumerate().map(|(slot, name)| (name.as_str(), slot)).collect();
        let mut seen = vec![false; names.len()];
        let mut slots = Vec::new();
        let mut origins = Vec::with_capacity(program.instructions.len());
        for op in &program.instructions {
            let mut lanes = [None; 4];
            if let ("get_float1", Some(Value::Text(name))) = (op.definition.id, &op.value) {
                if name.starts_with('$') {
                    return None;
                }
                defaults.get(name)?;
                if let Some(&slot) = slots_by_name.get(name.as_str()) {
                    lanes[0] = Some(slot);
                    if !std::mem::replace(seen.get_mut(slot)?, true) {
                        slots.push(slot);
                    }
                }
            } else if let Some(forwarded) = super::returned_product::forwarded(op, &program.instructions, &origins, None) {
                lanes = forwarded;
            } else if !op.definition.id.starts_with("const_") && (!op.depends_only_on_operands() || matches!(op.definition.id, "ifelse" | "sequence")) {
                // Random draws, assignments and control flow can interleave with the caller's inputs.
                return None;
            }
            origins.push(lanes);
        }
        let selected = origins.get(program.output)?;
        let width = super::returned_product::float_width(program.output_type());
        if width > 0 && selected[..width].iter().all(Option::is_some) {
            slots.clear();
            seen.fill(false);
            for &slot in selected[..width].iter().flatten() {
                if !std::mem::replace(seen.get_mut(slot)?, true) {
                    slots.push(slot);
                }
            }
        }
        Some(Self {
            registers: slots.iter().map(|slot| registers.get(*slot).copied()).collect::<Option<_>>()?,
            target: PatternTarget::Arguments(slots),
            channels: selected.map(|slot| slot.and_then(|i| registers.get(i)).copied()),
        })
    }
}

impl FunctionCall {
    /// A straight-line function can read the components of a packed argument in a
    /// different order from the pack. Repeated reads use the first draw for that lane.
    pub(super) fn packed_arithmetic(&self, registers: &[usize], sources: &[[Option<super::vector::PatternComponent>; 4]]) -> Option<PatternArguments> {
        use super::returned_product::{float_width, forwarded};
        use super::vector::PatternComponent;

        let ([name], &[input]) = (self.inputs.as_slice(), registers) else { return None };
        let width = float_width(super::value_type::type_code(self.defaults.get(name)?));
        if width < 2 || name.starts_with('$') || self.pattern_access != PatternAccess::Arguments {
            return None;
        }
        let supplied = sources.get(input)?;
        if supplied[..width].iter().all(Option::is_none) {
            return None;
        }
        let instructions = self.program.instructions.get(..=self.program.output)?;
        let mut origins = Vec::with_capacity(instructions.len());
        for op in instructions {
            let lanes = match (op.definition.id, &op.value) {
                ("get_float2" | "get_float3" | "get_float4", Some(Value::Text(read))) if read == name && float_width(op.type_code) == width => {
                    std::array::from_fn(|i| (i < width).then_some(PatternComponent::Register(i)))
                }
                ("ifelse" | "sequence", _) => return None,
                (id, _) if id.starts_with("const_") || op.depends_only_on_operands() => {
                    forwarded(op, instructions, &origins, Some(PatternComponent::Zero)).unwrap_or([None; 4])
                }
                _ => return None,
            };
            origins.push(lanes);
        }

        let mut pending = vec![self.program.output];
        let mut visited = vec![false; instructions.len()];
        let mut lanes = [None; 4];
        let mut reads = Vec::new();
        while let Some(index) = pending.pop() {
            if std::mem::replace(visited.get_mut(index)?, true) {
                continue;
            }
            let op = instructions.get(index)?;
            let selected = origins.get(index)?.get(..float_width(op.type_code))?;
            if selected.is_empty() || selected.iter().any(Option::is_none) {
                pending.extend(op.inputs.iter().rev().copied());
                continue;
            }
            for &origin in selected.iter().flatten() {
                let PatternComponent::Register(lane) = origin else { continue };
                let entry = lanes.get_mut(lane)?;
                if entry.is_none() {
                    match supplied.get(lane)? {
                        Some(PatternComponent::Register(source)) => {
                            *entry = Some(reads.len());
                            reads.push(*source);
                        }
                        Some(PatternComponent::Zero) => {}
                        None => return None,
                    }
                }
            }
        }
        if lanes[..width].iter().copied().eq((0..width).map(Some)) {
            return None;
        }
        Some(PatternArguments { registers: reads, target: PatternTarget::PackedArgument { width, lanes }, channels: [None; 4] })
    }

    /// A function made entirely of argument reads, vector packs and swizzles can return
    /// caller components directly. Sampling one of its return channels then stays lazy.
    pub(super) fn packed_projection(&self, registers: &[usize], sources: &[[Option<super::vector::PatternComponent>; 4]]) -> Option<PatternArguments> {
        if !self.defaults.values().any(|value| super::returned_product::float_width(super::value_type::type_code(value)) > 1) {
            return None;
        }
        let mut origins = Vec::with_capacity(self.program.instructions.len());
        for op in &self.program.instructions {
            let lanes = match (op.definition.id, &op.value) {
                ("get_float1" | "get_float2" | "get_float3" | "get_float4", Some(Value::Text(name))) => {
                    if name.starts_with('$') || super::value_type::type_code(self.defaults.get(name)?) != op.type_code {
                        return None;
                    }
                    let slot = self.inputs.iter().position(|input| input == name)?;
                    sources.get(*registers.get(slot)?)?.map(|component| match component {
                        Some(super::vector::PatternComponent::Register(register)) => Some(register),
                        _ => None,
                    })
                }
                _ => super::returned_product::forwarded(op, &self.program.instructions, &origins, None)?,
            };
            origins.push(lanes);
        }
        let channels = *origins.get(self.program.output)?;
        let width = super::returned_product::float_width(self.program.output_type());
        if width == 0 {
            return None;
        }
        Some(PatternArguments { registers: channels.get(..width)?.iter().copied().collect::<Option<_>>()?, target: PatternTarget::ReturnedChannels, channels })
    }
}

pub(super) fn pack_scalars(values: &[Value]) -> Result<Value, String> {
    let mut channels = [0.0; 4];
    let channels = channels.get_mut(..values.len()).ok_or("too many color components")?;
    for (channel, value) in channels.iter_mut().zip(values) {
        let Value::Float(component) = value else { return Err("expected a scalar color component".into()) };
        *channel = *component;
    }
    super::value_type::intermediate_float_value(channels)
}

impl PatternTarget {
    pub(super) fn packed_argument(&self, values: &[Value]) -> Result<Option<Value>, String> {
        let Self::PackedArgument { width, lanes } = self else { return Ok(None) };
        let mut channels = [0.0; 4];
        let channels = channels.get_mut(..*width).ok_or("invalid packed argument width")?;
        for (channel, source) in channels.iter_mut().zip(lanes) {
            if let Some(index) = source {
                let Some(Value::Float(value)) = values.get(*index) else { return Err("expected a scalar argument component".into()) };
                *channel = *value;
            }
        }
        super::value_type::intermediate_float_value(channels).map(Some)
    }
}

/// Wire presence is local to a call. Free variables can be defined by an executed Set in a caller.
pub(super) struct CallBindings<'a> {
    pub call: &'a FunctionCall,
    pub caller_assignments: &'a BTreeMap<String, Value>,
    pub parent: Option<&'a CallBindings<'a>>,
}

impl CallBindings<'_> {
    pub(super) fn is_defined(&self, name: &str, assignments: &BTreeMap<String, Value>) -> bool {
        if self.call.defaults.contains_key(name) {
            return self.call.inputs.iter().any(|input| input == name);
        }
        if assignments.contains_key(name) {
            return true;
        }
        let mut frame = Some(self);
        while let Some(current) = frame {
            if current.caller_assignments.contains_key(name) {
                return true;
            }
            frame = current.parent;
        }
        false
    }
}
