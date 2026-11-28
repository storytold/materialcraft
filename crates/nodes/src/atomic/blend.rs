use super::ATOMIC;
use super::input_resize::{InputResize, converted_color, resized_input};
use crate::ops;
use crate::ops::BLEND_MODES;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::{Image, Kind};
use materialcraft_core::OutputFormat;

pub(super) fn definition() -> NodeDefinition {
    node("blend", "Blend", ATOMIC)
        .desc("Blends a foreground over a background with a blending mode, weighted by opacity and an optional mask.")
        .atomic()
        .any_in("foreground", "Foreground")
        .any_in("background", "Background")
        .primary_input("background")
        .gray_in("mask", "Opacity")
        .float("opacity", "Opacity", 1.0, 0.0, 1.0)
        .choice("mode", "Blending mode", BLEND_MODES, 0)
        .boolean("alpha", "Alpha blending", true)
        .choice("color_blending", "Alpha interpretation", &["Source alpha", "Ignore alpha", "Straight", "Premultiplied"], 0)
        .any_out()
        .cook(blend)
}

/// The texture a fully weighted, unmasked Blend selects before size or depth conversion.
pub fn blend_selected_input(context: &CookContext) -> Option<usize> {
    let mode = context.i("mode");
    let opacity = context.f("opacity");
    if context.input(2).is_some() || !(0..=8).contains(&mode) {
        return None;
    }
    if opacity <= 0.0 {
        Some(1)
    } else if opacity >= 1.0 && (mode == 7 || (mode == 0 && context.input(0).is_some_and(Image::is_gray))) {
        Some(0)
    } else {
        None
    }
}

fn selected_float_color(context: &CookContext, port: usize, dynamic_opacity: bool) -> Option<Image> {
    let image = context.input(port)?;
    let source = context.input_formats.get(port).copied().unwrap_or(if image.integer_sampling() { OutputFormat::U16 } else { OutputFormat::F32 });
    if image.is_gray()
        || (image.width(), image.height()) != (context.w, context.h)
        || !matches!(source, OutputFormat::F16 | OutputFormat::F32)
        || !matches!(context.format, OutputFormat::F16 | OutputFormat::F32)
    {
        return None;
    }
    // Fixed opacity can forward the selected texture. A bound opacity still runs the color conversion at either endpoint.
    Some(if source == context.format && !dynamic_opacity {
        image.clone()
    } else {
        image.map(Kind::Color, |pixel| converted_color(pixel.map(crate::function::flush_subnormal)))
    })
}

fn blend(context: &CookContext) -> CookResult {
    blend_with_opacity(context, false)
}

/// Cook with the resolved opacity and whether its expression reads an exposed graph parameter.
pub fn blend_with_opacity(context: &CookContext, dynamic_opacity: bool) -> CookResult {
    let (mode, opacity) = (context.i("mode"), context.f("opacity"));
    // Stored graphs can compute an opacity outside the slider range. Bound it before applying the mask.
    let opacity = if (0..=8).contains(&mode) { opacity.clamp(0.0, 1.0) } else { opacity };
    let selected = blend_selected_input(context);
    if mode == 7
        && let Some(image) = selected.and_then(|port| selected_float_color(context, port, dynamic_opacity))
    {
        return Ok(vec![image]);
    }
    let copies_foreground = selected == Some(0);
    let foreground = resized_input(context, 0, if copies_foreground { InputResize::Copy } else { InputResize::Filter });
    let background = resized_input(context, 1, if opacity == 0.0 { InputResize::Copy } else { InputResize::Filter });
    let kind = if [&foreground, &background].iter().any(|i| i.as_ref().is_some_and(|i| !i.is_gray())) { Kind::Color } else { Kind::Gray };
    let foreground = foreground.unwrap_or_else(|| Image::filled(context.w, context.h, kind, [0.0, 0.0, 0.0, 0.0]));
    let background = background.unwrap_or_else(|| Image::filled(context.w, context.h, kind, [0.0, 0.0, 0.0, 1.0]));
    let mask = resized_input(context, 2, InputResize::Filter).map(|m| m.to_gray());
    let color_blending = context.i("color_blending");
    let blend_alpha = context.b("alpha") && !foreground.is_gray();
    let clamp =
        matches!(context.format, materialcraft_core::OutputFormat::U8 | materialcraft_core::OutputFormat::U16 | materialcraft_core::OutputFormat::Inherit);
    Ok(vec![Image::generate(context.w, context.h, kind, |x, y| {
        let (x, y) = (x as i64, y as i64);
        let m = mask.as_ref().map_or(1.0, |m| m.get_gray(x, y, false));
        // These modes read the mask through its transparency before applying global opacity.
        let m = if ((0..=2).contains(&mode) && color_blending == 0) || matches!(mode, 3 | 7 | 8) || (matches!(mode, 4..=6) && color_blending != 2) {
            1.0 - (1.0 - m)
        } else {
            m
        };
        ops::blend_px(mode, background.get(x, y, false), foreground.get(x, y, false), opacity * m, color_blending, blend_alpha, clamp)
    })])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve_params;
    use materialcraft_core::{OutputFormat, Value};
    use std::collections::BTreeMap;

    #[test]
    fn switch_endpoints_copy_signed_zero_at_the_selected_color_precision() {
        use OutputFormat::{F16, F32};
        let small = -2.0_f32.powi(-27);
        let pixels = [[0.5, -0.0, -0.25, -0.0], [-0.0, -0.5, -0.25, 0.5], [small, 0.5, 0.25, -0.0]];
        let field = Image::generate(3, 1, Kind::Color, |x, _| pixels[x as usize]);
        let other = Image::filled(3, 1, Kind::Color, [-0.5, -1.0, 1.0, 0.5]);
        for (source, target) in [(F32, F32), (F16, F32), (F32, F16), (F16, F16)] {
            let selected = crate::output_precision::apply(field.clone(), source);
            for (slot, opacity) in [(0, 1.0), (1, 0.0)] {
                let mut inputs = [Some(other.clone()), Some(other.clone()), None];
                inputs[slot] = Some(selected.clone());
                let mut formats = [F32; 3];
                formats[slot] = source;
                let params = resolve_params(&definition(), &BTreeMap::from([("mode".into(), Value::Enum(7)), ("opacity".into(), Value::Float(opacity))]));
                let vars = BTreeMap::new();
                let context = CookContext {
                    w: 3,
                    h: 1,
                    seed: 0,
                    tile: true,
                    input_formats: &formats,
                    format: target,
                    inputs: &inputs,
                    params: &params,
                    resource: &|_| None,
                    vars: &vars,
                };
                let result = crate::output_precision::apply(blend(&context).unwrap().remove(0), target);
                let same = source == target;
                let zero = if same { -0.0_f32 } else { 0.0 };
                let tiny = match (source, target) {
                    (F32, F32) => small,
                    (F16, F32) => 0.0,
                    _ => -0.0,
                };
                for (i, expected) in [[0.5, zero, -0.25, zero], pixels[1], [tiny, 0.5, 0.25, zero]].into_iter().enumerate() {
                    assert_eq!(
                        result.get(i as i64, 0, false).map(f32::to_bits),
                        expected.map(f32::to_bits),
                        "{source:?} to {target:?}, input {slot}, pixel {i}"
                    );
                }
            }
        }
    }

    #[test]
    fn switch_converts_subnormal_inputs_at_the_color_operation() {
        let background = [0x0000_0000, 0xbfd3_82af, 0xbf69_87c4, 0x3f80_0000].map(f32::from_bits);
        for (opacity, target, bits, expected) in [
            (1.0, OutputFormat::F16, [0xb180_0000, 0x3f80_0000, 0x8000_0001, 0x3b03_86a1], [0x8000_0000, 0x3f80_0000, 0, 0x3b03_8000]),
            (0.6137, OutputFormat::F32, [0x8000_0001, 0x3b03_86a1, 0xbf69_87c4, 0x8000_0000], [0, 0xbf23_1903, 0xbf69_87c4, 0x3ec5_c91e]),
        ] {
            let inputs = [Some(Image::filled(1, 1, Kind::Color, bits.map(f32::from_bits))), Some(Image::filled(1, 1, Kind::Color, background)), None];
            let params = resolve_params(&definition(), &BTreeMap::from([("mode".into(), Value::Enum(7)), ("opacity".into(), Value::Float(opacity))]));
            let vars = BTreeMap::new();
            let context = CookContext {
                w: 1,
                h: 1,
                seed: 0,
                tile: true,
                input_formats: &[OutputFormat::F32; 3],
                format: target,
                inputs: &inputs,
                params: &params,
                resource: &|_| None,
                vars: &vars,
            };
            let result = crate::output_precision::apply(blend(&context).unwrap().remove(0), target);
            assert_eq!(result.get(0, 0, false).map(f32::to_bits), expected, "opacity {opacity}, {target:?}");
        }
    }

    #[test]
    fn add_sub_limits_global_opacity_before_blending() {
        let mut inputs = [
            Some(Image::gray(2, 1, |x, _| if x == 0 { 35_122.0 / 65_535.0 } else { 32_767.0 / 65_535.0 })),
            Some(Image::gray(2, 1, |_, _| 32_767.0 / 65_535.0)),
            None,
        ];
        let vars = BTreeMap::new();
        for (opacity, masked, expected) in [(6.000_000_5, false, [37_476, 32_766]), (-0.5, false, [32_767, 32_767]), (6.000_000_5, true, [34_115, 32_767])] {
            inputs[2] = masked.then(|| Image::gray(2, 1, |x, _| if x == 0 { 73.0 / 255.0 } else { 126.0 / 255.0 }));
            let params = resolve_params(&definition(), &BTreeMap::from([("mode".into(), Value::Enum(4)), ("opacity".into(), Value::Float(opacity))]));
            let context = CookContext {
                w: 2,
                h: 1,
                seed: 0,
                tile: true,
                input_formats: &[OutputFormat::U16, OutputFormat::U16, OutputFormat::F32],
                format: OutputFormat::U16,
                inputs: &inputs,
                params: &params,
                resource: &|_| None,
                vars: &vars,
            };
            let image = crate::output_precision::apply(blend(&context).unwrap().remove(0), OutputFormat::U16);
            assert_eq!([0, 1].map(|x| (image.get_gray(x, 0, false) * 65_535.0).round() as u16), expected);
        }
    }

    #[test]
    fn blend_resizes_connected_images_through_stored_levels() {
        for (port, source, format, expected) in [
            (0, OutputFormat::U16, OutputFormat::U16, [26_112.0 / 65_535.0, 42_495.0 / 65_535.0]),
            (1, OutputFormat::U16, OutputFormat::F16, [0.3984375, f32::from_bits(0x3f25e000)]),
            (2, OutputFormat::F32, OutputFormat::U8, [102.0 / 255.0, 165.0 / 255.0]),
        ] {
            let mut inputs = [Some(Image::filled(16, 16, Kind::Gray, [1.0; 4])), Some(Image::filled(16, 16, Kind::Gray, [0.0; 4])), None];
            let field = Image::gray(2, 2, |x, y| [0.0, 0.25, 0.75, 1.0][(2 * y + x) as usize]);
            inputs[port] = Some(crate::output_precision::apply(field, source));
            let params = resolve_params(&definition(), &BTreeMap::from([("opacity".into(), Value::Float(if port == 1 { 0.0 } else { 1.0 }))]));
            let vars = BTreeMap::new();
            let context = CookContext {
                w: 16,
                h: 16,
                seed: 317,
                tile: true,
                input_formats: &[source; 3],
                format,
                inputs: &inputs,
                params: &params,
                resource: &|_| None,
                vars: &vars,
            };
            let result = crate::output_precision::apply(blend(&context).unwrap().remove(0), format);
            assert_eq!([result.get(2, 7, false)[0], result.get(6, 12, false)[0]], expected, "input {port}");
        }
    }

    #[test]
    fn blend_rounds_mask_color_and_coverage_independently() {
        let foreground = [116.0, 38.0, 60.0, 36.0].map(|v| v / 255.0);
        let background = [158.0, 163.0, 10.0, 68.0].map(|v| v / 255.0);
        let mask = 44.0 / 255.0;
        for (mode, interpretation, opacity, alpha, hdr, expected) in [
            (0, 0, 1.0, true, false, [0x3f1d97ad, 0x3f209511, 0x3d343117, 0x3e91adfe]),
            (0, 0, 0.07, true, false, [0x3f1e8c37, 0x3f236cdd, 0x3d21ff39, 0x3e892c70]),
            (0, 0, 0.3, false, false, [0x3f1e4fbd, 0x3f22b8df, 0x3d267f2b, 0x3e888889]),
            (0, 0, 0.3, true, true, [0x3f514e8e, 0x3f4b8353, 0xbf55520b, 0xbdcc9712]),
            (3, 0, 0.6875, true, false, [0x3f1d2bec, 0x3f214e9d, 0x3d1e9201, 0x3e8ed249]),
            (3, 1, 0.6875, true, false, [0x3f145cda, 0x3f131eb2, 0x3d120e5e, 0x3e8ed249]),
            (3, 3, 0.6875, true, false, [0x3f1d2bec, 0x3f214e9d, 0x3d1e9201, 0x3e8ed249]),
            (7, 0, 0.6137, true, false, [0x3f1a2797, 0x3f1659c4, 0x3d75aca1, 0x3e81bac6]),
            (8, 0, 0.6137, true, false, [0x3f21760a, 0x3f319bf2, 0x3d286e88, 0x3e8e257d]),
            (8, 1, 0.6137, false, false, [0x3f32bf2c, 0x3f834be5, 0x3d57e86e, 0x3e888889]),
        ] {
            let level = |v: f32| if hdr { v * 3.0 - 1.0 } else { v };
            let inputs = [
                Some(Image::filled(1, 1, Kind::Color, foreground.map(level))),
                Some(Image::filled(1, 1, Kind::Color, background.map(level))),
                Some(Image::gray(1, 1, |_, _| level(mask))),
            ];
            let vars = BTreeMap::new();
            let params = resolve_params(
                &definition(),
                &BTreeMap::from([
                    ("mode".into(), Value::Enum(mode)),
                    ("color_blending".into(), Value::Enum(interpretation)),
                    ("opacity".into(), Value::Float(opacity)),
                    ("alpha".into(), Value::Bool(alpha)),
                ]),
            );
            let context = CookContext {
                input_formats: &[],
                format: OutputFormat::F32,
                w: 1,
                h: 1,
                seed: 0,
                tile: true,
                inputs: &inputs,
                params: &params,
                resource: &|_| None,
                vars: &vars,
            };
            let result = blend(&context).unwrap();
            assert_eq!(result[0].get(0, 0, false).map(f32::to_bits), expected, "mode {mode}, interpretation {interpretation}, opacity {opacity}");
        }
    }
}
