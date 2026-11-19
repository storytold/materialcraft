use crate::{Depth, FormatError, Result, guard};
use craft_image_codecs::{ChannelLayout, CodecError, DecodeOptions, EncodeOptions, Format, Limits as CodecLimits, SampleType};
use craft_texture::{Image, Kind, Limits, MAX_SIDE, linear_to_srgb, srgb_to_linear};

/// Decode image file bytes into a linear float image (8-bit color is sRGB-decoded; grayscale files stay grayscale)
pub fn decode(bytes: &[u8], linear: bool) -> Result<Image> {
    if bytes.starts_with(b"DDS ") {
        return guard(|| crate::dds_image::decode(bytes, linear), || FormatError::Image("DDS decoder failed".into()));
    }

    const SVG_SNIFF_BYTES: usize = 4096;
    const MAX_DECODE_BYTES: u64 = 1 << 30;

    if std::str::from_utf8(bytes.get(..bytes.len().min(SVG_SNIFF_BYTES)).unwrap_or_default()).is_ok_and(|text| text.contains("<svg")) {
        return guard(|| crate::svg::decode(bytes), || FormatError::Image("SVG decoder failed".into()));
    }

    let limits = CodecLimits::new(MAX_SIDE, MAX_SIDE, u64::from(MAX_SIDE) * u64::from(MAX_SIDE), MAX_DECODE_BYTES);
    let opts = DecodeOptions { limits, keep_orientation: true };
    let format = craft_image_codecs::detect(bytes).unwrap_or(Format::Tga);
    let failed = || FormatError::Image("the decoder failed on this file".into());
    let codec_error = |e: CodecError| match e {
        CodecError::LimitExceeded(msg) => FormatError::Image(msg),
        e => FormatError::Image(e.to_string()),
    };

    let decoded = guard(|| craft_image_codecs::decode_as_with(format, bytes, &opts).map_err(codec_error), failed)?;
    let (w, h) = decoded.dimensions();
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err(FormatError::Image(format!("{w}x{h} is outside 1..={MAX_SIDE} pixels")));
    }

    if decoded.layout().is_gray() {
        let gray = decoded.converted(ChannelLayout::Gray, SampleType::F32);
        let data = gray.to_f32_samples().ok_or_else(|| FormatError::Image("bad buffer".into()))?;
        return Image::from_data(w, h, Kind::Gray, data, &Limits::default()).map_err(|e| FormatError::Image(e.to_string()));
    }

    let float = decoded.sample_type().is_float();
    let mut data = decoded.to_rgba_f32(&limits).map_err(codec_error)?;
    if !(float || linear) {
        for px in data.as_chunks_mut::<4>().0 {
            for c in px.iter_mut().take(3) {
                *c = srgb_to_linear(*c);
            }
        }
    }

    Image::from_data(w, h, Kind::Color, data, &Limits::default()).map_err(|e| FormatError::Image(e.to_string()))
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

    let opts = EncodeOptions { embed_icc: false, embed_metadata: false, ..EncodeOptions::default() };
    craft_image_codecs::encode(&codec_image, format, &opts).map_err(|e| FormatError::Image(e.to_string()))
}

#[cfg(test)]
mod transfer_tests {
    use super::*;

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
}
