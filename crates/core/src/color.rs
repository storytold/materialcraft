/// `[r, g, b(, a)]` linear floats, a single gray number, or `"#rrggbb(aa)"` (sRGB, converted to linear)
pub fn parse_color(j: &serde_json::Value) -> Option<[f32; 4]> {
    use serde_json::Value as J;
    match j {
        J::Array(a) if a.len() >= 3 => {
            let v: Vec<f32> = a.iter().take(4).map(|x| x.as_f64().map(|x| x as f32).filter(|x| x.is_finite())).collect::<Option<_>>()?;
            Some([v[0], v[1], v[2], v.get(3).copied().unwrap_or(1.0)])
        }
        J::Number(n) => n.as_f64().map(|v| v as f32).filter(|v| v.is_finite()).map(|v| [v, v, v, 1.0]),
        J::String(s) => {
            let s = s.trim_start_matches('#');
            if s.len() != 6 && s.len() != 8 {
                return None;
            }
            let c = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok().map(|v| v as f32 / 255.0);
            let a = if s.len() == 8 { c(6)? } else { 1.0 };
            Some([srgb_to_linear(c(0)?), srgb_to_linear(c(2)?), srgb_to_linear(c(4)?), a])
        }
        _ => None,
    }
}

/// sRGB transfer function, encoded → linear
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

/// sRGB transfer function, linear → encoded
pub fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.max(0.0).powf(1.0 / 2.4) - 0.055 }
}

#[cfg(test)]
mod tests {
    use super::parse_color;
    use serde_json::json;

    #[test]
    fn invalid_array_element_rejects_colour() {
        assert_eq!(parse_color(&json!([1.0, null, 0.5, 0.25])), None);
        assert_eq!(parse_color(&json!([1.0, "x", 0.5])), None);
        assert_eq!(parse_color(&json!([1.0, 0.5, 0.25, null])), None);
        assert_eq!(parse_color(&json!([1.0, 0.5, 0.25])), Some([1.0, 0.5, 0.25, 1.0]));
        assert_eq!(parse_color(&json!([1.0, 0.5, 0.25, 0.5])), Some([1.0, 0.5, 0.25, 0.5]));
    }
}
