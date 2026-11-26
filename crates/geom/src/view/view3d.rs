use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{AmbientLight, Background, ColorSpace, HeightTechnique, MaterialParameter, MaterialValue, MeshInfo, PointLight, Projection, Shader, ViewChannels};

/// Scene units per mesh unit: the built-in meshes are 2 across and count as 100 units, one meter
pub const UNITS_PER_MESH_UNIT: f32 = 50.0;

/// The 3D view's scene and look
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct View3d {
    /// Built-in mesh id (see the engine's scene list) or `custom`
    pub scene: String,
    /// UV repeats of the material on the mesh
    pub tiling: f32,
    /// Camera yaw and pitch (degrees) and distance (mesh radii)
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Where the camera orbits: offset from the mesh center (mesh radii, world axes)
    pub target: [f32; 3],
    /// Environment: `studio`, `sunset`, `overcast`, `night`
    pub environment: String,
    pub env_rotation: f32,
    pub exposure: f32,
    /// Height relief in mesh radii
    pub height_scale: f32,
    pub height_level: f32,
    pub normal_y_flip: bool,
    pub show_axis: bool,
    pub show_grid: bool,
    /// See through the mesh where the material's alpha is below one
    pub transparency: bool,
    /// Show the material's alpha channel on the mesh instead of the lit material
    pub alpha_view: bool,

    /// Camera lens, in millimeters on a 36 x 24 mm frame
    pub focal_length: f32,
    pub projection: Projection,
    /// What shows behind the mesh: the studio backdrop of a new view, one color or the environment
    pub background: Background,
    /// Linear RGB of the uniform background
    pub background_color: [f32; 3],
    /// Render at `render_resolution` (or the view's size) times `render_scale` instead of the view's size
    pub override_resolution: bool,
    /// With the override: start from the view's size instead of `render_resolution`
    pub viewport_resolution: bool,
    pub render_resolution: [u32; 2],
    /// Percent
    pub render_scale: u32,
    /// Fit the clip planes to the scene instead of using `near_clip` and `far_clip`
    pub auto_clip: bool,
    /// Scene units from the camera
    pub near_clip: f32,
    pub far_clip: f32,

    /// The environment lights the scene
    pub env_enabled: bool,
    pub ambient: AmbientLight,
    pub lights: [PointLight; 2],

    pub wireframe: bool,
    pub backface_culling: bool,
    pub bounding_box: bool,
    /// Mark the point lights and join them to the origin
    pub light_helpers: bool,
    pub vertex_tangent_space: bool,
    pub show_resolution: bool,
    pub scene_stats: bool,
    pub fps: bool,

    pub shader: Shader,
    pub technique: HeightTechnique,
    pub mesh_info: MeshInfo,
    pub uv_scale: [f32; 2],
    /// Size the material on the mesh by the graph's physical size
    pub physical_size: bool,
    /// Material parameters that differ from their defaults, by id
    pub material: BTreeMap<String, MaterialValue>,
    /// Ids of material parameters switched off: the shader reads them as absent
    pub disabled: BTreeSet<String>,
    /// How finely the displaced mesh is subdivided, 1 (coarse) to 8
    pub tessellation: f32,

    /// Rays per pixel along each side of a resting frame
    pub samples: u32,
    /// Ambient occlusion rays per sample (0 = off)
    pub ambient_occlusion: u32,
    /// The point lights cast shadows
    pub shadows: bool,
    /// Raise the mesh by the height output
    pub displacement: bool,

    pub view_channels: ViewChannels,
    pub color_space: ColorSpace,
}

impl View3d {
    pub const FOCAL_LENGTH: std::ops::RangeInclusive<f32> = 0.01..=100.0;
    /// Camera distance, in mesh radii
    pub const DISTANCE: std::ops::RangeInclusive<f32> = 1.2..=20.0;
    pub const EXPOSURE: std::ops::RangeInclusive<f32> = -10.0..=10.0;
    /// UV repeats of the material: the tiling and each side of the UV scale
    pub const REPEATS: std::ops::RangeInclusive<f32> = 0.1..=64.0;
    /// Mesh radii
    pub const HEIGHT_SCALE: std::ops::RangeInclusive<f32> = 0.0..=0.5;
    /// Scene units: the nearest and the farthest a clip plane may be
    pub const CLIP: std::ops::RangeInclusive<f32> = 0.001..=1_000_000.0;
    pub const MIN_RENDER_SIDE: u32 = 16;
    pub const TESSELLATION: std::ops::RangeInclusive<f32> = 1.0..=8.0;
    pub const MAX_SAMPLES: u32 = 4;
    pub const MAX_AMBIENT_OCCLUSION: u32 = 32;
    pub const MAX_RENDER_SIDE: u32 = 4096;
    pub const RENDER_SCALES: [u32; 4] = [25, 50, 100, 200];

    /// Height of the camera's frame in millimeters: a focal length means what it does on 36 x 24 film
    pub const FRAME_HEIGHT_MM: f32 = 24.0;
    /// How much wider than the mesh the framed view is
    const FRAMING_MARGIN: f32 = 1.1;

    /// The camera's vertical field of view in degrees, from its focal length
    pub fn vertical_fov(&self) -> f32 {
        let focal_length =
            if self.focal_length.is_finite() { self.focal_length.clamp(*View3d::FOCAL_LENGTH.start(), *View3d::FOCAL_LENGTH.end()) } else { 35.0 };
        (2.0 * (View3d::FRAME_HEIGHT_MM * 0.5 / focal_length).atan()).to_degrees()
    }

    /// The camera distance (mesh radii) at which the whole mesh is in view
    pub fn framing_distance(&self) -> f32 {
        self.framing_distance_for_aspect(1.0)
    }

    /// Camera distance that fits the mesh in a viewport of `width / height`
    pub fn framing_distance_for_aspect(&self, aspect: f32) -> f32 {
        let half_angle = (self.vertical_fov().clamp(5.0, 150.0) * 0.5).to_radians();
        let aspect = if aspect.is_finite() && aspect > 0.0 { aspect.min(1.0) } else { 1.0 };
        let half_angle = (half_angle.tan() * aspect).atan();
        (View3d::FRAMING_MARGIN / half_angle.sin()).clamp(*View3d::DISTANCE.start(), *View3d::DISTANCE.end())
    }

    /// The value of a material parameter: the stored one, else its default
    pub fn value(&self, id: &str) -> Option<MaterialValue> {
        let parameter = MaterialParameter::of(id)?;
        Some(self.material.get(id).copied().and_then(|value| parameter.kind.accepted(value)).unwrap_or_else(|| parameter.kind.default()))
    }

    /// A numeric material parameter (0 for one that is not a number)
    pub fn number(&self, id: &str) -> f32 {
        match self.value(id) {
            Some(MaterialValue::Number(number)) => number,
            _ => 0.0,
        }
    }

    /// A color material parameter, linear RGB (black for one that is not a color)
    pub fn color(&self, id: &str) -> [f32; 3] {
        match self.value(id) {
            Some(MaterialValue::Color(color)) => color,
            _ => [0.0; 3],
        }
    }

    /// The material parameter is switched on and belongs to the current shader
    pub fn uses(&self, id: &str) -> bool {
        !self.disabled.contains(id) && MaterialParameter::of(id).is_some_and(|parameter| parameter.applies_to(self.shader))
    }

    /// This view with every number finite and in range and nothing unknown in its material: what
    /// a view read from a file becomes before it is used
    pub fn sanitized(mut self) -> View3d {
        let fresh = View3d::default();
        let held = |value: f32, fallback: f32, range: std::ops::RangeInclusive<f32>| {
            if value.is_finite() { value.clamp(*range.start(), *range.end()) } else { fallback }
        };

        self.tiling = held(self.tiling, fresh.tiling, View3d::REPEATS);
        self.uv_scale = self.uv_scale.map(|repeats| held(repeats, 1.0, View3d::REPEATS));
        self.yaw = held(self.yaw, fresh.yaw, -360.0..=360.0).rem_euclid(360.0);
        self.pitch = held(self.pitch, fresh.pitch, -89.0..=89.0);
        self.distance = held(self.distance, fresh.distance, View3d::DISTANCE);
        self.target = self.target.map(|axis| if axis.is_finite() { axis } else { 0.0 });
        self.env_rotation = held(self.env_rotation, 0.0, -360.0..=360.0).rem_euclid(360.0);
        self.exposure = held(self.exposure, 0.0, View3d::EXPOSURE);
        self.height_scale = held(self.height_scale, fresh.height_scale, View3d::HEIGHT_SCALE);
        self.height_level = held(self.height_level, fresh.height_level, 0.0..=1.0);
        self.focal_length = held(self.focal_length, fresh.focal_length, View3d::FOCAL_LENGTH);
        self.background_color = self.background_color.map(|channel| held(channel, 0.0, 0.0..=1.0));
        self.render_resolution = self.render_resolution.map(|side| side.clamp(View3d::MIN_RENDER_SIDE, View3d::MAX_RENDER_SIDE));
        if !View3d::RENDER_SCALES.contains(&self.render_scale) {
            self.render_scale = fresh.render_scale;
        }
        self.near_clip = held(self.near_clip, fresh.near_clip, View3d::CLIP);
        self.far_clip = held(self.far_clip, fresh.far_clip, View3d::CLIP).max(self.near_clip);
        self.ambient.color = self.ambient.color.map(|channel| held(channel, 0.0, 0.0..=1.0));
        self.lights = self.lights.map(PointLight::sanitized);
        self.tessellation = held(self.tessellation, fresh.tessellation, View3d::TESSELLATION);
        self.samples = self.samples.clamp(1, View3d::MAX_SAMPLES);
        self.ambient_occlusion = self.ambient_occlusion.min(View3d::MAX_AMBIENT_OCCLUSION);

        // A stored value stays only if its parameter exists, takes it and it is not the default
        let material = std::mem::take(&mut self.material);
        self.material = material
            .into_iter()
            .filter_map(|(id, value)| {
                let parameter = MaterialParameter::of(&id)?;
                let value = parameter.kind.accepted(value)?;
                (value != parameter.kind.default()).then_some((id, value))
            })
            .collect();
        self.disabled.retain(|id| MaterialParameter::of(id).is_some());
        self
    }

    /// Camera position and lens of a new view; the scene and everything else stay
    pub fn with_default_position(&self) -> View3d {
        let fresh = View3d::default();
        View3d { yaw: fresh.yaw, pitch: fresh.pitch, distance: fresh.distance, target: fresh.target, ..self.clone() }
    }

    /// The camera of a new view: position and every camera property
    pub fn with_default_camera(&self) -> View3d {
        let fresh = View3d::default();
        View3d {
            focal_length: fresh.focal_length,
            projection: fresh.projection,
            background: fresh.background,
            background_color: fresh.background_color,
            override_resolution: fresh.override_resolution,
            viewport_resolution: fresh.viewport_resolution,
            render_resolution: fresh.render_resolution,
            render_scale: fresh.render_scale,
            auto_clip: fresh.auto_clip,
            near_clip: fresh.near_clip,
            far_clip: fresh.far_clip,
            ..self.with_default_position()
        }
    }

    pub fn with_default_lights(&self) -> View3d {
        let fresh = View3d::default();
        View3d { ambient: fresh.ambient, lights: fresh.lights, ..self.clone() }
    }

    pub fn with_default_environment(&self) -> View3d {
        let fresh = View3d::default();
        View3d { environment: fresh.environment, env_enabled: fresh.env_enabled, env_rotation: fresh.env_rotation, exposure: fresh.exposure, ..self.clone() }
    }

    /// The material back to its defaults. `common_only` keeps the shader and what belongs to it alone
    pub fn with_default_material(&self, common_only: bool) -> View3d {
        let fresh = View3d::default();
        let mut view = self.clone();
        let shared = |id: &String| MaterialParameter::of(id).is_some_and(|parameter| parameter.shaders.count_ones() > 1);

        view.material.retain(|id, _| common_only && !shared(id));
        view.disabled.retain(|id| common_only && !shared(id));
        view.tiling = fresh.tiling;
        view.uv_scale = fresh.uv_scale;
        view.physical_size = fresh.physical_size;
        view.height_scale = fresh.height_scale;
        view.height_level = fresh.height_level;
        view.normal_y_flip = fresh.normal_y_flip;
        if !common_only {
            view.shader = fresh.shader;
            view.technique = fresh.technique;
            view.mesh_info = fresh.mesh_info;
            view.tessellation = fresh.tessellation;
        }
        view
    }
}

impl Default for View3d {
    fn default() -> Self {
        View3d {
            scene: "rounded_cube".into(),
            tiling: 1.0,
            yaw: 35.0,
            pitch: 25.0,
            distance: 3.2,
            target: [0.0; 3],
            environment: "studio".into(),
            env_rotation: 0.0,
            exposure: 0.0,
            height_scale: 0.04,
            height_level: 0.0,
            normal_y_flip: true,
            show_axis: false,
            show_grid: false,
            transparency: true,
            alpha_view: false,
            focal_length: 35.0,
            projection: Projection::Perspective,
            background: Background::Studio,
            // sRGB 4, as dark as a view can be without reading as off
            background_color: [0.0012; 3],
            override_resolution: false,
            viewport_resolution: true,
            render_resolution: [1920, 1080],
            render_scale: 100,
            auto_clip: true,
            near_clip: 1.0,
            far_clip: 10_000.0,
            env_enabled: true,
            ambient: AmbientLight::default(),
            lights: [PointLight::first(), PointLight::second()],
            wireframe: false,
            backface_culling: true,
            bounding_box: false,
            light_helpers: false,
            vertex_tangent_space: false,
            show_resolution: false,
            scene_stats: false,
            fps: false,
            shader: Shader::OpenPbr,
            technique: HeightTechnique::Tessellation,
            mesh_info: MeshInfo::Normal,
            uv_scale: [1.0; 2],
            physical_size: false,
            material: BTreeMap::new(),
            disabled: BTreeSet::new(),
            tessellation: 4.0,
            samples: 2,
            ambient_occlusion: 0,
            shadows: false,
            displacement: true,
            view_channels: ViewChannels::Rgb,
            color_space: ColorSpace::Aces,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_longer_lens_sees_less_and_frames_the_mesh_from_further_away() {
        let normal = View3d::default();
        let long = View3d { focal_length: 85.0, ..View3d::default() };

        assert!((normal.vertical_fov() - 37.85).abs() < 0.01, "35 mm on a 24 mm high frame: {}", normal.vertical_fov());
        assert!(long.vertical_fov() < normal.vertical_fov() && long.framing_distance() > normal.framing_distance());
        assert!((View3d { focal_length: f32::NAN, ..View3d::default() }.vertical_fov() - 37.85).abs() < 0.01, "a broken lens reads as the default");
        assert_eq!(View3d { focal_length: 0.01, ..View3d::default() }.framing_distance(), 1.2, "the widest lens stays outside the mesh");
    }

    #[test]
    fn a_material_value_falls_back_to_its_default_when_missing_or_of_the_wrong_kind() {
        let mut view = View3d::default();
        assert_eq!(view.number("specularRoughness"), 0.3);
        assert_eq!(view.color("baseColor"), [0.6038; 3]);

        view.material.insert("specularRoughness".into(), MaterialValue::Number(7.0));
        view.material.insert("baseColor".into(), MaterialValue::Toggle(true));
        assert_eq!(view.number("specularRoughness"), 1.0, "a stored value is held in range");
        assert_eq!(view.color("baseColor"), [0.6038; 3]);
        assert_eq!(view.value("noSuchParameter"), None);
    }

    #[test]
    fn a_parameter_is_used_only_while_switched_on_and_part_of_the_shader() {
        let mut view = View3d::default();
        assert!(view.uses("coatWeight"));

        view.disabled.insert("coatWeight".into());
        assert!(!view.uses("coatWeight"));

        view.shader = Shader::Lambert;
        assert!(view.uses("baseColor") && !view.uses("baseMetalness"));
    }

    #[test]
    fn the_resets_touch_only_their_own_part() {
        let mut view = View3d { yaw: 120.0, focal_length: 80.0, exposure: 2.0, shader: Shader::Blinn, scene: "sphere".into(), ..View3d::default() };
        view.lights[0].enabled = true;
        view.material.insert("baseColor".into(), MaterialValue::Color([1.0, 0.0, 0.0]));
        view.material.insert("fuzzWeight".into(), MaterialValue::Number(1.0));

        let position = view.with_default_position();
        assert_eq!((position.yaw, position.focal_length, position.scene.as_str()), (35.0, 80.0, "sphere"));
        assert_eq!(view.with_default_camera().focal_length, 35.0);
        assert!(!view.with_default_lights().lights[0].enabled && view.with_default_lights().exposure == 2.0);
        assert_eq!(view.with_default_environment().exposure, 0.0);

        let common = view.with_default_material(true);
        assert_eq!(common.shader, Shader::Blinn);
        assert!(!common.material.contains_key("baseColor") && common.material.contains_key("fuzzWeight"), "only shared parameters reset");
        let all = view.with_default_material(false);
        assert!(all.material.is_empty() && all.shader == Shader::OpenPbr && all.yaw == 120.0);
    }

    #[test]
    fn a_view_from_a_file_is_put_in_range_and_loses_what_it_cannot_use() {
        let mut view = View3d {
            distance: 1.0e9,
            focal_length: f32::NAN,
            tiling: -3.0,
            near_clip: 50.0,
            far_clip: 2.0,
            render_scale: 33,
            samples: 99,
            ..View3d::default()
        };
        view.lights[0].intensity = f32::INFINITY;
        view.material.insert("noSuchParameter".into(), MaterialValue::Number(1.0));
        view.material.insert("baseColor".into(), MaterialValue::Number(1.0));
        view.material.insert("coatWeight".into(), MaterialValue::Number(5.0));
        view.material.insert("fuzzWeight".into(), MaterialValue::Number(0.0));
        view.disabled.insert("noSuchParameter".into());

        let view = view.sanitized();
        assert_eq!((view.distance, view.focal_length, view.tiling), (20.0, 35.0, 0.1));
        assert_eq!((view.near_clip, view.far_clip, view.render_scale, view.samples), (50.0, 50.0, 100, 4));
        assert_eq!(view.lights[0].intensity, 1.0);
        assert_eq!(view.material.into_iter().collect::<Vec<_>>(), [("coatWeight".to_string(), MaterialValue::Number(1.0))]);
        assert!(view.disabled.is_empty());
        assert_eq!(View3d::default().sanitized(), View3d::default());
    }

    #[test]
    fn a_saved_view_from_before_these_settings_loads_with_their_defaults() {
        let view: View3d = serde_json::from_str(r#"{"scene": "torus", "yaw": 10.0, "showGrid": true}"#).unwrap();

        assert_eq!((view.scene.as_str(), view.yaw, view.show_grid), ("torus", 10.0, true));
        assert_eq!(View3d { scene: "rounded_cube".into(), yaw: 35.0, show_grid: false, ..view }, View3d::default());
    }
}
