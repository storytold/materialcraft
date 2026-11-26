use super::{ParameterKind, Shader};

/// One parameter of a material model, as the Properties panel lists it
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialParameter {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub kind: ParameterKind,
    /// The output usage whose image replaces this value when the material provides one
    pub usage: Option<&'static str>,
    /// Bits of the shaders that have the parameter (see [`Shader::bit`])
    pub shaders: u16,
    /// False for a value that is stored with the state but that the CPU renderer does not show
    pub rendered: bool,
}

impl MaterialParameter {
    pub fn of(id: &str) -> Option<&'static MaterialParameter> {
        PARAMETERS.iter().find(|parameter| parameter.id == id)
    }

    pub fn applies_to(&self, shader: Shader) -> bool {
        self.shaders & shader.bit() != 0
    }
}

const OPEN_PBR: u16 = Shader::OpenPbr.bit();
const COATED: u16 = Shader::MetallicRoughnessCoated.bit();
const SUBSURFACE: u16 = Shader::MetallicRoughnessSss.bit();
/// The metallic and roughness models
const METALLIC: u16 = OPEN_PBR | Shader::MetallicRoughness.bit() | COATED | SUBSURFACE;
/// The models with a specular color and a glossiness
const GLOSSY: u16 = Shader::SpecularGlossiness.bit() | Shader::Blinn.bit();
const PHYSICAL: u16 = METALLIC | Shader::SpecularGlossiness.bit();
const LIT: u16 = PHYSICAL | Shader::Blinn.bit() | Shader::Lambert.bit();
const SURFACE: u16 = LIT | Shader::Unlit.bit();
const RELIEF: u16 = PHYSICAL | Shader::Blinn.bit();

/// sRGB 204 as linear light: the gray of an unassigned base color
const LIGHT_GRAY: [f32; 3] = [0.6038; 3];
const WHITE: [f32; 3] = [1.0; 3];

const fn level(id: &'static str, label: &'static str, group: &'static str, default: f32, usage: Option<&'static str>, shaders: u16) -> MaterialParameter {
    MaterialParameter { id, label, group, kind: ParameterKind::Level { default }, usage, shaders, rendered: true }
}

const fn color(id: &'static str, label: &'static str, group: &'static str, default: [f32; 3], usage: Option<&'static str>, shaders: u16) -> MaterialParameter {
    MaterialParameter { id, label, group, kind: ParameterKind::Color { default }, usage, shaders, rendered: true }
}

const fn slider(id: &'static str, label: &'static str, group: &'static str, range: [f32; 2], default: f32, shaders: u16) -> MaterialParameter {
    MaterialParameter { id, label, group, kind: ParameterKind::Slider { min: range[0], max: range[1], default }, usage: None, shaders, rendered: true }
}

const fn toggle(id: &'static str, label: &'static str, group: &'static str, default: bool, shaders: u16) -> MaterialParameter {
    MaterialParameter { id, label, group, kind: ParameterKind::Toggle { default }, usage: None, shaders, rendered: true }
}

/// A parameter the CPU renderer keeps but does not show
const fn stored(parameter: MaterialParameter) -> MaterialParameter {
    MaterialParameter { rendered: false, ..parameter }
}

/// Every material parameter, in panel order. The names and defaults follow the public OpenPBR
/// Surface specification
pub const PARAMETERS: &[MaterialParameter] = &[
    level("baseWeight", "Weight", "Base", 1.0, None, OPEN_PBR),
    color("baseColor", "Color", "Base", LIGHT_GRAY, Some("baseColor"), SURFACE),
    level("baseMetalness", "Metalness", "Base", 0.0, Some("metallic"), METALLIC),
    level("baseDiffuseRoughness", "Diffuse roughness", "Base", 0.0, None, OPEN_PBR),
    level("specularWeight", "Weight", "Specular", 1.0, None, METALLIC),
    color("specularColor", "Color", "Specular", WHITE, None, OPEN_PBR),
    color("specular", "Specular", "Specular", [0.04; 3], None, GLOSSY),
    level("glossiness", "Glossiness", "Specular", 0.7, None, GLOSSY),
    level("specularRoughness", "Roughness", "Specular", 0.3, Some("roughness"), METALLIC),
    level("specularAnisotropy", "Anisotropy", "Specular", 0.0, None, OPEN_PBR),
    slider("specularIor", "IOR", "Specular", [1.0, 3.0], 1.5, OPEN_PBR),
    level("transmissionWeight", "Weight", "Transmission", 0.0, None, OPEN_PBR),
    color("transmissionColor", "Color", "Transmission", WHITE, None, OPEN_PBR),
    stored(slider("transmissionDepth", "Depth", "Transmission", [0.0, 10.0], 0.0, OPEN_PBR)),
    stored(level("transmissionDispersionScale", "Dispersion scale", "Transmission", 0.0, None, OPEN_PBR)),
    stored(slider("transmissionDispersionAbbeNumber", "Abbe number", "Transmission", [9.0, 91.0], 20.0, OPEN_PBR)),
    level("subsurfaceWeight", "Weight", "Subsurface", 0.0, None, OPEN_PBR | SUBSURFACE),
    color("subsurfaceColor", "Color", "Subsurface", LIGHT_GRAY, None, OPEN_PBR | SUBSURFACE),
    stored(slider("subsurfaceRadius", "Radius", "Subsurface", [0.0, 10.0], 1.0, OPEN_PBR | SUBSURFACE)),
    color("subsurfaceRadiusScale", "Radius scale", "Subsurface", [1.0, 0.2159, 0.0513], None, OPEN_PBR | SUBSURFACE),
    stored(slider("subsurfaceAnisotropy", "Anisotropy", "Subsurface", [-1.0, 1.0], 0.0, OPEN_PBR)),
    level("coatWeight", "Weight", "Coat", 0.0, None, OPEN_PBR | COATED),
    color("coatColor", "Color", "Coat", WHITE, None, OPEN_PBR | COATED),
    level("coatRoughness", "Roughness", "Coat", 0.0, None, OPEN_PBR | COATED),
    stored(level("coatAnisotropy", "Anisotropy", "Coat", 0.0, None, OPEN_PBR)),
    slider("coatIor", "IOR", "Coat", [1.0, 3.0], 1.6, OPEN_PBR | COATED),
    level("coatDarkening", "Darkening", "Coat", 1.0, None, OPEN_PBR),
    level("fuzzWeight", "Weight", "Fuzz", 0.0, None, OPEN_PBR),
    color("fuzzColor", "Color", "Fuzz", WHITE, None, OPEN_PBR),
    level("fuzzRoughness", "Roughness", "Fuzz", 0.5, None, OPEN_PBR),
    slider("emissionLuminance", "Luminance", "Emission", [0.0, 10.0], 0.0, SURFACE),
    color("emissionColor", "Color", "Emission", WHITE, Some("emissive"), SURFACE),
    level("thinFilmWeight", "Weight", "Thin-film", 0.0, None, OPEN_PBR),
    level("thinFilmThickness", "Thickness", "Thin-film", 0.5, None, OPEN_PBR),
    slider("thinFilmIor", "IOR", "Thin-film", [1.0, 3.0], 1.4, OPEN_PBR),
    level("geometryOpacity", "Opacity", "Geometry", 1.0, Some("opacity"), SURFACE),
    stored(toggle("geometryThinWalled", "Thin walled", "Geometry", false, OPEN_PBR)),
    color("geometryNormal", "Normal", "Geometry", [0.5, 0.5, 1.0], Some("normal"), LIT),
    level("ambientOcclusion", "Ambient occlusion", "Geometry", 1.0, Some("ambientOcclusion"), LIT),
    level("height", "Height", "Height", 0.5, Some("height"), RELIEF),
    stored(toggle("phongTessellation", "Phong tessellation", "Height", false, RELIEF)),
    stored(level("phongTessellationFactor", "Phong tessellation factor", "Height", 0.6, None, RELIEF)),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_parameter_has_its_own_id_and_a_default_it_accepts() {
        for (index, parameter) in PARAMETERS.iter().enumerate() {
            assert!(PARAMETERS[..index].iter().all(|earlier| earlier.id != parameter.id), "{} is listed twice", parameter.id);
            assert_eq!(parameter.kind.accepted(parameter.kind.default()), Some(parameter.kind.default()), "{}", parameter.id);
            assert_ne!(parameter.shaders, 0, "{} belongs to no shader", parameter.id);
        }
    }

    #[test]
    fn the_simple_shaders_list_only_what_they_shade_with() {
        let ids = |shader: Shader| PARAMETERS.iter().filter(|parameter| parameter.applies_to(shader)).map(|parameter| parameter.id).collect::<Vec<_>>();

        assert_eq!(ids(Shader::Unlit), ["baseColor", "emissionLuminance", "emissionColor", "geometryOpacity"]);
        assert_eq!(ids(Shader::Lambert), ["baseColor", "emissionLuminance", "emissionColor", "geometryOpacity", "geometryNormal", "ambientOcclusion"]);
        assert!(ids(Shader::MeshInfo).is_empty());
        assert!(ids(Shader::OpenPbr).len() > ids(Shader::MetallicRoughness).len());
        assert!(ids(Shader::SpecularGlossiness).contains(&"glossiness") && !ids(Shader::SpecularGlossiness).contains(&"baseMetalness"));
    }
}
