use super::{Program, catalog, instruction::Instruction};
use materialcraft_core::{FunctionGraph, ValueType};
use std::collections::{BTreeMap, BTreeSet};

pub fn compile(graph: &FunctionGraph, seed: u32) -> Result<Program, String> {
    compile_with_literal_inputs(graph, seed, &[])
}

/// Compile with the named inputs fixed by an enclosing graph instance.
pub fn compile_with_literal_inputs(graph: &FunctionGraph, seed: u32, literal_inputs: &[String]) -> Result<Program, String> {
    compile_program(graph, seed, literal_inputs, None)
}

pub(super) fn compile_program(
    graph: &FunctionGraph,
    seed: u32,
    literal_inputs: &[String],
    pixel: Option<&super::EvaluationContext<'_>>,
) -> Result<Program, String> {
    let mut remaining = super::MAX_EXPANDED_NODES;
    let mut program = compile_nested(graph, seed, 0, &mut remaining)?;
    mark_literals(&mut program, literal_inputs.iter().map(|name| (name.as_str(), Constant::Literal)).collect());
    super::reciprocal::identify_products(&mut program, BTreeMap::new());
    plan_pixel_operations(&mut program, pixel, true);
    super::assigned_product::identify(&mut program);
    Ok(program)
}

fn compile_nested(graph: &FunctionGraph, seed: u32, depth: usize, remaining: &mut usize) -> Result<Program, String> {
    if graph.nodes.len() > 4096 {
        return Err("function exceeds the 4096-node graph limit".into());
    }

    if depth > super::MAX_CALL_DEPTH || graph.nodes.len() > *remaining {
        return Err(format!("function exceeds the {}-level or {}-node expanded execution limit", super::MAX_CALL_DEPTH, super::MAX_EXPANDED_NODES));
    }

    *remaining -= graph.nodes.len();
    let output = super::output_node(graph).ok_or("function has no result node")?.id;
    let mut nodes = BTreeMap::new();
    for node in &graph.nodes {
        if nodes.insert(node.id, node).is_some() {
            return Err(format!("duplicate function node {}", node.id));
        }
    }

    let mut pending = vec![(output, false)];
    let mut active = BTreeSet::new();
    let mut indices = BTreeMap::new();
    let mut instructions: Vec<Instruction> = Vec::new();
    while let Some((id, visited)) = pending.pop() {
        if indices.contains_key(&id) {
            continue;
        }

        let node = nodes.get(&id).ok_or_else(|| format!("missing function node {id}"))?;
        if node.missing.is_some() {
            return Err(format!("Missing function operation `{}`", node.op));
        }

        let definition = if node.instance.is_some() {
            &super::function_call::DEFINITION
        } else {
            catalog::definition(&node.op).ok_or_else(|| format!("unknown function `{}`", node.op))?
        };
        let names: Vec<_> = super::input_identifiers(node).collect();

        // Operations use zero for absent inputs; calls require source defaults
        let missing_input = node
            .instance
            .as_ref()
            .is_some_and(|instance| names.iter().zip(&node.inputs).any(|(name, input)| input.is_none() && !instance.defaults.contains_key(*name)));
        if node.inputs.len() != names.len() || missing_input {
            return Err(format!("{} ({id}) requires connected inputs: {}", super::node_label(node), names.join(", ")));
        }

        // Legacy connectors must not evaluate stateful inputs twice
        let ports: Vec<_> = names
            .into_iter()
            .zip(&node.inputs)
            .filter(|(name, _)| node.instance.as_ref().is_none_or(|instance| instance.defaults.contains_key(*name)))
            .collect();
        let sources: Vec<_> = ports.iter().filter_map(|(_, input)| **input).collect();
        if !visited {
            if !active.insert(id) {
                return Err(format!("function cycle at node {id}"));
            }

            pending.push((id, true));
            pending.extend(sources.iter().rev().map(|input| (*input, false)));
            continue;
        }

        active.remove(&id);
        let connected: Vec<usize> =
            sources.iter().map(|input| indices.get(input).copied().ok_or_else(|| "unresolved function input".to_string())).collect::<Result<_, _>>()?;
        let inputs = if node.instance.is_some() || ports.iter().all(|(_, input)| input.is_some()) {
            connected
        } else {
            let sibling = if node.op == "ifelse" {
                // Absent value branches must use a value-port type
                ports.iter().skip(1).find_map(|(_, input)| {
                    input.and_then(|id| indices.get(&id)).and_then(|index| instructions.get(*index)).map(|instruction| instruction.type_code)
                })
            } else {
                connected.first().and_then(|index| instructions.get(*index)).map(|instruction| instruction.type_code)
            };
            let mut connected = connected.into_iter();
            let mut inputs = Vec::with_capacity(ports.len());
            for (slot, (name, input)) in ports.iter().enumerate() {
                let index = match input {
                    Some(_) => connected.next().ok_or("unresolved function input")?,
                    None => {
                        let sibling_type_code = ports
                            .get(if slot == 0 { 1 } else { 0 })
                            .and_then(|(_, input)| **input)
                            .and_then(|id| indices.get(&id))
                            .and_then(|index| instructions.get(*index))
                            .map(|instruction| instruction.type_code);
                        let kind = super::unconnected_input::value_type(&node.op, name, sibling.unwrap_or(node.type_code), sibling_type_code);
                        let (definition, value) = unconnected(kind)?;
                        let type_code = super::value_type::type_code(&value);
                        instructions.push(Instruction {
                            id: 0,
                            definition,
                            inputs: Vec::new(),
                            value: Some(value),
                            type_code,
                            literal: true,
                            pattern_literal: true,
                            literal_divisor: false,
                            reciprocal_factor: None,
                            pixel_product_sum: None,
                            pixel_interval: None,
                            control_registers: Vec::new(),
                            call: None,
                        });
                        instructions.len() - 1
                    }
                };
                inputs.push(index);
            }

            if matches!(node.op.as_str(), "vector2" | "vector3" | "vector4" | "ivector2" | "ivector3" | "ivector4")
                && ports.first().is_some_and(|(_, input)| input.is_none())
            {
                // Connected components occupy the leading lanes. The inferred zero operand pads the end.
                inputs.reverse();
            }
            inputs
        };

        let call = if let Some(instance) = &node.instance {
            let source = node.source.as_ref().ok_or_else(|| format!("unresolved Function Graph `{}`", instance.graph))?.as_ref().map_err(Clone::clone)?;
            let program = compile_nested(source, seed, depth + 1, remaining)?;
            for (port, index) in instance
                .inputs
                .iter()
                .zip(&node.inputs)
                .filter(|(port, input)| input.is_some() && instance.defaults.contains_key(&port.identifier))
                .map(|(port, _)| port)
                .zip(&inputs)
            {
                if instructions.get(*index).is_some_and(|instruction| instruction.type_code != port.type_code) {
                    return Err(format!("{}: input `{}` has the wrong type", instance.graph, port.identifier));
                }
            }
            let names: Vec<_> = ports.iter().filter(|(_, input)| input.is_some()).map(|(name, _)| (*name).to_string()).collect();
            let mut defaults = instance.defaults.clone();
            if !names.is_empty() {
                // Once a call supplies arguments, an omitted numeric argument reads its typed zero.
                for (name, input) in &ports {
                    if input.is_none()
                        && let Some(value) = defaults.get_mut(*name)
                        && let Some(kind) = ValueType::of(value)
                        && kind != ValueType::String
                    {
                        *value = kind.default_value();
                    }
                }
            }
            let exports =
                program.instructions.iter().any(|instruction| instruction.definition.id == "set" || instruction.call.as_ref().is_some_and(|call| call.exports));
            let pattern_arguments = super::function_call::PatternArguments::identify(&program, &names, &defaults, &inputs);
            Some(super::function_call::FunctionCall {
                pattern_access: super::function_call::PatternAccess::identify(&program, &defaults),
                program,
                inputs: names,
                defaults,
                argument_factors: None,
                reciprocals: None,
                assigned_products: Vec::new(),
                pattern_arguments,
                exports,
            })
        } else {
            None
        };

        let mut value = node.value.clone().or_else(|| catalog::default_value(&node.op));
        // Getters consume one reference marker. Set and Is Defined address the literal name.
        if definition.id.starts_with("get_")
            && let Some(materialcraft_core::Value::Text(name)) = &mut value
            && let Some(identifier) = name.strip_prefix('%')
        {
            *name = identifier.to_owned();
        }
        let input_type = |index: usize| inputs.get(index).and_then(|index| instructions.get(*index)).map_or(256, |instruction| instruction.type_code);
        let inferred = super::inferred_type::inferred_type(&node.op, value.as_ref(), input_type);
        let type_code = call.as_ref().map_or_else(|| if node.type_code == 0 { inferred } else { node.type_code }, |call| call.program.output_type());
        let operation = |inputs, value, call| Instruction { call, ..Instruction::new(id, definition, inputs, value, type_code) };
        let mut inputs = inputs;
        if definition.id == "rand"
            && type_code == 256
            && let [range] = inputs.as_slice()
            && instructions.get(*range).is_some_and(|r| r.type_code == 256 && r.value != Some(materialcraft_core::Value::Float(1.0)))
        {
            // Keep the unit sample in a register. A packed or shared range can then reuse its
            // unrounded factors without drawing again, including on each loop iteration.
            inputs.push(instructions.len());
            instructions.push(operation(Vec::new(), None, None));
        }
        indices.insert(id, instructions.len());
        instructions.push(operation(inputs, value, call));
    }

    // Instructions come after their inputs, so walking back from a loop reaches its whole condition and body
    for index in 0..instructions.len() {
        if instructions.get(index).is_some_and(|instruction| instruction.definition.id == "while") {
            let repeated = loop_members(&instructions, index);
            if let Some(instruction) = instructions.get_mut(index) {
                instruction.control_registers = repeated;
            }
        }
    }

    let output = indices.get(&output).copied().ok_or("missing result instruction")?;

    let sequence_prefixes = super::sequence_prefix::identify(&instructions, output);
    Ok(Program {
        instructions,
        output,
        seed,
        sequence_prefixes,
        processor_branches: BTreeMap::new(),
        pattern_branches: BTreeMap::new(),
        pattern_components: BTreeMap::new(),
        loop_sums: BTreeMap::new(),
    })
}

/// A write can replace an argument before its next read.
pub(super) fn remove_written_arguments<T>(program: &Program, arguments: &mut BTreeMap<&str, T>) {
    for op in &program.instructions {
        if arguments.is_empty() {
            break;
        }
        if let Some(call) = &op.call
            && call.exports
        {
            // A callee's unrelated assignments leave the caller's known operands available.
            remove_written_arguments(&call.program, arguments);
        }
        if op.definition.id == "set"
            && let Some(materialcraft_core::Value::Text(name)) = &op.value
        {
            arguments.remove(name.as_str());
        }
    }
}

// A pure expression is fixed only as far as its least constant operand.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Constant {
    Varying,
    PatternDefault,
    Literal,
}

fn mark_literals(program: &mut Program, mut arguments: BTreeMap<&str, Constant>) -> Constant {
    remove_written_arguments(program, &mut arguments);
    let mut literals = Vec::with_capacity(program.instructions.len());
    for op in &mut program.instructions {
        let source = |index: &usize| literals.get(*index).copied().unwrap_or(Constant::Varying);
        let id = op.definition.id;
        op.literal_divisor = id == "div" && op.inputs.get(1).is_some_and(|i| source(i) == Constant::Literal);
        let constant = if let Some(call) = &mut op.call {
            let mut bound: BTreeMap<_, _> = call.defaults.keys().map(|name| (name.as_str(), Constant::PatternDefault)).collect();
            for (name, input) in call.inputs.iter().zip(&op.inputs) {
                bound.insert(name.as_str(), source(input));
            }
            mark_literals(&mut call.program, bound)
        } else if id.starts_with("const_") {
            Constant::Literal
        } else if id.starts_with("get_")
            && let Some(materialcraft_core::Value::Text(name)) = &op.value
        {
            arguments.get(name.as_str()).copied().unwrap_or(Constant::Varying)
        } else if op.depends_only_on_operands() {
            op.inputs.iter().map(source).min().unwrap_or(Constant::Varying)
        } else {
            Constant::Varying
        };
        op.literal = constant == Constant::Literal;
        op.pattern_literal = constant != Constant::Varying;
        literals.push(constant);
    }
    literals.get(program.output).copied().unwrap_or(Constant::Varying)
}

fn plan_pixel_operations(program: &mut Program, pixel: Option<&super::EvaluationContext<'_>>, root: bool) -> super::returned_product::Lanes {
    let mut returned = BTreeMap::new();
    for index in 0..program.instructions.len() {
        let (previous, current) = program.instructions.split_at_mut(index);
        if let Some(op) = current.first_mut() {
            if let Some(call) = &mut op.call {
                let lanes = plan_pixel_operations(&mut call.program, None, false);
                if lanes.iter().any(Option::is_some) {
                    returned.insert(index, lanes);
                }
            }
            op.pixel_interval = pixel_interval(op, previous).map(Box::new);
        }
    }
    let readers = pixel.and_then(|context| super::pixel_condition::live_readers(program, context));
    super::product_sum::identify(&mut program.instructions, readers.as_deref());
    super::branch_assignments::identify(program);
    // Only pixel consumers can force a product to round before a later sum. The pattern
    // channel pass appends its own reads, which the pixel evaluator never visits.
    let exported = super::returned_product::identify(program, returned, root);
    let original_len = program.instructions.len();
    let pattern_end = super::sequence_prefix::pattern_planning_end(program);
    super::vector::identify_pattern_components(program, pattern_end, root);
    super::branch_inputs::identify(program, pattern_end..original_len);
    program.loop_sums =
        program.instructions.iter().enumerate().filter_map(|(index, op)| super::loop_sum::LoopSum::identify(program, op).map(|sum| (index, sum))).collect();
    exported
}

fn pixel_interval(op: &Instruction, previous: &[Instruction]) -> Option<super::instruction::PixelInterval> {
    if op.definition.id != "max" || op.type_code != 256 {
        return None;
    }
    let &[a, b] = op.inputs.as_slice() else { return None };
    for (lower, inner) in [(a, b), (b, a)] {
        let minimum = previous.get(inner)?;
        if !previous.get(lower)?.literal || minimum.definition.id != "min" {
            continue;
        }
        let &[x, y] = minimum.inputs.as_slice() else { continue };
        for (input, upper) in [(x, y), (y, x)] {
            if previous.get(upper)?.literal {
                return Some(super::instruction::PixelInterval { input, lower, upper, input_is_literal: previous.get(input)?.literal });
            }
        }
    }
    None
}

/// The instructions the condition and the body of loop `index` evaluate again on every iteration
fn loop_members(instructions: &[Instruction], index: usize) -> Vec<usize> {
    let mut member = vec![false; instructions.len()];
    let mut pending: Vec<usize> = instructions.get(index).map(|instruction| instruction.inputs.iter().skip(1).copied().collect()).unwrap_or_default();
    while let Some(next) = pending.pop() {
        if let Some(seen) = member.get_mut(next)
            && !*seen
        {
            *seen = true;
            pending.extend(instructions.get(next).map(|instruction| instruction.inputs.as_slice()).unwrap_or_default());
        }
    }

    member.iter().enumerate().filter(|(_, member)| **member).map(|(index, _)| index).collect()
}

/// The constant an unconnected input reads: false or zero of the operand type
fn unconnected(value_type: materialcraft_core::ValueType) -> Result<(&'static super::FunctionDefinition, materialcraft_core::Value), String> {
    use materialcraft_core::ValueType;

    let operation = match value_type {
        ValueType::Boolean => "const_bool",
        ValueType::Integer => "const_int1",
        ValueType::Integer2 => "const_int2",
        ValueType::Integer3 => "const_int3",
        ValueType::Integer4 => "const_int4",
        ValueType::Float => "const_float1",
        ValueType::Float2 => "const_float2",
        ValueType::Float3 => "const_float3",
        ValueType::Float4 => "const_float4",
        ValueType::String => "const_string",
    };
    let definition = catalog::definition(operation).ok_or("missing constant operation")?;

    Ok((definition, value_type.default_value()))
}
