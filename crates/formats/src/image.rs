use crate::{Depth, FormatError, Result, guard};
use craft_image_codecs::{ChannelLayout, CodecError, DecodeOptions, EncodeOptions, Format, Limits as CodecLimits, SampleType};
use craft_texture::{Image, Kind, Limits, MAX_SIDE, linear_to_srgb, srgb_to_linear};
use materialcraft_nodes::BitmapColorSpace;

const MAX_DECODE_BYTES: u64 = 1 << 30;

/// Decode image file bytes into a linear float image (8-bit color is sRGB-decoded; grayscale files stay grayscale)
pub fn decode(bytes: &[u8], linear: bool) -> Result<Image> {
    decode_with_color_space(bytes, if linear { BitmapColorSpace::Linear } else { BitmapColorSpace::Srgb })
}

/// Decode a bitmap using its file transfer or an explicit interpretation of its stored channels.
pub fn decode_with_color_space(bytes: &[u8], space: BitmapColorSpace) -> Result<Image> {
    if bytes.starts_with(b"DDS ") {
        return guard(|| crate::dds_image::decode(bytes, space == BitmapColorSpace::Linear), || FormatError::Image("DDS decoder failed".into()));
    }

    const SVG_SNIFF_BYTES: usize = 4096;
    if std::str::from_utf8(bytes.get(..bytes.len().min(SVG_SNIFF_BYTES)).unwrap_or_default()).is_ok_and(|text| text.contains("<svg")) {
        return guard(|| crate::svg::decode(bytes), || FormatError::Image("SVG decoder failed".into()));
    }

    let limits = CodecLimits::new(MAX_SIDE, MAX_SIDE, u64::from(MAX_SIDE) * u64::from(MAX_SIDE), MAX_DECODE_BYTES);
    let opts = DecodeOptions { limits, keep_orientation: true };
    let format = craft_image_codecs::detect(bytes).unwrap_or(Format::Tga);
    let failed = || FormatError::Image("the decoder failed on this file".into());
    let codec_error = |error: CodecError| match error {
        CodecError::LimitExceeded(message) => FormatError::Image(message),
        error => FormatError::Image(error.to_string()),
    };
    let decoded = guard(|| craft_image_codecs::decode_as_with(format, bytes, &opts).map_err(codec_error), failed)?;
    let (w, h) = decoded.dimensions();
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err(FormatError::Image(format!("{w}x{h} is outside 1..={MAX_SIDE} pixels")));
    }
    let png = if space == BitmapColorSpace::FromFile && format == Format::Png { materialcraft_core::png_raster_info(bytes) } else { None };
    let stored = match space {
        BitmapColorSpace::Linear => true,
        BitmapColorSpace::Srgb => false,
        BitmapColorSpace::FromFile => png.is_some() || matches!(format, Format::Jpeg | Format::Bmp | Format::Tiff | Format::Tga),
    };
    let table = png.and_then(|info| {
        let maximum = if info.format == materialcraft_core::OutputFormat::U8 { 255 } else { 65535 };
        png_channel_table(bytes, maximum)
    });
    let float = decoded.sample_type().is_float();
    if decoded.layout().is_gray() {
        let gray = decoded.converted(ChannelLayout::Gray, SampleType::F32);
        let mut data = gray.to_f32_samples().ok_or_else(|| FormatError::Image("bad buffer".into()))?;
        if let Some(table) = &table {
            for value in &mut data {
                *value = mapped_channel(table, *value);
            }
        }
        return Image::from_data(w, h, Kind::Gray, data, &Limits::default())
            .map(|image| image.with_integer_sampling(!float))
            .map_err(|error| FormatError::Image(error.to_string()));
    }
    let mut data = decoded.to_rgba_f32(&limits).map_err(codec_error)?;
    if let Some(table) = table {
        for pixel in data.as_chunks_mut::<4>().0 {
            for channel in &mut pixel[..3] {
                *channel = mapped_channel(&table, *channel);
            }
        }
    } else if !(float || stored) {
        for pixel in data.as_chunks_mut::<4>().0 {
            for channel in &mut pixel[..3] {
                *channel = srgb_to_linear(*channel);
            }
        }
    }
    Image::from_data(w, h, Kind::Color, data, &Limits::default())
        .map(|image| image.with_integer_sampling(stored && !float))
        .map_err(|error| FormatError::Image(error.to_string()))
}

fn png_channel_table(bytes: &[u8], maximum: u32) -> Option<Vec<f32>> {
    let metadata = craft_image_codecs::png_probe::color_chunks(bytes)?;
    if metadata.srgb {
        return None;
    }
    // Recover the five-place file gamma before rounding the correction exponent.
    let gamma = (f64::from(metadata.gamma?) * 100_000.0).round() / 100_000.0;
    let ratio = gamma * 2.2;
    let exponent = (100_000.0 / ratio).round() / 100_000.0;
    if (ratio * 100_000.0).round() >= 95_000.0 && exponent >= 0.95 {
        return None;
    }
    let levels = f64::from(maximum);
    Some((0..=maximum).map(|v| ((f64::from(v) / levels).powf(exponent) * levels).round() as f32 / maximum as f32).collect())
}

fn mapped_channel(table: &[f32], value: f32) -> f32 {
    let last = table.len().saturating_sub(1) as f32;
    table.get((value * last).round() as usize).copied().unwrap_or(value)
}

/// Encode `image` as `ext` (see [`EXPORT_FORMATS`]). `srgb`: color values are written through the sRGB curve (base color); data maps (normal, roughness…) are written linear
pub fn encode(image: &Image, ext: &str, depth: Depth, srgb: bool) -> Result<Vec<u8>> {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    let encode_channel = |v: f32| if srgb { linear_to_srgb(v) } else { v };
    let (w, h) = (image.width(), image.height());

    // Store sRGB grayscale colors as RGB so readers apply the transfer
    // Preserve single-channel storage for raw grayscale data
    let source_color = image.kind() == Kind::Color;
    let color = source_color || srgb;
    let pixel = |i: usize| -> [f32; 4] {
        if source_color {
            let s = image.data().get(i * 4..i * 4 + 4).unwrap_or(&[0.0, 0.0, 0.0, 1.0]);
            [encode_channel(s[0]), encode_channel(s[1]), encode_channel(s[2]), s[3]]
        } else {
            let v = encode_channel(image.data().get(i).copied().unwrap_or(0.0));
            [v, v, v, 1.0]
        }
    };

    let pixel_count = (w * h) as usize;
    let format = match ext.as_str() {
        "png" => Format::Png,
        "jpg" | "jpeg" => Format::Jpeg,
        "tga" => Format::Tga,
        "bmp" => Format::Bmp,
        "webp" => Format::WebP,
        "exr" => Format::OpenExr,
        "hdr" => Format::Hdr,
        other => return Err(FormatError::Image(format!("unsupported export format `{other}`"))),
    };

    let float_only = matches!(format, Format::OpenExr | Format::Hdr);
    let sixteen_ok = format == Format::Png;
    let quantize_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let quantize_u16 = |v: f32| (v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
    let codec_image = if float_only {
        let data: Vec<f32> = (0..pixel_count)
            .flat_map(|i| {
                let p = pixel(i);
                [p[0], p[1], p[2]]
            })
            .collect();
        craft_image_codecs::Image::from_f32(w, h, ChannelLayout::Rgb, &data)
    } else if depth != Depth::U8 && sixteen_ok {
        if color {
            let data: Vec<u16> = (0..pixel_count).flat_map(|i| pixel(i).map(quantize_u16)).collect();
            craft_image_codecs::Image::from_u16(w, h, ChannelLayout::Rgba, &data)
        } else {
            let data: Vec<u16> = (0..pixel_count).map(|i| quantize_u16(pixel(i)[0])).collect();
            craft_image_codecs::Image::from_u16(w, h, ChannelLayout::Gray, &data)
        }
    } else if color && format != Format::Jpeg {
        let data: Vec<u8> = (0..pixel_count).flat_map(|i| pixel(i).map(quantize_u8)).collect();
        craft_image_codecs::Image::from_u8(w, h, ChannelLayout::Rgba, data)
    } else if color {
        let data: Vec<u8> = (0..pixel_count)
            .flat_map(|i| {
                let p = pixel(i);
                [quantize_u8(p[0]), quantize_u8(p[1]), quantize_u8(p[2])]
            })
            .collect();
        craft_image_codecs::Image::from_u8(w, h, ChannelLayout::Rgb, data)
    } else {
        let data: Vec<u8> = (0..pixel_count).map(|i| quantize_u8(pixel(i)[0])).collect();
        craft_image_codecs::Image::from_u8(w, h, ChannelLayout::Gray, data)
    }
    .map_err(|e| FormatError::Image(e.to_string()))?;

    // Match the JPEG quality and RGB sampling used before the shared encoder cutover.
    let opts = EncodeOptions { jpeg_quality: 75, jpeg_chroma_subsampling: false, embed_icc: false, embed_metadata: false, ..EncodeOptions::default() };
    craft_image_codecs::encode(&codec_image, format, &opts).map_err(|e| FormatError::Image(e.to_string()))
}

#[cfg(test)]
mod transfer_tests {
    use super::*;

    #[test]
    fn jpeg_export_keeps_quality_and_full_chroma_defaults() {
        let texture = Image::filled(16, 16, Kind::Color, [0.5, 0.25, 0.75, 1.0]);
        for extension in ["jpg", "jpeg"] {
            let jpeg = encode(&texture, extension, Depth::U8, true).unwrap();

            // SOF0 for a 16 × 16, 8-bit, three-component baseline JPEG.
            let prefix = [0xff, 0xc0, 0, 17, 8, 0, 16, 0, 16, 3];
            let frame = jpeg.windows(19).find(|segment| segment.starts_with(&prefix)).unwrap();
            let sampling = [frame[11], frame[14], frame[17]];
            assert_eq!((craft_image_codecs::jpeg_quality(&jpeg), sampling), (Some(75), [0x11; 3]), "{extension}");
        }
    }

    #[test]
    fn grayscale_png_round_trips_selected_transfer() {
        let image = Image::filled(2, 2, Kind::Gray, [0.25; 4]);
        for depth in [Depth::U8, Depth::U16] {
            for srgb in [false, true] {
                let bytes = encode(&image, "png", depth, srgb).unwrap();
                let decoded = decode(&bytes, !srgb).unwrap();
                assert_eq!(decoded.kind(), if srgb { Kind::Color } else { Kind::Gray });
                assert!((decoded.data()[0] - 0.25).abs() < 0.003);
                if !srgb {
                    // Existing grayscale data imports stay linear even at the default setting
                    assert!((decode(&bytes, false).unwrap().data()[0] - 0.25).abs() < 0.003);
                }
                let stored = decode(&bytes, true).unwrap();
                let expected = if srgb { linear_to_srgb(0.25) } else { 0.25 };
                assert!((stored.data()[0] - expected).abs() < 0.003);
            }
        }
    }

    #[test]
    fn raster_encode_preserves_defaults_and_rejects_invalid_inputs() {
        let color = Image::filled(2, 2, Kind::Color, [0.5, 0.25, 0.75, 1.0]);
        let png = encode(&color, "png", Depth::U8, true).unwrap();
        for chunk in [b"iCCP", b"eXIf", b"tEXt", b"iTXt", b"zTXt"] {
            assert!(!png.windows(4).any(|w| w == chunk), "unexpected PNG metadata chunk {chunk:?}");
        }

        let jpeg = encode(&color, "jpg", Depth::U8, true).unwrap();
        assert!(!jpeg.windows(4).any(|w| w == b"Exif" || w == b"ICC_"));
        let decoded = decode(&jpeg, false).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2, 2));

        assert!(encode(&color, "psd", Depth::U8, true).is_err());
        assert!(decode(b"not an image", false).is_err());
    }
}
