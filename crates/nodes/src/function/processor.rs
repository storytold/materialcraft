use super::{EvaluationContext, EvaluationState};
use crate::{CookContext, CookResult};
use craft_texture::{Image, Kind, Limits};
use materialcraft_core::Value;
use rayon::prelude::*;

fn pixel(value: Value, gray: bool) -> Result<[f32; 4], String> {
    let values = super::value_type::floats(&value)?;

    let color = if gray {
        let intensity = if super::value_type::dimensions(&value) == 4 {
            // RGBA results on a grayscale processor use equal RGB weights
            // Preserve the fused accumulation order at floating-point outputs
            values[2].mul_add(1.0 / 3.0, values[0].mul_add(1.0 / 3.0, values[1] * (1.0 / 3.0)))
        } else {
            values[0]
        };

        [intensity, 0.0, 0.0, 0.0]
    } else {
        match super::value_type::dimensions(&value) {
            1 => [values[0], values[0], values[0], 1.0],
            2 => [values[0], values[1], 0.0, 1.0],
            3 => [values[0], values[1], values[2], 1.0],
            _ => values,
        }
    };
    // Texture storage has a defined NaN bit pattern even when the host operation's payload varies.
    Ok(color.map(|v| if v.is_nan() { f32::from_bits(0x7fff_ffff) } else { v }))
}

pub(crate) fn pixel_processor(context: &CookContext) -> CookResult {
    pixel_processor_with_dependencies(context).map(|(image, _)| vec![image])
}

/// Cook the image and report published inputs read by the compiled pixel function.
pub fn pixel_processor_with_dependencies(context: &CookContext) -> Result<(Image, bool), String> {
    let graph = context.function("function").ok_or("missing pixel function")?;
    let gray = !context.b("color_mode");
    let kind = if gray { Kind::Gray } else { Kind::Color };

    let (width, height) = craft_texture::clamp_size(context.w, context.h);
    let frame = EvaluationContext {
        image_formats: context.input_formats,
        scope: super::FunctionScope::PixelProcessor,
        position: [0.0; 2],
        size: [width as f32, height as f32],
        images: context.inputs,
        tile: context.tile,
        variables: context.vars.graph_values(),
    };
    let program = super::compile::compile_program(graph, context.seed, context.vars.literal_inputs(), Some(&frame))?;
    let channels = kind.channels();
    let mut data = vec![0.0; width as usize * height as usize * channels];
    data.par_chunks_mut(width as usize * channels).enumerate().try_for_each_init(EvaluationState::default, |state, (y, row)| -> Result<(), String> {
        for (x, target) in row.chunks_exact_mut(channels).enumerate() {
            let evaluation = EvaluationContext { position: [(x as f32 + 0.5) / width as f32, (y as f32 + 0.5) / height as f32], ..frame };

            let result = pixel(program.evaluate_pixel(&evaluation, state)?, gray)?;
            target.copy_from_slice(result.get(..channels).ok_or("invalid output channels")?);
        }

        Ok(())
    })?;
    let image = Image::from_data(width, height, kind, data, &Limits::default()).map_err(|e| e.to_string())?;
    Ok((image, program.reads_graph_parameter(frame.variables)))
}
