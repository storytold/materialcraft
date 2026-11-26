use materialcraft_geom::view::{HeightTechnique, UNITS_PER_MESH_UNIT, View3d};

use crate::{Light, Look, Maps, Relief, Scene, Settings, VIEW_TRIANGLES};

/// The tessellation setting that subdivides the displaced mesh to [`VIEW_TRIANGLES`]
const DEFAULT_TESSELLATION: f32 = 4.0;

impl Settings {
    /// The render settings of a view, for one ray per pixel
    pub fn for_view(view: &View3d) -> Settings {
        Settings {
            yaw: view.yaw,
            pitch: view.pitch,
            distance: view.distance,
            target: view.target,
            tiling: view.tiling,
            uv_scale: view.uv_scale,
            exposure: view.exposure,
            env_rotation: view.env_rotation,
            normal_y_flip: view.normal_y_flip,
            supersample: 1,
            ao_rays: view.ambient_occlusion.min(View3d::MAX_AMBIENT_OCCLUSION),
            key_light: 0.0,
            transparency: view.transparency,
            alpha_view: view.alpha_view,
            fov: view.vertical_fov(),
            ortho: view.projection == materialcraft_geom::view::Projection::Orthographic,
            clip: (!view.auto_clip).then(|| [view.near_clip / UNITS_PER_MESH_UNIT, view.far_clip / UNITS_PER_MESH_UNIT]),
            background: view.background,
            background_color: view.background_color,
            env_light: view.env_enabled,
            ambient: if view.ambient.enabled { view.ambient.color } else { [0.0; 3] },
            lights: [Light::of(&view.lights[0]), Light::of(&view.lights[1])],
            shadows: view.shadows,
            backface_culling: view.backface_culling,
            wireframe: view.wireframe,
            channels: view.view_channels,
            color_space: view.color_space,
            look: Look::of(view),
            relief: Relief { depth: view.height_scale, level: view.height_level },
        }
    }
}

/// How far the view's height output moves the mesh, or `None` while the mesh stays as it is:
/// displacement is off, the shader shows no height or it shows it with parallax
pub fn relief_of(view: &View3d) -> Option<Relief> {
    let moves = view.displacement && view.shader.shows_height() && view.technique == HeightTechnique::Tessellation && view.uses("height");
    moves.then_some(Relief { depth: view.height_scale, level: view.height_level })
}

/// How often the material repeats over the mesh's UV square along U and V
pub fn uv_repeats(view: &View3d) -> [f32; 2] {
    view.uv_scale.map(|scale| scale * view.tiling)
}

/// `scene` raised by the view's height output and subdivided to at most `triangles`, or `None`
/// while nothing moves
pub fn raised(scene: &Scene, maps: &Maps, view: &View3d, triangles: usize) -> Option<Scene> {
    let relief = relief_of(view)?;

    // A normal map that is switched off leaves the relief to shade itself
    let shading = Maps { normal: maps.normal.clone().filter(|_| view.uses("geometryNormal")), ..maps.clone() };
    scene.with_relief(&shading, relief, uv_repeats(view), triangles)
}

/// The triangle budget of the view's displaced mesh
pub fn relief_triangles(view: &View3d) -> usize {
    let tessellation =
        if view.tessellation.is_finite() { view.tessellation.clamp(*View3d::TESSELLATION.start(), *View3d::TESSELLATION.end()) } else { DEFAULT_TESSELLATION };
    (VIEW_TRIANGLES as f32 * tessellation / DEFAULT_TESSELLATION) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use materialcraft_geom::view::{Background, Projection, Shader};

    #[test]
    fn the_camera_settings_follow_the_view() {
        let settings = Settings::for_view(&View3d::default());
        assert!((settings.fov - 37.85).abs() < 0.01, "35 mm on a 24 mm high frame: {}", settings.fov);
        assert_eq!((settings.ortho, settings.clip, settings.background), (false, None, Background::Studio));

        let view = View3d {
            focal_length: 12.0,
            projection: Projection::Orthographic,
            auto_clip: false,
            near_clip: 25.0,
            far_clip: 500.0,
            background: Background::Environment,
            ..View3d::default()
        };
        let settings = Settings::for_view(&view);
        assert!((settings.fov - 90.0).abs() < 0.01);
        assert_eq!((settings.ortho, settings.clip, settings.background), (true, Some([0.5, 10.0]), Background::Environment));

        // A broken focal length reads as the default lens
        assert!((Settings::for_view(&View3d { focal_length: f32::NAN, ..View3d::default() }).fov - 37.85).abs() < 0.01);
    }

    #[test]
    fn lights_shine_only_while_switched_on() {
        let mut view = View3d::default();
        assert_eq!(Settings::for_view(&view).lights, [None, None]);

        view.lights[0].enabled = true;
        view.lights[0].intensity = 2.0;
        view.ambient.color = [0.1, 0.2, 0.3];
        let settings = Settings::for_view(&view);
        assert_eq!(settings.lights[0].map(|light| light.color), Some([2.0; 3]));
        assert_eq!(settings.ambient, [0.1, 0.2, 0.3]);

        view.ambient.enabled = false;
        assert_eq!(Settings::for_view(&view).ambient, [0.0; 3]);
    }

    #[test]
    fn the_mesh_is_displaced_only_by_tessellation_of_a_shader_that_shows_height() {
        let view = View3d::default();
        assert_eq!(relief_of(&view), Some(Relief { depth: 0.04, level: 0.0 }));

        assert_eq!(relief_of(&View3d { displacement: false, ..view.clone() }), None);
        assert_eq!(relief_of(&View3d { technique: HeightTechnique::Parallax, ..view.clone() }), None);
        assert_eq!(relief_of(&View3d { shader: Shader::Lambert, ..view.clone() }), None);
        assert_eq!(relief_of(&View3d { disabled: ["height".to_string()].into(), ..view.clone() }), None);

        assert_eq!(uv_repeats(&View3d { tiling: 2.0, uv_scale: [1.0, 3.0], ..view.clone() }), [2.0, 6.0]);
        assert_eq!(relief_triangles(&view), VIEW_TRIANGLES);
        assert_eq!(relief_triangles(&View3d { tessellation: 1.0, ..view.clone() }), VIEW_TRIANGLES / 4);
        assert_eq!(relief_triangles(&View3d { tessellation: f32::NAN, ..view }), VIEW_TRIANGLES);
    }

    #[test]
    fn a_switched_off_normal_map_leaves_the_relief_to_shade_itself() {
        use craft_texture::Image;

        let scene = Scene::new(materialcraft_geom::primitives::plane(2.0, 1));
        let slope = Image::gray(8, 8, |x, _| x as f32 / 7.0);
        let maps = Maps { height: Some(slope), normal: Some(Image::gray(1, 1, |_, _| 0.5).to_color()), ..Maps::default() };
        let view = View3d { height_scale: 0.5, ..View3d::default() };
        let tilted = |scene: &Scene| scene.mesh.normals.iter().any(|normal| normal.x.abs() > 0.05);

        let with_map = raised(&scene, &maps, &view, 2048).map(|scene| tilted(&scene));
        assert_eq!(with_map, Some(false), "the normal map shades the relief, the mesh keeps its normals");

        let without_map = raised(&scene, &maps, &View3d { disabled: ["geometryNormal".to_string()].into(), ..view.clone() }, 2048).map(|scene| tilted(&scene));
        assert_eq!(without_map, Some(true), "the displaced surface's own normals show the slope");

        assert!(raised(&scene, &maps, &View3d { displacement: false, ..view }, 2048).is_none());
    }
}
