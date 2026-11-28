use super::FunctionDefinition;
use materialcraft_core::Value;

pub(crate) struct Instruction {
    pub id: u32,
    pub definition: &'static FunctionDefinition,
    pub inputs: Vec<usize>,
    pub value: Option<Value>,
    pub type_code: u32,
    /// Pure expression with only literal inputs. Its arithmetic rounds before pixel evaluation.
    pub literal: bool,
    /// A pattern condition also folds when its inputs come from unwired function arguments.
    pub(super) pattern_literal: bool,
    /// A constant denominator folds to its rounded reciprocal even with a varying numerator.
    pub(super) literal_divisor: bool,
    /// Original scalar divisor retained when a pixel expression divides by its reciprocal.
    pub(super) reciprocal_factor: Option<super::reciprocal::Factor>,
    pub(super) pixel_product_sum: Option<super::product_sum::ProductSum>,
    /// Registers for max(lower, min(value, upper)) with literal scalar bounds.
    pub(super) pixel_interval: Option<Box<PixelInterval>>,
    /// A loop refreshes its condition and body. A pixel branch first evaluates its shared effects.
    pub control_registers: Vec<usize>,
    pub call: Option<super::function_call::FunctionCall>,
}

impl Instruction {
    pub(super) fn new(id: u32, definition: &'static FunctionDefinition, inputs: Vec<usize>, value: Option<Value>, type_code: u32) -> Self {
        Self {
            id,
            definition,
            inputs,
            value,
            type_code,
            literal: false,
            pattern_literal: false,
            literal_divisor: false,
            reciprocal_factor: None,
            pixel_product_sum: None,
            pixel_interval: None,
            control_registers: Vec::new(),
            call: None,
        }
    }

    pub(super) fn depends_only_on_operands(&self) -> bool {
        self.call.is_none() && !self.inputs.is_empty() && !matches!(self.definition.id, "rand" | "samplelum" | "samplecol" | "set" | "while")
    }

    pub(super) fn product_operation(&self) -> Option<&'static str> {
        // A cancelled reciprocal keeps the original operands live for their other readers.
        // The retained-product pass can use its factor after both operands have run.
        if self.reciprocal_factor.is_some() {
            return None;
        }
        let [_, _] = self.inputs.as_slice() else { return None };
        match self.definition.id {
            "rand" => Some("mul"),
            op @ ("mul" | "mulscalar" | "div") => Some(op),
            _ => None,
        }
    }
}

pub(super) struct PixelInterval {
    pub input: usize,
    pub lower: usize,
    pub upper: usize,
    pub input_is_literal: bool,
}

impl PixelInterval {
    pub fn saturate(&self, state: &super::EvaluationState) -> Option<Value> {
        if state.scalar_register(self.lower)? != 0.0 {
            return None;
        }
        let input = state.scalar_register(self.input)?;
        let upper = state.scalar_register(self.upper)?;
        let value = if upper == 1.0 {
            input
        } else if self.input_is_literal && input == 1.0 {
            upper
        } else {
            return None;
        };
        // Saturation maps NaN and both signed zeros to positive zero.
        Some(Value::Float(if value > 0.0 { value.min(1.0) } else { 0.0 }))
    }
}
