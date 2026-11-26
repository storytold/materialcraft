use super::MaterialValue;

/// What a material parameter holds and how its control looks
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParameterKind {
    /// A number on a slider
    Slider {
        min: f32,
        max: f32,
        default: f32,
    },
    /// A gray level, 0 to 1
    Level {
        default: f32,
    },
    /// A linear RGB color
    Color {
        default: [f32; 3],
    },
    Toggle {
        default: bool,
    },
}

impl ParameterKind {
    pub fn default(self) -> MaterialValue {
        match self {
            ParameterKind::Slider { default, .. } | ParameterKind::Level { default } => MaterialValue::Number(default),
            ParameterKind::Color { default } => MaterialValue::Color(default),
            ParameterKind::Toggle { default } => MaterialValue::Toggle(default),
        }
    }

    /// `value` as this kind holds it: in range, or `None` when it is another kind of value
    pub fn accepted(self, value: MaterialValue) -> Option<MaterialValue> {
        match (self, value) {
            (ParameterKind::Slider { min, max, .. }, MaterialValue::Number(number)) if number.is_finite() => {
                Some(MaterialValue::Number(number.clamp(min, max)))
            }
            (ParameterKind::Level { .. }, MaterialValue::Number(number)) if number.is_finite() => Some(MaterialValue::Number(number.clamp(0.0, 1.0))),
            (ParameterKind::Color { .. }, MaterialValue::Color(color)) if color.iter().all(|channel| channel.is_finite()) => {
                Some(MaterialValue::Color(color.map(|channel| channel.clamp(0.0, 1.0))))
            }
            (ParameterKind::Toggle { .. }, MaterialValue::Toggle(on)) => Some(MaterialValue::Toggle(on)),
            _ => None,
        }
    }
}
