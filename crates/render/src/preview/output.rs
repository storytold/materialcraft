use materialcraft_geom::view::Shader;

use super::Settings;

/// How shaded values become display values
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Output {
    /// Scene light: exposed and tone mapped
    Lit,
    /// A color shown as it is: encoded for display, nothing else
    Color,
    /// Data shown as numbers: 0 is black and 1 is white
    Data,
}

impl Output {
    pub(super) fn of(settings: &Settings) -> Output {
        match settings.look.shader {
            _ if settings.alpha_view => Output::Data,
            Shader::MeshInfo => Output::Data,
            Shader::Unlit => Output::Color,
            _ => Output::Lit,
        }
    }
}
