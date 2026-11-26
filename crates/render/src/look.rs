use materialcraft_geom::glam::Vec3;
use materialcraft_geom::view::{HeightTechnique, MaterialParameter, MaterialValue, MeshInfo, ParameterKind, Shader, View3d};

/// The material model's values for one frame: what the shader uses wherever the material has no
/// image for a channel, and which channels are switched on
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub shader: Shader,
    pub mesh_info: MeshInfo,
    /// Show the height output by shifting the texture under the view instead of moving the mesh
    pub parallax: bool,
    pub base_weight: f32,
    pub base_color: Vec3,
    pub metalness: f32,
    pub diffuse_roughness: f32,
    pub specular_weight: f32,
    pub specular_color: Vec3,
    pub roughness: f32,
    pub anisotropy: f32,
    pub ior: f32,
    /// Specular color and glossiness of the glossy models
    pub glossy_specular: Vec3,
    pub glossiness: f32,
    pub transmission_weight: f32,
    pub transmission_color: Vec3,
    pub subsurface_weight: f32,
    pub subsurface_color: Vec3,
    /// How strongly each channel of light travels through the surface
    pub subsurface_tint: Vec3,
    pub coat_weight: f32,
    pub coat_color: Vec3,
    pub coat_roughness: f32,
    pub coat_ior: f32,
    pub coat_darkening: f32,
    pub fuzz_weight: f32,
    pub fuzz_color: Vec3,
    pub fuzz_roughness: f32,
    pub emission_luminance: f32,
    pub emission_color: Vec3,
    pub thin_film_weight: f32,
    /// Micrometers
    pub thin_film_thickness: f32,
    pub thin_film_ior: f32,
    pub opacity: f32,
    /// The surface normal in tangent space, as a normal map stores it (0.5, 0.5, 1 is flat)
    pub normal: Vec3,
    pub ambient_occlusion: f32,
    /// Channels whose image is used: a channel switched off ignores it
    pub base_color_map: bool,
    pub metalness_map: bool,
    pub roughness_map: bool,
    pub emission_map: bool,
    pub opacity_map: bool,
    pub normal_map: bool,
    pub ambient_occlusion_map: bool,
}

impl Look {
    /// The look of a view's material
    pub fn of(view: &View3d) -> Look {
        let number = |id: &str| match value(view, id) {
            MaterialValue::Number(number) => number,
            _ => 0.0,
        };
        let color = |id: &str| match value(view, id) {
            MaterialValue::Color(color) => Vec3::from(color),
            _ => Vec3::ZERO,
        };
        let map = |id: &str| view.uses(id);

        Look {
            shader: view.shader,
            mesh_info: view.mesh_info,
            parallax: view.displacement && view.uses("height") && view.shader.shows_height() && view.technique == HeightTechnique::Parallax,
            base_weight: number("baseWeight"),
            base_color: color("baseColor"),
            metalness: number("baseMetalness"),
            diffuse_roughness: number("baseDiffuseRoughness"),
            specular_weight: number("specularWeight"),
            specular_color: color("specularColor"),
            roughness: number("specularRoughness"),
            anisotropy: number("specularAnisotropy"),
            ior: number("specularIor"),
            glossy_specular: color("specular"),
            glossiness: number("glossiness"),
            transmission_weight: number("transmissionWeight"),
            transmission_color: color("transmissionColor"),
            subsurface_weight: number("subsurfaceWeight"),
            subsurface_color: color("subsurfaceColor"),
            subsurface_tint: color("subsurfaceRadiusScale"),
            coat_weight: number("coatWeight"),
            coat_color: color("coatColor"),
            coat_roughness: number("coatRoughness"),
            coat_ior: number("coatIor"),
            coat_darkening: number("coatDarkening"),
            fuzz_weight: number("fuzzWeight"),
            fuzz_color: color("fuzzColor"),
            fuzz_roughness: number("fuzzRoughness"),
            emission_luminance: number("emissionLuminance"),
            emission_color: color("emissionColor"),
            thin_film_weight: number("thinFilmWeight"),
            thin_film_thickness: number("thinFilmThickness"),
            thin_film_ior: number("thinFilmIor"),
            opacity: number("geometryOpacity"),
            normal: color("geometryNormal"),
            ambient_occlusion: number("ambientOcclusion"),
            base_color_map: map("baseColor"),
            metalness_map: map("baseMetalness"),
            roughness_map: map("specularRoughness"),
            emission_map: map("emissionColor"),
            opacity_map: map("geometryOpacity"),
            normal_map: map("geometryNormal"),
            ambient_occlusion_map: map("ambientOcclusion"),
        }
    }
}

impl Default for Look {
    fn default() -> Self {
        Look::of(&View3d::default())
    }
}

/// What the shader reads for a parameter: its value while it is switched on, the value that
/// leaves the surface without it while it is switched off, and its default in a shader that
/// does not have it
fn value(view: &View3d, id: &str) -> MaterialValue {
    let Some(parameter) = MaterialParameter::of(id) else { return MaterialValue::Number(0.0) };

    if !parameter.applies_to(view.shader) {
        return parameter.kind.default();
    }

    if view.disabled.contains(id) {
        return absent(parameter);
    }

    view.value(id).unwrap_or_else(|| parameter.kind.default())
}

/// The value that reads as "this channel is not there"
fn absent(parameter: &MaterialParameter) -> MaterialValue {
    match (parameter.id, parameter.kind) {
        // Fully opaque, unoccluded, flat and at rest are the absent states of these
        ("geometryOpacity" | "ambientOcclusion", _) => MaterialValue::Number(1.0),
        ("geometryNormal" | "height", kind) => kind.default(),
        // A surface always has an index of refraction
        (_, ParameterKind::Slider { default, min, .. }) if min >= 1.0 => MaterialValue::Number(default),
        (_, ParameterKind::Slider { .. } | ParameterKind::Level { .. }) => MaterialValue::Number(0.0),
        (_, ParameterKind::Color { .. }) => MaterialValue::Color([0.0; 3]),
        (_, ParameterKind::Toggle { .. }) => MaterialValue::Toggle(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_channel_switched_off_reads_as_absent_and_one_of_another_shader_as_its_default() {
        let mut view = View3d::default();
        view.material.insert("coatWeight".into(), MaterialValue::Number(0.8));
        assert_eq!(Look::of(&view).coat_weight, 0.8);

        for off in ["baseColor", "geometryOpacity", "specularIor", "coatWeight"] {
            view.disabled.insert(off.into());
        }
        let look = Look::of(&view);
        assert_eq!((look.base_color, look.opacity, look.ior, look.coat_weight), (Vec3::ZERO, 1.0, 1.5, 0.0));
        assert!(!look.base_color_map && look.roughness_map);

        // Lambert has no coat, base weight or metalness: they read as their defaults
        let lambert = Look::of(&View3d { shader: Shader::Lambert, disabled: Default::default(), ..view });
        assert_eq!((lambert.coat_weight, lambert.base_weight, lambert.metalness), (0.0, 1.0, 0.0));
        assert!(!lambert.metalness_map && lambert.base_color_map);
    }

    #[test]
    fn parallax_is_a_choice_of_the_shaders_that_show_height() {
        let parallax = View3d { technique: HeightTechnique::Parallax, ..View3d::default() };

        assert!(Look::of(&parallax).parallax);
        assert!(!Look::of(&View3d { shader: Shader::Unlit, ..parallax }).parallax);
    }
}
