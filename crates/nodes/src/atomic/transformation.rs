use super::ATOMIC;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use materialcraft_core::OutputFormat;

pub(super) fn definition() -> NodeDefinition {
    node("transformation", "Transformation 2D", ATOMIC)
        .desc("Transforms the input with an offset and a 2 by 2 matrix.")
        .atomic()
        .any_in("input", "Input")
        .float2("offset", "Offset", [0.0; 2], -1.0, 1.0)
        .float4("matrix", "Transform matrix", [1.0, 0.0, 0.0, 1.0], -10.0, 10.0)
        .choice("filtering", "Filtering", &["Bilinear", "Nearest"], 0)
        .choice("mip_mode", "Mipmap mode", &["Automatic", "Manual"], 0)
        .int("mip_level", "Mipmap level", 0, 0, 13)
        .color("matte", "Matte color", [0.0; 4])
        .any_out()
        .cook(transformation)
}

fn transformation(context: &CookContext) -> CookResult {
    // Preserve source texels so nearest sampling does not blur the input
    let input = context.input(0).cloned().unwrap_or_else(|| Image::filled(context.w, context.h, Kind::Gray, [0.0; 4]));
    let offset = context.v2("offset");
    let matrix = context.v4("matrix");
    const MIN_DETERMINANT: f32 = 1e-8;
    let determinant = matrix[0] * matrix[3] - matrix[1] * matrix[2];
    if !determinant.is_finite() || determinant.abs() < MIN_DETERMINANT {
        return Err("transform matrix is singular".into());
    }

    let nearest = context.i("filtering") == 1;
    let source_format = context.input_formats.first().copied().unwrap_or(if input.integer_sampling() { OutputFormat::U16 } else { OutputFormat::F32 });
    let tiling = context.tiling_axes();
    let centered = offset == [0.0; 2] && matches!(matrix, [1.0 | -1.0, 0.0, 0.0, 1.0 | -1.0] | [0.0, 1.0 | -1.0, 1.0 | -1.0, 0.0]);
    let (input, sample_format) = if centered && !nearest && context.i("mip_mode") == 0 {
        let target = if matrix[0] == 0.0 { [context.h, context.w] } else { [context.w, context.h] };
        resize_intermediate_levels(input, target, source_format, context.format, tiling)
    } else {
        let levels = mip_level(&input, matrix, context);
        let sample_format = if levels > 0 && (input.width() > 1 || input.height() > 1) { context.format } else { source_format };
        (reduce_mip(input, levels, source_format, context.format), sample_format)
    };
    let matte = crate::output_precision::store_pixel(context.v4("matte"), sample_format);
    // A centered resize extends an untiled edge texel, including after a manually selected mip.
    let border = (!centered).then_some(matte);
    // Fold the pivot into the translation once. SBS stores matrix columns consecutively.
    let translation = [0.5 - (0.5 * matrix[0] + 0.5 * matrix[2]) + offset[0], 0.5 - (0.5 * matrix[1] + 0.5 * matrix[3]) + offset[1]];

    Ok(vec![Image::generate_uv(context.w, context.h, input.kind(), |u, v| {
        let source_u = matrix[2].mul_add(v, matrix[0] * u) + translation[0];
        let source_v = matrix[3].mul_add(v, matrix[1] * u) + translation[1];
        if nearest {
            if (!tiling[0] && !(0.0..1.0).contains(&source_u)) || (!tiling[1] && !(0.0..1.0).contains(&source_v)) {
                matte
            } else {
                input.sample_nearest_axes(source_u, source_v, tiling)
            }
        } else {
            crate::texture_sampler::sample(&input, [source_u, source_v], tiling, sample_format, border)
        }
    })])
}

fn resize_intermediate_levels(
    mut input: Image,
    target: [u32; 2],
    mut source_format: OutputFormat,
    destination_format: OutputFormat,
    tiling: [bool; 2],
) -> (Image, OutputFormat) {
    let source = [input.width(), input.height()];
    let mut next = target.map(|n| n.clamp(1, craft_texture::MAX_SIDE));
    let mut levels = Vec::new();
    // Work back from the output so axes with unequal ratios reach their final step together.
    // For example, 16×16 to 64×32 passes through 32×16.
    while next != source {
        next = std::array::from_fn(|i| if next[i] > source[i] { next[i].div_ceil(2).max(source[i]) } else { next[i].saturating_mul(2).min(source[i]) });
        if next != source {
            levels.push(next);
        }
    }
    for [width, height] in levels.into_iter().rev() {
        input = crate::texture_sampler::resize_input_level(&input, width, height, source_format, tiling);
        input = crate::output_precision::apply(input, destination_format);
        source_format = destination_format;
    }
    (input, source_format)
}

fn mip_level(input: &Image, matrix: [f32; 4], context: &CookContext) -> u32 {
    // Thirteen reductions cover the maximum supported side of 8192 texels.
    if context.i("mip_mode") == 1 {
        return context.i("mip_level").clamp(0, 13) as u32;
    }
    let width = context.w.max(1) as f32;
    let height = context.h.max(1) as f32;
    // Measure each source coordinate's gradient over both output axes, in source texels.
    let gradient_squared = |dx: f32, dy: f32, size: u32| {
        let x = dx * (size as f32 / width);
        let y = dy * (size as f32 / height);
        x * x + y * y
    };
    let u = gradient_squared(matrix[0], matrix[2], input.width());
    let v = gradient_squared(matrix[1], matrix[3], input.height());
    // Round the natural log of the squared footprint before converting it to a mip level.
    let log_footprint = f64::from(u.max(v).max(1.0)).ln() as f32;
    let level = 0.5 * (log_footprint / std::f32::consts::LN_2);
    // The fractional level is rounded to ten bits before selecting the nearest image.
    const LEVEL_STEPS: f32 = 1024.0;
    let stored_level = (level * LEVEL_STEPS).round() / LEVEL_STEPS;
    stored_level.round().clamp(0.0, 13.0) as u32
}

fn reduce_mip(mut input: Image, levels: u32, mut source_format: OutputFormat, format: OutputFormat) -> Image {
    for _ in 0..levels {
        if input.width() <= 1 && input.height() <= 1 {
            break;
        }

        // Both final filters read the same mip chain. Each reduction samples at the preceding level's precision.
        input = crate::texture_sampler::reduce_level(&input, source_format, format);
        source_format = format;
    }

    input
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve_params;
    use materialcraft_core::Value;
    use std::collections::BTreeMap;

    #[test]
    fn automatic_mip_samples_near_half_level_boundaries() {
        let vars = BTreeMap::new();
        for (source, output, matrix, corner, interior) in [
            ([64; 2], [64; 2], [1.0, -1.0, 1.0, 1.0], 0x3f1bdc1c, 0x3ee969e9),
            ([64; 2], [64; 2], [0.995, -0.995, 0.995, 0.995], 0x3f45eac6, 0x3eee42ee),
            ([64; 2], [64; 2], [0.9998, -0.9998, 0.9998, 0.9998], 0x3f1c4282, 0x3ee9c847),
            ([64; 2], [64; 2], [1.4139308, 0.0, 0.0, 0.875], 0x3f18f838, 0x3f061050),
            ([64; 2], [64; 2], [1.9993231, -1.9993231, 1.9993231, 1.9993231], 0x3f351f60, 0x3eee62e2),
            ([64; 2], [64; 2], [0.625, 2.7598505, -0.34375, 0.61466354], 0x3f033f7f, 0x3f0292d2),
            ([64; 2], [64; 2], [0.625, 4.667103, -0.34375, 10.302016], 0x3f02df1f, 0x3ef555d5),
            ([64; 2], [64; 2], [1.9996, -1.9996, 1.9996, 1.9996], 0x3f353172, 0x3eee62e2),
            ([64; 2], [64; 2], [3.9992, -3.9992, 3.9992, 3.9992], 0x3f163273, 0x3ef48504),
            ([64; 2], [64; 2], [1.2, 0.9, 0.19, 0.13], 0x3f2c8cad, 0x3e7988fa),
            ([64; 2], [64; 2], [1.2, 0.19, 0.9, 0.13], 0x3f00abeb, 0x3f265292),
            ([128, 32], [64; 2], [1.0, -1.0, 1.0, 1.0], 0x3f054585, 0x3ef272f2),
            ([128, 32], [32, 128], [1.2, 0.9, 0.19, 0.13], 0x3ef1be3d, 0x3f0bd212),
        ] {
            let input = Image::gray(source[0], source[1], |x, y| ((13 * x + 19 * y + 7 * x * y) % 256) as f32 / 255.0).with_integer_sampling(true);
            let params = resolve_params(&definition(), &BTreeMap::from([("matrix".into(), Value::Float4(matrix))]));
            let context = CookContext {
                w: output[0],
                h: output[1],
                seed: 0,
                format: OutputFormat::F32,
                tile: true,
                inputs: &[Some(input)],
                input_formats: &[OutputFormat::U8],
                params: &params,
                resource: &|_| None,
                vars: &vars,
            };
            let image = transformation(&context).unwrap();
            assert_eq!(image[0].get_gray(0, 0, false).to_bits(), corner, "{matrix:?}");
            assert_eq!(image[0].get_gray(19, 43, false).to_bits(), interior, "{matrix:?}");
        }
    }

    #[test]
    fn affine_texture_coordinates_keep_the_fraction_at_rotated_texel_boundaries() {
        let image = Image::generate(128, 128, Kind::Color, |x, y| [(x & 1) as f32, (y & 1) as f32, ((x / 2) & 1) as f32, ((y / 2) & 1) as f32]);
        let params = resolve_params(
            &definition(),
            &BTreeMap::from([
                ("matrix".into(), Value::Float4([2.494_419_6, -1.666_711_1, 1.111_140_7, 1.662_946_3])),
                ("offset".into(), Value::Float2([0.1875, -0.3125])),
                ("mip_mode".into(), Value::Enum(1)),
            ]),
        );
        let context = CookContext {
            w: 64,
            h: 64,
            seed: 773,
            format: OutputFormat::F32,
            tile: true,
            inputs: &[Some(image.with_integer_sampling(true))],
            input_formats: &[OutputFormat::U8],
            params: &params,
            resource: &|_| None,
            vars: &BTreeMap::new(),
        };
        let result = transformation(&context).unwrap();
        for (x, y, bits) in [(62, 9, [0x3eae00ae, 0x3f800000, 0x3f28ffa9, 0x3f800000]), (15, 33, [0x3f04ff85, 0x3f01ff82, 0, 0x3f01ff82])] {
            assert_eq!(result[0].get(x, y, false).map(f32::to_bits), bits);
        }
    }

    #[test]
    fn centered_resizes_filter_rectangular_levels_in_source_coordinates() {
        let vars = BTreeMap::from([("$tiling".into(), Value::Int(1))]);
        for (source_format, height, matrix, mip_mode, mip_level, corner, interior) in [
            (OutputFormat::F32, 8, [1.0, 0.0, 0.0, 1.0], 0, 0, 0xbf27bdf0, 0x3e92acaa),
            (OutputFormat::U16, 32, [0.0, 1.0, -1.0, 0.0], 0, 0, 0x3ea1c922, 0x3f410291),
            (OutputFormat::F32, 64, [1.0, 0.0, 0.0, 1.0], 1, 1, 0xbf0c5715, 0xbc9d4750),
        ] {
            let integer = source_format == OutputFormat::U16;
            let input = Image::gray(16, 16, |x, y| {
                let ramp = f64::from((97 * x + 43 * y + 7 * x * y) % 1024) / 1023.0;
                if integer { (ramp * 65535.0).round() as f32 / 65535.0 } else { (3.0 * ramp - 1.25) as f32 }
            })
            .with_integer_sampling(integer);
            let params = resolve_params(
                &definition(),
                &BTreeMap::from([
                    ("matrix".into(), Value::Float4(matrix)),
                    ("mip_mode".into(), Value::Enum(mip_mode)),
                    ("mip_level".into(), Value::Int(mip_level)),
                    ("matte".into(), Value::Color([0.125, 0.375, -0.5, 1.25])),
                ]),
            );
            let context = CookContext {
                w: 64,
                h: height,
                seed: 631,
                format: OutputFormat::F32,
                tile: false,
                inputs: &[Some(input)],
                input_formats: &[source_format],
                params: &params,
                resource: &|_| None,
                vars: &vars,
            };
            let result = transformation(&context).unwrap();
            assert_eq!(result[0].get_gray(0, 0, false).to_bits(), corner);
            assert_eq!(result[0].get_gray(17, 4, false).to_bits(), interior);
        }
    }

    #[test]
    fn mip_reductions_use_the_precision_of_each_source_level() {
        for (format, bits, expected) in [
            (OutputFormat::F16, [0x3fa2e000, 0x3cd0c000, 0x404e0000, 0xc0180000], 0x3f092000),
            (OutputFormat::F32, [0x4012ab5d, 0xbf9f3b55, 0x4075821b, 0xc02f817b], 0x3f090e53),
            (OutputFormat::U8, [0x3f62e2e3, 0x3ec8c8c9, 0x3f46c6c7, 0x3f068687], 0x3f2525a5),
        ] {
            let input = Image::gray(2, 2, |x, y| f32::from_bits(bits[(2 * y + x) as usize])).with_integer_sampling(format == OutputFormat::U8);
            let output = reduce_mip(input, 1, format, OutputFormat::F32);
            assert_eq!(output.get_gray(0, 0, false).to_bits(), expected);
        }
    }

    #[test]
    fn large_affine_offsets_preserve_the_last_visible_mip_texels() {
        let color = [23.0, 139.0, 211.0, 255.0].map(|v| v / 255.0);
        let inputs = [Some(Image::filled(128, 128, Kind::Color, color).with_integer_sampling(true))];
        for (scale, offset, wrap, target) in [(256.0, [127.5, -127.5], 1, (0, 127)), (1024.0, [-492.5, 511.5], 2, (125, 0))] {
            let vars = BTreeMap::from([("$tiling".into(), Value::Int(wrap))]);
            let params = resolve_params(
                &definition(),
                &BTreeMap::from([
                    ("matrix".into(), Value::Float4([scale, 0.0, 0.0, scale])),
                    ("offset".into(), Value::Float2(offset)),
                    ("matte".into(), Value::Color([0.0, 0.0, 0.0, 1.0])),
                ]),
            );
            let context = CookContext {
                input_formats: &[OutputFormat::U8],
                w: 128,
                h: 128,
                seed: 0,
                format: OutputFormat::U8,
                tile: false,
                inputs: &inputs,
                params: &params,
                resource: &|_| None,
                vars: &vars,
            };
            let image = crate::output_precision::apply(transformation(&context).unwrap().remove(0), OutputFormat::U8);
            assert_eq!(image.get(target.0, target.1, false).map(|v| (v * 255.0).round() as u8), [11, 69, 105, 255]);
            assert_eq!(image.get(80, 80, false), [0.0, 0.0, 0.0, 1.0]);
        }
    }

    #[test]
    fn bilinear_transform_filters_the_matte_at_each_untiled_edge() {
        let params = resolve_params(&definition(), &BTreeMap::from([("offset".into(), Value::Float2([0.25 / 4.0, 0.375 / 4.0]))]));
        let inputs = [Some(Image::filled(4, 4, Kind::Color, [1.0; 4]).with_integer_sampling(true))];
        let vars = BTreeMap::new();
        let context = CookContext {
            input_formats: &[],
            w: 4,
            h: 4,
            seed: 0,
            format: OutputFormat::F32,
            tile: false,
            inputs: &inputs,
            params: &params,
            resource: &|_| None,
            vars: &vars,
        };
        let result = transformation(&context).unwrap();
        for (x, y, level) in [(0, 0, 65535.0), (3, 0, 49151.0), (0, 3, 40959.0), (3, 3, 30720.0)] {
            assert_eq!(result[0].get(x, y, false), [level / 65535.0; 4], "texel ({x}, {y})");
        }
    }

    #[test]
    fn filtered_float_texels_round_at_the_source_precision() {
        let params = resolve_params(&definition(), &BTreeMap::from([("offset".into(), Value::Float2([0.25 / 2.0, 0.375 / 2.0]))]));
        let vars = BTreeMap::new();
        for (format, bits, expected) in [
            (OutputFormat::F16, [0x3fd0e000, 0xbfa46000, 0xc02d8000, 0x407f0000], 0x3e33a000),
            (OutputFormat::F32, [0x3fc50790, 0xc0781fc9, 0x3f8389d2, 0xc029c3cd], 0x3e1fdd4b),
        ] {
            let inputs = [Some(Image::gray(2, 2, |x, y| f32::from_bits(bits[(2 * y + x) as usize])))];
            let context = CookContext {
                w: 2,
                h: 2,
                seed: 0,
                format: OutputFormat::F32,
                tile: true,
                inputs: &inputs,
                input_formats: &[format],
                params: &params,
                resource: &|_| None,
                vars: &vars,
            };
            assert_eq!(transformation(&context).unwrap()[0].get_gray(0, 0, false).to_bits(), expected, "{format:?}");
        }
    }

    #[test]
    fn matte_uses_the_inputs_storage_even_with_a_float_output() {
        let matte = [0.5, 0.1, -0.312345, 1.144444];
        let params = resolve_params(&definition(), &BTreeMap::from([("offset".into(), Value::Float2([1.0, 1.0])), ("matte".into(), Value::Color(matte))]));
        let vars = BTreeMap::new();
        for (format, expected) in [
            (OutputFormat::U8, [127.0 / 255.0, 25.0 / 255.0, 0.0, 1.0]),
            (OutputFormat::U16, [32767.0 / 65535.0, 6553.0 / 65535.0, 0.0, 1.0]),
            (OutputFormat::F16, [0.5, 0.099975586, -0.31225586, 1.1435547]),
            (OutputFormat::F32, matte),
        ] {
            let integer = matches!(format, OutputFormat::U8 | OutputFormat::U16);
            let inputs = [Some(Image::filled(4, 4, Kind::Color, [1.0; 4]).with_integer_sampling(integer))];
            let context = CookContext {
                w: 4,
                h: 4,
                seed: 0,
                format: OutputFormat::F32,
                tile: false,
                inputs: &inputs,
                input_formats: &[format],
                params: &params,
                resource: &|_| None,
                vars: &vars,
            };
            assert_eq!(transformation(&context).unwrap()[0].get(2, 2, false), expected, "{format:?}");
        }
    }
}
