use super::ATOMIC;
use super::input_resize::{InputResize, resized_input};
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use materialcraft_core::{OutputFormat, Value};

pub(super) fn definition() -> NodeDefinition {
    node("levels", "Levels", ATOMIC)
        .desc("Remaps the input range, midtones and output range.")
        .atomic()
        .any_in("input", "Input")
        .float4("in_low", "Level in low", [0.0; 4], 0.0, 1.0)
        .float4("in_mid", "Level in mid", [0.5; 4], 0.0, 1.0)
        .float4("in_high", "Level in high", [1.0; 4], 0.0, 1.0)
        .float4("out_low", "Level out low", [0.0; 4], 0.0, 1.0)
        .float4("out_high", "Level out high", [1.0; 4], 0.0, 1.0)
        .boolean("clamp_intermediate", "Clamp intermediate values", true)
        .any_out()
        .cook(levels)
}

fn levels(context: &CookContext) -> CookResult {
    let image = resized_input(context, 0, InputResize::Levels).unwrap_or_else(|| Image::filled(context.w, context.h, Kind::Gray, [0.0; 4]));
    let control = |name| {
        if image.is_gray()
            && let Some(Value::Float(level)) = context.value(name)
        {
            return [*level; 4];
        }
        let rgba = context.v4(name);
        if image.is_gray() { [(rgba[0] + rgba[1] + rgba[2]) / 3.0; 4] } else { rgba }
    };
    let endpoint = |name| {
        let values = control(name);
        match context.format {
            OutputFormat::U8 | OutputFormat::U16 | OutputFormat::Inherit => values.map(|v| v.clamp(0.0, 1.0)),
            OutputFormat::F16 | OutputFormat::F32 => values,
        }
    };
    let low = endpoint("in_low");
    let mid = control("in_mid");
    let high = endpoint("in_high");
    let output_low = endpoint("out_low");
    let output_high = endpoint("out_high");
    let clamp = context.b("clamp_intermediate");

    let ranges: [_; 4] = std::array::from_fn(|c| ChannelRange::new(low[c], mid[c], high[c], output_low[c], output_high[c]));
    Ok(vec![image.map(image.kind(), |pixel| std::array::from_fn(|c| ranges[c].map(pixel[c], clamp)))])
}

pub fn levels_fn(v: f32, in_low: f32, in_mid: f32, in_high: f32, out_low: f32, out_high: f32) -> f32 {
    ChannelRange::new(in_low, in_mid, in_high, out_low, out_high).map(v, true)
}

struct ChannelRange {
    in_low: f32,
    scale: f32,
    gamma: f32,
    out_low: f32,
    out_span: f32,
}

impl ChannelRange {
    fn new(in_low: f32, in_mid: f32, in_high: f32, out_low: f32, out_high: f32) -> Self {
        let span = in_high - in_low;
        // Coincident endpoints use the slope of a 0.0001-wide interval. Narrow nonzero spans retain their own slope.
        let scale = if span == 0.0 { 10_000.0 } else { span.recip() };
        // The texture evaluator flushes a subnormal slope before it multiplies the input.
        let scale = if scale.is_subnormal() { 0.0_f32.copysign(scale) } else { scale };
        let mid = if in_mid.is_finite() { in_mid.clamp(0.0, 1.0) } else { 0.5 };
        let gamma = if mid <= 0.5 { (9.0 - 16.0 * mid).recip() } else { 16.0 * mid - 7.0 };
        Self { in_low, scale, gamma, out_low, out_span: out_high - out_low }
    }

    fn map(&self, value: f32, clamp: bool) -> f32 {
        let t = (value - self.in_low) * self.scale;
        // A nonfinite input or range must not spread NaN through the graph.
        let t = if t.is_finite() { t } else { 0.0 };
        let t = if clamp { t.clamp(0.0, 1.0) } else { t };
        let curved = if t == 0.0 || t == 1.0 { t } else { crate::function::signed_pixel_power(t, self.gamma) };
        curved.mul_add(self.out_span, self.out_low)
    }
}

#[cfg(test)]
mod levels_tests {
    use super::levels_fn;

    #[test]
    fn enormous_ranges_flush_the_reciprocal_before_scaling_the_pixel() {
        assert!(levels_fn(3.0, 0.0, 0.5, 2.0_f32.powi(125), 0.0, 1.0) > f32::MIN_POSITIVE);
        assert_eq!(levels_fn(3.0, 0.0, 0.5, 2.0_f32.powi(127), 0.0, 1.0).to_bits(), 0);
    }

    #[test]
    fn levels_remaps_the_input_range_midtones_and_output_range() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-5;

        // Identity by default
        for v in [0.0, 0.25, 0.5, 1.0] {
            assert!(close(levels_fn(v, 0.0, 0.5, 1.0, 0.0, 1.0), v));
        }

        // The input range stretches and clips
        assert!(close(levels_fn(0.4, 0.2, 0.5, 0.6, 0.0, 1.0), 0.5));
        assert!(close(levels_fn(0.1, 0.2, 0.5, 0.6, 0.0, 1.0), 0.0));
        assert!(close(levels_fn(0.9, 0.2, 0.5, 0.6, 0.0, 1.0), 1.0));

        // The same gamma curve applies after normalizing the input range
        assert!(close(levels_fn(0.25, 0.0, 0.25, 1.0, 0.0, 1.0), 0.7578583));
        assert!(close(levels_fn(0.3, 0.2, 0.25, 0.6, 0.0, 1.0), 0.7578583));

        // The output range, inverted when low > high
        assert!(close(levels_fn(0.5, 0.0, 0.5, 1.0, 0.2, 0.6), 0.4));
        assert!(close(levels_fn(0.0, 0.0, 0.5, 1.0, 1.0, 0.0), 1.0));

        // At coincident endpoints the low output begins a steep finite ramp.
        assert_eq!(levels_fn(0.49, 0.5, 0.5, 0.5, 0.0, 1.0), 0.0);
        assert_eq!(levels_fn(0.5, 0.5, 0.5, 0.5, 0.0, 1.0), 0.0);
        assert_eq!(levels_fn(0.51, 0.5, 0.5, 0.5, 0.0, 1.0), 1.0);
        assert!(levels_fn(f32::NAN, 0.0, 0.5, 1.0, 0.0, 1.0).is_finite());
        assert!(levels_fn(0.5, f32::NAN, 0.5, f32::NAN, 0.0, 1.0).is_finite());
    }
}
