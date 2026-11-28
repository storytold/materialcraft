#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use craft_texture::{Image, Kind};
use materialcraft_core::{OutputFormat, Value};
use materialcraft_nodes::{CookContext, find, resolve_params};
use std::{collections::BTreeMap, time::Instant};

fn main() -> Result<(), String> {
    const REPETITIONS: u32 = 3;
    for side in [512, 1024] {
        let inputs = [Some(Image::generate(side, side, Kind::Color, |x, y| {
            [x as f32 / side as f32, y as f32 / side as f32, f32::from((x / 7 + y / 11) % 2 == 0), 0.75]
        }))];
        for (operation, intensity) in [("blur", 32.0), ("directional_blur", 10.0), ("sharpen", 10.0)] {
            let definition = find(operation).ok_or("missing filter definition")?;
            let parameters = BTreeMap::from([("intensity".into(), Value::Float(intensity)), ("angle".into(), Value::Float(0.125))]);
            let parameters = resolve_params(definition, &parameters);
            let resource = |_: &str| None;
            let vars = BTreeMap::new();
            let context = CookContext {
                input_formats: &[],
                w: side,
                h: side,
                seed: 0,
                format: OutputFormat::F32,
                tile: true,
                inputs: &inputs,
                params: &parameters,
                resource: &resource,
                vars: &vars,
            };

            let start = Instant::now();
            for _ in 0..REPETITIONS {
                std::hint::black_box((definition.cook)(&context)?);
            }
            println!("{operation} {side}x{side}: {:.3} ms/cook ({REPETITIONS} repetitions)", start.elapsed().as_secs_f64() * 1000.0 / f64::from(REPETITIONS));
        }
    }

    Ok(())
}
