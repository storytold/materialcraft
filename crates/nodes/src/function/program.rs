use super::{
    EvaluationContext, EvaluationState, evaluation_step::EvaluationStep, function_call::CallBindings, instruction::Instruction, operation, value_type,
};
use crate::random::FunctionRandom;
use materialcraft_core::Value;

/// Validated graph with input ids resolved to instruction indices
pub struct Program {
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) output: usize,
    pub(crate) seed: u32,
    pub(super) processor_branches: std::collections::BTreeMap<usize, super::branch_inputs::BranchInputs>,
    pub(super) pattern_branches: std::collections::BTreeMap<usize, super::branch_inputs::BranchInputs>,
    pub(super) pattern_components: std::collections::BTreeMap<usize, super::vector::PatternComponent>,
    pub(super) sequence_prefixes: Vec<super::sequence_prefix::Visit>,
    pub(super) loop_sums: std::collections::BTreeMap<usize, super::loop_sum::LoopSum>,
}

const MAX_EVALUATION_STEPS: usize = 100_000;

impl Program {
    pub fn output_type(&self) -> u32 {
        self.instructions.get(self.output).map_or(0, |instruction| instruction.type_code)
    }

    /// The variable read by a direct link. Disconnected editor nodes have no instructions.
    pub fn input_identifier(&self) -> Option<&str> {
        let [op] = self.instructions.as_slice() else { return None };
        match &op.value {
            Some(Value::Text(name)) if op.definition.id.starts_with("get_") => Some(name),
            _ => None,
        }
    }

    /// Whether the result depends only on literals and pure operations.
    pub fn output_is_literal(&self) -> bool {
        self.instructions.get(self.output).is_some_and(|instruction| instruction.literal)
    }

    /// A grayscale Uniform folds its channel mean when its property has only fixed operands.
    pub fn uses_fixed_uniform_color(&self, variables: &std::collections::BTreeMap<String, Value>) -> bool {
        self.instructions.iter().all(|op| {
            op.literal
                || op.depends_only_on_operands()
                || op.definition.id == "isdefined"
                || match (&op.value, op.definition.id) {
                    (Some(Value::Text(name)), getter) if getter.starts_with("get_") && !name.starts_with('$') => !variables.contains_key(name),
                    (Some(Value::Text(name)), "get_float2") => matches!(name.as_str(), "$size" | "$sizelog2"),
                    (Some(Value::Text(name)), "get_integer1") => {
                        matches!(name.as_str(), "$tiling" | "$pixelratio") || super::SystemVariable::named(name).is_none()
                    }
                    (Some(Value::Text(name)), "get_integer2") => name == "$uvtile",
                    (Some(Value::Text(name)), "get_float3") => name == "$physicalsize",
                    _ => false,
                }
        })
    }

    /// Whether the property reads a bound graph parameter, even if its final value is constant.
    pub fn reads_graph_parameter(&self, variables: &std::collections::BTreeMap<String, Value>) -> bool {
        self.reads_bound_input(|name| variables.contains_key(name))
    }

    pub(crate) fn reads_bound_input(&self, bound: impl Fn(&str) -> bool) -> bool {
        self.instructions.iter().any(|op| {
            !op.literal && op.definition.id.starts_with("get_") && matches!(&op.value, Some(Value::Text(name)) if !name.starts_with('$') && bound(name))
        })
    }

    /// Unbound user inputs in a fixed pattern image selector contribute their typed zero.
    pub(crate) fn uses_fixed_image_slot(&self, variables: &std::collections::BTreeMap<String, Value>) -> bool {
        self.output_is_literal()
            || self.instructions.iter().all(|op| {
                op.literal
                    || op.depends_only_on_operands()
                    || (op.definition.id.starts_with("get_")
                        && matches!(&op.value, Some(Value::Text(name)) if !name.starts_with('$') && !variables.contains_key(name)))
            })
    }

    /// Whether constants and the current image dimensions determine the entire computation.
    pub fn depends_only_on_image_size(&self) -> bool {
        self.instructions.iter().all(|op| {
            op.definition.id.starts_with("const_")
                || op.depends_only_on_operands()
                || (op.definition.id == "get_float2" && matches!(&op.value, Some(Value::Text(name)) if matches!(name.as_str(), "$size" | "$sizelog2")))
        })
    }

    pub fn node_type(&self, id: u32) -> Option<u32> {
        self.instructions.iter().find(|instruction| instruction.id == id).map(|instruction| instruction.type_code)
    }

    fn instruction(&self, index: usize) -> Result<&Instruction, String> {
        self.instructions.get(index).ok_or_else(|| "invalid function instruction".into())
    }

    fn input(&self, index: usize, slot: usize) -> Result<usize, String> {
        self.instruction(index)?.inputs.get(slot).copied().ok_or_else(|| "missing function input".into())
    }

    pub fn evaluate(&self, context: &EvaluationContext<'_>, state: &mut EvaluationState) -> Result<Value, String> {
        state.variables.clone_from(context.variables);
        let mut random = self.random_stream(context);
        self.evaluate_finite(context, state, &mut random)
    }

    /// Pattern expressions borrow inherited variables and export only their explicit assignments.
    pub(crate) fn evaluate_with_random(
        &self,
        context: &EvaluationContext<'_>,
        state: &mut EvaluationState,
        random: &mut FunctionRandom,
    ) -> Result<Value, String> {
        state.variables.clear();
        self.evaluate_finite(context, state, random)
    }

    pub(crate) fn evaluate_pattern_seed<'a>(
        &self,
        context: &EvaluationContext<'_>,
        state: &mut EvaluationState,
        random: &mut FunctionRandom,
        assigned: impl Iterator<Item = (&'a str, i32)>,
    ) -> Result<Value, String> {
        state.variables.clear();
        // An integer written by an earlier property occupies a float slot. Published inputs
        // and values assigned within this seed expression use their ordinary integer words.
        state.variables.extend(assigned.map(|(name, value)| (name.to_owned(), Value::Int((value as f32).to_bits() as i32))));
        self.evaluate_finite(context, state, random)
    }

    /// A quadrant blends its tint components individually, including an undefined channel.
    pub(crate) fn evaluate_pattern_tint(
        &self,
        context: &EvaluationContext<'_>,
        state: &mut EvaluationState,
        random: &mut FunctionRandom,
    ) -> Result<Value, String> {
        state.variables.clear();
        self.run_with_random(context, state, random)
    }

    fn evaluate_finite(&self, context: &EvaluationContext<'_>, state: &mut EvaluationState, random: &mut FunctionRandom) -> Result<Value, String> {
        let value = self.run_with_random(context, state, random)?;
        // Callers may clamp undefined intermediates; completed functions must be finite
        if value.as_vec4().is_some_and(|components| components.iter().any(|value| !value.is_finite())) {
            return Err("function produced a non-finite output".into());
        }
        Ok(value)
    }

    /// A grayscale pattern can blend an infinite or undefined scalar into a finite background.
    pub fn evaluate_pattern_opacity(&self, context: &EvaluationContext<'_>, state: &mut EvaluationState) -> Result<f32, String> {
        state.variables.clone_from(context.variables);
        let mut random = self.random_stream(context);
        match self.run_with_random(context, state, &mut random)? {
            Value::Float(opacity) => Ok(opacity),
            _ => Err("pattern opacity requires a scalar".into()),
        }
    }

    /// Evaluate an integer texture's channel control at its normalized storage boundary.
    pub fn evaluate_normalized_color(&self, context: &EvaluationContext<'_>, state: &mut EvaluationState) -> Result<Value, String> {
        state.variables.clone_from(context.variables);
        let mut random = self.random_stream(context);
        let channels = match self.run_with_random(context, state, &mut random)? {
            Value::Float(v) => [v, v, v, 1.0],
            Value::Float3([r, g, b]) => [r, g, b, 1.0],
            Value::Float4(rgba) | Value::Color(rgba) => rgba,
            _ => return Err("channel control requires a gray level, RGB or RGBA".into()),
        };
        Ok(Value::Float4(channels.map(|v| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) })))
    }

    fn run_with_random(&self, context: &EvaluationContext<'_>, state: &mut EvaluationState, random: &mut FunctionRandom) -> Result<Value, String> {
        state.random = *random;
        let mut remaining = MAX_EVALUATION_STEPS;
        let result = self.run(context, state, &mut remaining, None);
        *random = state.random;
        result
    }

    fn random_stream(&self, context: &EvaluationContext<'_>) -> FunctionRandom {
        let seed = context.variables.get("$randomseed").and_then(Value::as_i32).map_or(self.seed, |seed| seed as u32);
        match context.scope {
            super::FunctionScope::PixelProcessor => FunctionRandom::pixel(seed, context.position),
            super::FunctionScope::ValueProcessor => FunctionRandom::pixel(seed, [0.0; 2]),
            super::FunctionScope::FxMap => FunctionRandom::image(seed),
            super::FunctionScope::Node | super::FunctionScope::RandomSeed | super::FunctionScope::FxMapRandomSeed => FunctionRandom::uniform(seed),
        }
    }

    // Pixel assignments are local; only the general evaluator exports the complete environment
    pub(super) fn evaluate_pixel(&self, context: &EvaluationContext<'_>, state: &mut EvaluationState) -> Result<Value, String> {
        state.variables.clear();
        state.random = self.random_stream(context);
        let mut remaining = MAX_EVALUATION_STEPS;
        self.run(context, state, &mut remaining, None)
    }

    fn run(
        &self,
        context: &EvaluationContext<'_>,
        state: &mut EvaluationState,
        remaining_steps: &mut usize,
        bindings: Option<&CallBindings<'_>>,
    ) -> Result<Value, String> {
        use EvaluationStep::*;

        state.steps.clear();
        state.values.clear();
        state.assignments.clear();
        state.registers.clear();
        state.registers.resize(self.instructions.len(), None);
        state.visited_prefixes.clear();
        state.visited_prefixes.resize(self.sequence_prefixes.len(), false);
        state.steps.push(Enter(self.output));

        while let Some(step) = state.steps.pop() {
            *remaining_steps = remaining_steps.checked_sub(1).ok_or("function exceeded its execution budget; check loop termination")?;
            match step {
                Enter(index) => {
                    let instruction = self.instruction(index)?;
                    if let Some(Some(value)) = state.registers.get(index) {
                        state.values.push(value.clone());
                        continue;
                    }

                    state.steps.push(Store(index));
                    if context.scope.is_pattern()
                        && let Some(source) = self.pattern_components.get(&index)
                    {
                        match source {
                            super::vector::PatternComponent::Register(source) => state.steps.push(Enter(*source)),
                            super::vector::PatternComponent::Zero => state.values.push(Value::Float(0.0)),
                        }
                        continue;
                    }

                    match instruction.definition.id {
                        "set" if matches!(&instruction.value, Some(Value::Text(name)) if bindings.is_some_and(|scope| scope.call.defaults.contains_key(name))) =>
                        {
                            // A formal is read-only. Even side effects in the unused operand are skipped.
                            let kind = materialcraft_core::ValueType::from_code(instruction.type_code).ok_or("unknown function assignment type")?;
                            state.values.push(kind.default_value());
                        }
                        "ifelse" => {
                            state.steps.push(Branch { instruction: index, prefix: false });
                            state.steps.push(Enter(self.input(index, 0)?));
                        }
                        "sequence" if !self.sequence_prefixes.is_empty() => {
                            state.steps.push(Enter(self.input(index, 1)?));
                            state.steps.push(Prefix(self.input(index, 0)?));
                        }
                        "sequence" => {
                            state.steps.push(Sequence(index));
                            state.steps.push(Enter(self.input(index, 0)?));
                        }
                        "while" => {
                            state.steps.push(LoopStart(index));
                            state.steps.push(Enter(self.input(index, 0)?));
                        }
                        _ => {
                            state.steps.push(Apply { instruction: index, base: state.values.len() });
                            let inputs = match &instruction.pixel_product_sum {
                                Some(product) if context.scope == super::FunctionScope::PixelProcessor => product.inputs.as_slice(),
                                _ if context.scope.is_pattern() => instruction
                                    .call
                                    .as_ref()
                                    .and_then(|call| call.pattern_arguments.as_ref())
                                    .map_or(instruction.inputs.as_slice(), |arguments| arguments.registers.as_slice()),
                                _ => instruction.inputs.as_slice(),
                            };
                            state.steps.extend(inputs.iter().rev().map(|&index| Enter(index)));
                        }
                    }
                }
                Apply { instruction: index, base } => {
                    let instruction = self.instruction(index)?;
                    let arguments = state.values.get(base..).ok_or("invalid function operands")?;
                    let pattern = instruction.call.as_ref().and_then(|call| call.pattern_arguments.as_ref()).filter(|_| context.scope.is_pattern());
                    let result = if context.scope == super::FunctionScope::PixelProcessor
                        && let Some(factor) = instruction.reciprocal_factor
                        && let Some(product) = super::reciprocal::multiply_original(arguments.first(), factor.read(state))
                    {
                        Ok(product)
                    } else if context.scope == super::FunctionScope::PixelProcessor
                        && let Some(value) = instruction.pixel_interval.as_deref().and_then(|interval| interval.saturate(state))
                    {
                        Ok(value)
                    } else if let Some(product) = &instruction.pixel_product_sum
                        && context.scope == super::FunctionScope::PixelProcessor
                    {
                        product.evaluate(arguments, instruction.definition.id == "sub", state)
                    } else if pattern.is_some_and(|plan| matches!(plan.target, super::function_call::PatternTarget::ReturnedChannels)) {
                        super::function_call::pack_scalars(arguments)
                    } else if let Some(call) = &instruction.call {
                        let factors = if let Some([a, b]) = call.argument_factors.filter(|_| context.scope == super::FunctionScope::PixelProcessor) {
                            Some([a.read(state).ok_or("missing scalar argument factor")?, b.read(state).ok_or("missing scalar argument factor")?])
                        } else {
                            None
                        };
                        let reciprocals = call.reciprocals.as_deref().filter(|_| context.scope == super::FunctionScope::PixelProcessor);
                        if let Some(plan) = reciprocals
                            && !plan.arguments.is_empty()
                        {
                            let mut values = state.calls.get_mut(&index).map(|child| std::mem::take(&mut child.reciprocal_arguments)).unwrap_or_default();
                            values.clear();
                            values.extend(plan.arguments.iter().map(|factor| factor.read(state)));
                            state.calls.entry(index).or_default().reciprocal_arguments = values;
                        }
                        if !call.assigned_products.is_empty() && context.scope == super::FunctionScope::PixelProcessor {
                            let mut values = state.calls.get_mut(&index).map(|child| std::mem::take(&mut child.assigned_arguments)).unwrap_or_default();
                            values.clear();
                            values.extend(call.assigned_products.iter().map(|binding| binding.read(state)));
                            state.calls.entry(index).or_default().assigned_arguments = values;
                        }
                        let storage = state.calls.entry(index).or_default();
                        storage.argument_factors = factors;
                        storage.variables.clone_from(&state.variables);
                        storage.variables.extend(call.defaults.clone());
                        let packed = pattern.map(|plan| plan.target.packed_argument(arguments)).transpose()?.flatten();
                        let arguments = packed.as_ref().map_or(arguments, std::slice::from_ref);
                        for (i, value) in arguments.iter().enumerate() {
                            let slot = match pattern.map(|plan| &plan.target) {
                                Some(super::function_call::PatternTarget::Arguments(slots)) => *slots.get(i).ok_or("missing function argument slot")?,
                                _ => i,
                            };
                            let name = call.inputs.get(slot).ok_or("missing function argument name")?;
                            storage.variables.insert(name.clone(), value.clone());
                        }

                        // A function call consumes its caller's stream instead of restarting it
                        storage.random = state.random;
                        let child = CallBindings { call, caller_assignments: &state.assignments, parent: bindings };
                        let result = call.program.run(context, storage, remaining_steps, Some(&child));
                        if let Some(plan) = reciprocals {
                            storage.returned_reciprocals = plan.results.map(|factor| factor.and_then(|f| f.read(storage)));
                        }
                        state.random = storage.random;
                        if call.exports && result.is_ok() {
                            // A free assignment can pass through a caller with a formal of the same name.
                            for (name, value) in &storage.assignments {
                                if bindings.is_none_or(|scope| !scope.call.defaults.contains_key(name)) {
                                    state.variables.insert(name.clone(), value.clone());
                                }
                                state.assignments.insert(name.clone(), value.clone());
                            }
                        }

                        result
                    } else {
                        operation::evaluate(
                            instruction,
                            arguments,
                            &mut state.variables,
                            &mut state.random,
                            context,
                            bindings.map(|call| (call, &state.assignments)),
                        )
                    }
                    .map_err(|error| format!("{} ({}): {error}", instruction.definition.label, instruction.id))?;

                    if instruction.definition.id == "set"
                        && let Some(Value::Text(name)) = &instruction.value
                    {
                        state.assignments.insert(name.clone(), result.clone());
                    }

                    state.values.truncate(base);
                    state.values.push(result);
                }
                Store(index) => {
                    let value = state.values.last().ok_or("missing function result")?;
                    let register = state.registers.get_mut(index).ok_or("invalid function register")?;
                    *register = Some(value.clone());
                }
                Prefix(index) => {
                    use super::sequence_prefix::Visit;
                    if state.registers.get(index).is_some_and(Option::is_some)
                        || state.visited_prefixes.get_mut(index).is_some_and(|seen| std::mem::replace(seen, true))
                    {
                        continue;
                    }
                    match self.sequence_prefixes.get(index).copied().unwrap_or(Visit::Value) {
                        Visit::Skip => {}
                        Visit::Value => {
                            state.steps.push(Discard);
                            state.steps.push(Enter(index));
                        }
                        Visit::Inputs => state.steps.extend(self.instruction(index)?.inputs.iter().rev().copied().map(Prefix)),
                        Visit::Branch => {
                            state.steps.push(Branch { instruction: index, prefix: true });
                            state.steps.push(Enter(self.input(index, 0)?));
                        }
                    }
                }
                Branch { instruction: index, prefix } => {
                    let condition = state.values.pop().ok_or("missing branch condition")?;
                    let take_true = value_type::boolean(&condition)?;
                    let selected = self.input(index, if take_true { 1 } else { 2 })?;
                    let branches = if context.scope.is_pattern() { &self.pattern_branches } else { &self.processor_branches };
                    let (shared, after_selected) = if let Some(inputs) = branches.get(&index) {
                        (inputs.skipped(take_true), take_true)
                    } else if context.scope == super::FunctionScope::PixelProcessor {
                        (self.instruction(index)?.control_registers.as_slice(), false)
                    } else {
                        (&[][..], false)
                    };
                    let selected = || if prefix { Prefix(selected) } else { Enter(selected) };
                    if !after_selected {
                        state.steps.push(selected());
                    }
                    for &index in shared.iter().rev() {
                        if prefix {
                            state.steps.push(Prefix(index));
                        } else if state.registers.get(index).is_some_and(Option::is_none) {
                            state.steps.push(Discard);
                            state.steps.push(Enter(index));
                        }
                    }
                    if after_selected {
                        state.steps.push(selected());
                    }
                }
                Discard => {
                    state.values.pop().ok_or("missing discarded function value")?;
                }
                Sequence(index) => {
                    state.values.pop().ok_or("missing sequence result")?;
                    state.steps.push(Enter(self.input(index, 1)?));
                }
                LoopStart(index) => {
                    let initializer = state.values.pop().ok_or("missing loop initialization")?;
                    // The initializer may assign a different type. A skipped body has its own typed zero.
                    let last = materialcraft_core::ValueType::from_code(self.instruction(index)?.type_code)
                        .map_or(initializer, materialcraft_core::ValueType::default_value);
                    self.clear_loop_results(index, state)?;
                    state.steps.push(LoopCondition { instruction: index, iteration: 0, last, checkpoint: None });
                    state.steps.push(Enter(self.input(index, 1)?));
                }
                LoopCondition { instruction: index, mut iteration, mut last, mut checkpoint } => {
                    let exit = value_type::boolean(&state.values.pop().ok_or("missing loop condition")?)?;
                    let maximum = self.instruction(index)?.value.as_ref().and_then(Value::as_i32).unwrap_or(super::DEFAULT_LOOP_LIMIT);
                    if !exit
                        && maximum > 0
                        && let Some(previous) = checkpoint.as_ref()
                    {
                        let advance = previous.advance(state, &mut last, (maximum as u32).saturating_sub(iteration), remaining_steps);
                        iteration += advance;
                        if advance > 0 || !previous.tracks_sum() {
                            checkpoint = None;
                        }
                    }
                    if exit || (maximum >= 0 && iteration >= maximum as u32) {
                        state.values.push(last);
                    } else {
                        // Check consecutive visits for fixed points. A sum can retain a short sequence
                        // of increments until the rest of the loop state repeats.
                        if maximum > 0 && (maximum as u32).saturating_sub(iteration) > 1 && iteration.is_power_of_two() {
                            checkpoint = super::evaluation_state::LoopCheckpoint::capture(state, self.loop_sums.get(&index));
                        }
                        self.clear_loop_results(index, state)?;
                        state.steps.push(LoopBody { instruction: index, iteration: iteration.saturating_add(1), checkpoint });
                        state.steps.push(Enter(self.input(index, 2)?));
                    }
                }
                LoopBody { instruction: index, iteration, mut checkpoint } => {
                    let last = state.values.pop().ok_or("missing loop body result")?;
                    if checkpoint.as_mut().is_some_and(|previous| !previous.record_body(state, &last)) {
                        checkpoint = None;
                    }
                    self.clear_loop_results(index, state)?;
                    state.steps.push(LoopCondition { instruction: index, iteration, last, checkpoint });
                    state.steps.push(Enter(self.input(index, 1)?));
                }
            }
        }

        let result = state.values.pop().ok_or("function returned no value")?;
        let expected = self.instruction(self.output)?.type_code;
        if value_type::type_code(&result) != expected {
            return Err(format!("function returned {}, expected native type {expected}", result.type_name()));
        }

        Ok(result)
    }

    fn clear_loop_results(&self, index: usize, state: &mut EvaluationState) -> Result<(), String> {
        // Outgoing wires share a node value across reads, branches and effects
        // Only the active loop condition and body run again
        for repeated in &self.instruction(index)?.control_registers {
            if let Some(register) = state.registers.get_mut(*repeated) {
                *register = None;
            }
        }

        Ok(())
    }
}
