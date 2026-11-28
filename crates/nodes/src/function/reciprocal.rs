//! Fixed-point reciprocal used by runtime pixel division.

use super::{EvaluationState, Program, fixed_point::square_fraction, function_call::FunctionCall, returned_product::forwarded, value_type};
use materialcraft_core::Value;
use std::collections::BTreeMap;

/// A scalar before its reciprocal rounded, in the frame which evaluated it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Factor {
    Register(usize),
    Argument(usize),
    Returned { call: usize, lane: u8 },
}

impl Factor {
    pub(super) fn read(self, state: &EvaluationState) -> Option<f32> {
        match self {
            Self::Register(i) => state.scalar_register(i),
            Self::Argument(i) => state.reciprocal_arguments.get(i).copied().flatten(),
            Self::Returned { call, lane } => state.calls.get(&call)?.returned_reciprocals.get(usize::from(lane)).copied().flatten(),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Origin {
    One,
    Reciprocal(Factor),
}

type Lanes = [Option<Origin>; 4];

pub(super) struct CallFactors {
    pub arguments: Vec<Factor>,
    pub results: [Option<Factor>; 4],
}

fn call_origins(call: &mut FunctionCall, index: usize, inputs: &[usize], origins: &[Lanes]) -> Lanes {
    let mut factors = CallFactors { arguments: Vec::new(), results: [None; 4] };
    let mut bound = BTreeMap::new();
    for (name, register) in call.inputs.iter().zip(inputs) {
        if let Some(lanes) = origins.get(*register)
            && lanes.iter().any(Option::is_some)
        {
            bound.insert(
                name.as_str(),
                lanes.map(|lane| match lane {
                    Some(Origin::Reciprocal(factor)) => {
                        let slot = factors.arguments.len();
                        factors.arguments.push(factor);
                        Some(Origin::Reciprocal(Factor::Argument(slot)))
                    }
                    other => other,
                }),
            );
        }
    }
    let mut result = identify_products(&mut call.program, bound);
    for (lane, origin) in result.iter_mut().enumerate() {
        if let Some(Origin::Reciprocal(factor)) = origin {
            factors.results[lane] = Some(*factor);
            *factor = Factor::Returned { call: index, lane: lane as u8 };
        }
    }
    if !factors.arguments.is_empty() || factors.results.iter().any(Option::is_some) {
        call.reciprocals = Some(Box::new(factors));
    }
    result
}

/// Track the literal unit numerator and the original divisor through vector lanes and calls.
/// The caller still evaluates both operands, including assignments and random draws.
pub(super) fn identify_products(program: &mut Program, mut arguments: BTreeMap<&str, Lanes>) -> Lanes {
    super::compile::remove_written_arguments(program, &mut arguments);
    let instructions = &mut program.instructions;
    if instructions.iter().any(|op| op.definition.id == "while") {
        // A loop can clear the original divisor while another register still holds its reciprocal.
        for call in instructions.iter_mut().filter_map(|op| op.call.as_mut()) {
            identify_products(&mut call.program, BTreeMap::new());
        }
        return [None; 4];
    }
    let mut origins: Vec<Lanes> = Vec::with_capacity(instructions.len());
    for index in 0..instructions.len() {
        let (previous, current) = instructions.split_at_mut(index);
        let op = &mut current[0];
        if let Some(call) = &mut op.call {
            let result = call_origins(call, index, &op.inputs, &origins);
            origins.push(result);
            continue;
        }
        let lane = |register: usize, component: usize| origins.get(register).and_then(|v| v.get(component)).copied().flatten();
        let mut result = [None; 4];
        match (op.definition.id, op.inputs.as_slice()) {
            ("get_float1" | "get_float2" | "get_float3" | "get_float4", []) => {
                if let Some(Value::Text(name)) = &op.value
                    && !name.starts_with('$')
                    && let Some(lanes) = arguments.get(name.as_str())
                {
                    result = *lanes;
                }
            }
            ("const_float1" | "const_float2" | "const_float3" | "const_float4", []) => {
                if let Some(value) = op.value.as_ref()
                    && let Ok(components) = value_type::floats(value)
                {
                    for (slot, component) in result.iter_mut().zip(components).take(value_type::dimensions(value)) {
                        *slot = (component == 1.0).then_some(Origin::One);
                    }
                }
            }
            ("div", &[a, b]) if op.type_code == 256 && !op.literal => {
                if let Some(Origin::Reciprocal(factor)) = lane(b, 0) {
                    op.reciprocal_factor = Some(factor);
                }
                if matches!(lane(a, 0), Some(Origin::One)) {
                    result[0] = Some(Origin::Reciprocal(Factor::Register(b)));
                }
            }
            ("mul" | "mulscalar", &[a, b]) => {
                for (i, slot) in result.iter_mut().enumerate().take(super::returned_product::float_width(op.type_code)) {
                    let left = lane(a, i);
                    let right = lane(b, if op.definition.id == "mulscalar" { 0 } else { i });
                    *slot = match (left, right) {
                        (Some(Origin::One), other) | (other, Some(Origin::One)) => other,
                        _ => None,
                    };
                }
            }
            ("passthrough", &[input]) => result = origins.get(input).copied().unwrap_or(result),
            _ => result = forwarded(op, previous, &origins, None).unwrap_or(result),
        }
        origins.push(result);
    }
    origins.get(program.output).copied().unwrap_or([None; 4])
}

pub(super) fn multiply_original(numerator: Option<&Value>, factor: Option<f32>) -> Option<Value> {
    let Value::Float(numerator) = numerator? else { return None };
    let flush = super::arithmetic::flush_subnormal;
    Some(Value::Float(flush(flush(*numerator) * flush(factor?))))
}

#[derive(Clone, Copy)]
struct Polynomial {
    quarter_constant: i32,
    half_slope: i32,
    curvature: i32,
}

const fn nearest(value: f64) -> i32 {
    (value + if value < 0.0 { -0.5 } else { 0.5 }) as i32
}

// Derive the coefficients from 1/x. The normalized mantissa spans 128 equal intervals.
// In units of the output significand, the function on interval i is 2^31 / (128+i+t).
const fn polynomials() -> [Polynomial; 128] {
    const SCALE: f64 = 2_147_483_648.0;
    let mut result = [Polynomial { quarter_constant: 0, half_slope: 0, curvature: 0 }; 128];
    let mut i = 0;
    while i < result.len() {
        let start = 128.0 + i as f64;
        let mid = start + 0.5;
        let mid_squared = mid * mid;
        // On [-1/2,1/2], the linear minimax approximation of t³ is 3t/16.
        let center_slope = -SCALE / mid_squared - (3.0 / 16.0) * SCALE / (mid_squared * mid_squared);
        let half_slope = nearest((center_slope - SCALE / (mid_squared * mid)) * 0.5);
        let slope = 2 * half_slope;
        let curvature = nearest(center_slope) - slope;

        // Center the quarter-unit constant between the extrema of the residual.
        let mut low = SCALE;
        let mut high = 0.0;
        let mut step = 0;
        while step <= 64 {
            let t = step as f64 / 64.0;
            let residual = SCALE / (start + t) - slope as f64 * t - curvature as f64 * t * t;
            if residual < low {
                low = residual;
            }
            if residual > high {
                high = residual;
            }
            step += 1;
        }
        result[i] = Polynomial { quarter_constant: nearest(2.0 * (low + high)), half_slope, curvature };
        i += 1;
    }
    result
}

const POLYNOMIALS: [Polynomial; 128] = polynomials();

fn factor(denominator: f32, literal_divisor: bool) -> Option<f32> {
    let bits = denominator.to_bits();
    let exponent = (bits >> 23) & 255;
    if literal_divisor || !(1..=252).contains(&exponent) {
        // At the limits, an intermediate reciprocal can overflow or underflow a finite quotient.
        let reciprocal = denominator.recip();
        return reciprocal.is_normal().then_some(reciprocal);
    }
    let bin = POLYNOMIALS.get(((bits >> 16) & 127) as usize)?;
    let fraction = bits & 0xffff;
    let polynomial = (i64::from(bin.quarter_constant) << 13)
        + i64::from(bin.half_slope) * i64::from(fraction)
        + i64::from(bin.curvature) * i64::from(square_fraction(fraction));
    // The observed rounding threshold includes a bias of 2023/32768 of an output unit.
    let significand = ((polynomial + 16_384 + 2_023) >> 15) as u32;
    let magnitude = ((252 - exponent) << 23) + significand;
    Some(f32::from_bits((bits & 0x8000_0000) | magnitude))
}

pub(crate) fn divide(numerator: f32, denominator: f32) -> f32 {
    divide_by(numerator, denominator, false)
}

pub(super) fn divide_by(numerator: f32, denominator: f32, literal_divisor: bool) -> f32 {
    factor(denominator, literal_divisor).map_or_else(|| numerator / denominator, |reciprocal| numerator * reciprocal)
}

pub(super) fn divide_add(numerator: f32, denominator: f32, addend: f32, literal_divisor: bool) -> f32 {
    factor(denominator, literal_divisor).map_or_else(|| numerator / denominator + addend, |reciprocal| numerator.mul_add(reciprocal, addend))
}
