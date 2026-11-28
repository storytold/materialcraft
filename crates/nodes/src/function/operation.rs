use super::{
    EvaluationContext,
    function_call::CallBindings,
    instruction::Instruction,
    value_type::{self, boolean, dimensions, floats, integers},
};
use materialcraft_core::Value;
use std::collections::BTreeMap;

pub(super) fn evaluate(
    instruction: &Instruction,
    arguments: &[Value],
    variables: &mut BTreeMap<String, Value>,
    random: &mut crate::random::FunctionRandom,
    context: &EvaluationContext<'_>,
    bindings: Option<(&CallBindings<'_>, &BTreeMap<String, Value>)>,
) -> Result<Value, String> {
    let op = instruction.definition.id;
    let first = || arguments.first().ok_or("missing first operand");
    let second = || arguments.get(1).ok_or("missing second operand");
    let name = || match &instruction.value {
        Some(Value::Text(name)) => Ok(name.as_str()),
        _ => Err("variable name must be a string".to_string()),
    };

    if op.starts_with("const_") {
        let value = instruction.value.clone().ok_or("constant has no value")?;
        if value_type::type_code(&value) != instruction.type_code {
            return Err("constant has the wrong type".into());
        }

        return value_type::canonical(value);
    }

    if op.starts_with("get_") {
        let name = name()?;
        let missing = || format!("undefined variable `{name}` in this context");
        let value = match context.variable(name, variables)? {
            Some(value) => value,
            // A pattern's traversal seed selects its stream. The integer getter has no binding for it.
            None if context.scope.is_pattern() && name == "$randomseed" && op == "get_integer1" => Value::Int(0),
            // Unbound integer names in node properties read as zero, including reserved spellings.
            // Known system variables still enforce their own scope and type.
            None if matches!(context.scope, super::FunctionScope::Node | super::FunctionScope::RandomSeed)
                && op == "get_integer1"
                && super::SystemVariable::named(name).is_none() =>
            {
                Value::Int(0)
            }
            // Unset numeric variables start at typed zero in a value processor.
            None if !name.starts_with('$') || context.scope == super::FunctionScope::ValueProcessor => {
                materialcraft_core::ValueType::from_code(instruction.type_code)
                    .filter(|kind| *kind != materialcraft_core::ValueType::String)
                    .map(materialcraft_core::ValueType::default_value)
                    .ok_or_else(missing)?
            }
            None => return Err(missing()),
        };

        // Older authored graphs read an exposed integer option as a Boolean
        // Keep system-variable type checks strict; user options use zero/nonzero
        if op == "get_bool"
            && !name.starts_with('$')
            && matches!(value, Value::Int(_) | Value::Enum(_))
            && let Some(integer) = value.as_i32()
        {
            return Ok(Value::Bool(integer != 0));
        }

        if value_type::type_code(&value) != instruction.type_code {
            return Err(format!("variable `{name}` has the wrong type for {op}"));
        }

        return value_type::canonical(value);
    }

    if context.scope.uses_seed_words() && !instruction.literal {
        // Runtime scalar seed sums add the storage words. A float-to-integer cast retains the floored float's bits.
        match (op, arguments) {
            ("add", [Value::Int(left), Value::Int(right)]) => return Ok(Value::Int(left.wrapping_add(*right))),
            ("add", [Value::Float(left), Value::Float(right)]) => {
                let sum = f32::from_bits(left.to_bits().wrapping_add(right.to_bits()));
                return Ok(Value::Float(super::arithmetic::flush_subnormal(sum)));
            }
            ("toint1", [Value::Float(value)]) => {
                // Rounding a signaling NaN quiets it and preserves the seed payload.
                let word = if value.is_nan() { value.to_bits() | 0x0040_0000 } else { value.floor().to_bits() };
                return Ok(Value::Int(word as i32));
            }
            _ => {}
        }
    }

    // A folded scalar cast interprets the 32-bit word as unsigned. Vector casts use signed lanes.
    if op == "tofloat"
        && instruction.literal
        && let Value::Int(value) = first()?
    {
        return Ok(Value::Float(*value as u32 as f32));
    }
    if op.starts_with("to") || op.contains("vector") || op.contains("swizzle") {
        return super::vector::evaluate(op, arguments, instruction.value.as_ref());
    }

    Ok(match op {
        "passthrough" => first()?.clone(),
        "set" => {
            let name = name()?;
            if name.is_empty() || name.starts_with('$') {
                return Err("use a nonempty user variable name".into());
            }

            let value = first()?.clone();
            variables.insert(name.to_string(), value.clone());
            value
        }
        "isdefined" => {
            let name = name()?;
            let defined = match bindings {
                Some((call, assignments)) if super::SystemVariable::named(name).is_none() => call.is_defined(name, assignments),
                _ => context.variable(name, variables)?.is_some(),
            };
            Value::Bool(defined)
        }
        "and" => Value::Bool(boolean(first()?)? && boolean(second()?)?),
        "or" => Value::Bool(boolean(first()?)? || boolean(second()?)?),
        "not" => Value::Bool(!boolean(first()?)?),
        "eq" | "noteq" => {
            let (left, right) = (first()?, second()?);
            if value_type::type_code(left) != value_type::type_code(right) {
                return Err("comparison inputs must have the same type".into());
            }

            Value::Bool((left == right) == (op == "eq"))
        }
        "gt" | "gteq" | "lr" | "lreq" => {
            let (left, right) = (first()?, second()?);
            if dimensions(left) != 1 || value_type::type_code(left) != value_type::type_code(right) {
                return Err("comparison requires matching scalars".into());
            }

            let ordering = match (left, right) {
                (Value::Int(a), Value::Int(b)) if instruction.literal => Some((*a as u32).cmp(&(*b as u32))),
                (Value::Int(a), Value::Int(b)) => a.partial_cmp(b),
                (Value::Float(a), Value::Float(b)) => a.partial_cmp(b),
                (Value::Bool(a), Value::Bool(b)) => a.partial_cmp(b),
                _ => return Err("comparison requires numeric inputs".into()),
            };

            // Ordered IEEE comparisons are false for NaN, including <= and >=
            // Undefined intermediates can therefore select a finite fallback
            Value::Bool(ordering.is_some_and(|ordering| match op {
                "gt" => ordering.is_gt(),
                "gteq" => ordering.is_ge(),
                "lr" => ordering.is_lt(),
                _ => ordering.is_le(),
            }))
        }
        "samplecol" | "samplelum" => {
            let position = floats(first()?)?;
            if dimensions(first()?) != 2 {
                return Err("sample position must be float2".into());
            }

            let setting = instruction.value.as_ref().map(integers).transpose()?.unwrap_or([0, 0, 0, 0]);
            let index = usize::try_from(setting[0]).map_err(|_| "negative image index")?;
            let (bilinear, repeat) = match (context.scope, setting[1]) {
                (super::FunctionScope::Node | super::FunctionScope::RandomSeed, mode) => (mode != 0, context.tile),
                (_, 2) => (false, false),
                (_, 3) => (true, false),
                (_, mode) => (mode != 0, true),
            };
            let image = context.images.get(index).and_then(Option::as_ref);
            let uniform_sampler = context.scope == super::FunctionScope::ValueProcessor || context.scope.is_pattern();
            let pixel = match image {
                Some(image) if uniform_sampler => {
                    let format = context.image_formats.get(index).copied().unwrap_or(materialcraft_core::OutputFormat::F32);
                    crate::texture_sampler::sample_uniform(image, [position[0], position[1]], format, bilinear, repeat)
                }
                Some(image) if context.scope == super::FunctionScope::PixelProcessor => {
                    let format = context.image_formats.get(index).copied().unwrap_or(materialcraft_core::OutputFormat::F32);
                    crate::texture_sampler::sample_pixel(image, [position[0], position[1]], format, bilinear, repeat)
                }
                Some(image) if !bilinear => {
                    image.get((position[0] * image.width() as f32).floor() as i64, (position[1] * image.height() as f32).floor() as i64, repeat)
                }
                Some(image) => image.sample_stored(position[0], position[1], context.tile),
                None => [0.0; 4],
            };

            if op == "samplecol" {
                Value::Float4(pixel)
            } else if (uniform_sampler || context.scope == super::FunctionScope::PixelProcessor)
                && image.is_some_and(|texture| texture.kind() == craft_texture::Kind::Color)
            {
                Value::Float(((pixel[1] + pixel[2]) + pixel[0]) * (1.0 / 3.0))
            } else {
                Value::Float(pixel[0])
            }
        }
        _ => return super::arithmetic::evaluate(instruction, arguments, random, context.scope),
    })
}
