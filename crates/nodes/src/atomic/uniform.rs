use super::ATOMIC;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use materialcraft_core::Value;

pub(super) fn definition() -> NodeDefinition {
    node("uniform", "Uniform color", ATOMIC)
        .desc("A single color or gray value.")
        .atomic()
        .generator()
        .color("color", "Color", [0.0, 0.0, 0.0, 1.0])
        .boolean("color_mode", "Color output", true)
        .any_out()
        .cook(uniform)
}

fn uniform(context: &CookContext) -> CookResult {
    let source = context.value("color");
    // A three-component function needs an explicit alpha to supply this property.
    let color = if matches!(source, Some(Value::Float3(_))) { [0.0, 0.0, 0.0, 1.0] } else { context.v4("color") };

    Ok(vec![if !context.b("color_mode") {
        let gray = match source {
            Some(Value::Float(level)) => *level,
            // Runtime RGBA rounds each channel's contribution before accumulating the three.
            Some(Value::Float4([r, g, b, _])) => r * (1.0 / 3.0) + g * (1.0 / 3.0) + b * (1.0 / 3.0),
            _ => (color[0] + color[1] + color[2]) / 3.0,
        };
        Image::filled(context.w, context.h, Kind::Gray, [gray; 4])
    } else {
        Image::filled(context.w, context.h, Kind::Color, color)
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use materialcraft_core::{OutputFormat, Value};
    use std::collections::BTreeMap;

    #[test]
    fn grayscale_uniform_averages_the_three_color_channels() {
        for (color, expected) in [([0.125, 0.3125, 0.6875, 0.40625], 0.375_f32), ([1.75, -0.875, -0.25, 0.5], 0.20833333)] {
            let params = BTreeMap::from([("color".into(), Value::Color(color)), ("color_mode".into(), Value::Bool(false))]);
            let context = CookContext {
                w: 1,
                h: 1,
                seed: 0,
                tile: true,
                format: OutputFormat::F32,
                inputs: &[],
                input_formats: &[],
                params: &params,
                resource: &|_| None,
                vars: &BTreeMap::new(),
            };
            let image = uniform(&context).unwrap().remove(0);
            assert_eq!(image.get_gray(0, 0, false).to_bits(), expected.to_bits());
        }
    }
}
