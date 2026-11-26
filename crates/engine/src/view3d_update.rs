//! `view.3d`: the view after the given parameters are applied. Everything is checked before
//! anything changes, so a rejected parameter leaves the view as it was.

use materialcraft_geom::view::{
    AmbientLight, Background, ColorSpace, HeightTechnique, MaterialParameter, MaterialValue, MeshInfo, PointLight, Projection, Shader, View3d, ViewChannels,
};
use serde_json::Value as J;

use crate::commands::{bad, boolp, f32p, strp, u64p};
use crate::view_cmds::{ENVIRONMENTS, SCENES};
use crate::{EngineError, Result, Session};

const COMMAND: &str = "view.3d";

fn rejected(message: impl Into<String>) -> EngineError {
    bad(COMMAND, message)
}

/// `[a, b, ...]` of exactly `N` finite numbers under `key`; `None` when the key is absent
fn numbers<const N: usize>(parameters: &J, key: &str) -> Result<Option<[f32; N]>> {
    let Some(value) = parameters.get(key) else { return Ok(None) };
    let listed: Option<Vec<f32>> = value
        .as_array()
        .filter(|items| items.len() == N)
        .and_then(|items| items.iter().map(|item| item.as_f64().map(|number| number as f32).filter(|number| number.is_finite())).collect());

    listed.and_then(|numbers| <[f32; N]>::try_from(numbers).ok()).map(Some).ok_or_else(|| rejected(format!("`{key}` is a list of {N} numbers")))
}

/// A linear RGB color under `key`, each channel held within 0..1
fn color(parameters: &J, key: &str) -> Result<Option<[f32; 3]>> {
    Ok(numbers::<3>(parameters, key)?.map(|channels| channels.map(|channel| channel.clamp(0.0, 1.0))))
}

/// One of a list of named choices under `key`
fn choice<T: Copy>(parameters: &J, key: &str, from_id: fn(&str) -> Option<T>) -> Result<Option<T>> {
    let Some(value) = parameters.get(key) else { return Ok(None) };
    value.as_str().and_then(from_id).map(Some).ok_or_else(|| rejected(format!("unknown {key} `{value}`")))
}

/// The object under `key`
fn object<'a>(parameters: &'a J, key: &str) -> Result<Option<&'a serde_json::Map<String, J>>> {
    match parameters.get(key) {
        None => Ok(None),
        Some(value) => value.as_object().map(Some).ok_or_else(|| rejected(format!("`{key}` is an object"))),
    }
}

fn point_light(parameters: &J, key: &str, light: PointLight) -> Result<PointLight> {
    if object(parameters, key)?.is_none() {
        return Ok(light);
    }

    let given = &parameters[key];

    Ok(PointLight {
        enabled: boolp(given, "enabled").unwrap_or(light.enabled),
        color: color(given, "color")?.unwrap_or(light.color),
        intensity: f32p(given, "intensity").unwrap_or(light.intensity),
        yaw: f32p(given, "yaw").unwrap_or(light.yaw),
        pitch: f32p(given, "pitch").unwrap_or(light.pitch),
        distance: f32p(given, "distance").unwrap_or(light.distance),
    }
    .sanitized())
}

/// A material parameter's value as given: a number, a color or a switch of the kind the parameter holds
fn material_value(parameter: &MaterialParameter, given: &J) -> Result<MaterialValue> {
    let value = match given {
        J::Bool(on) => Some(MaterialValue::Toggle(*on)),
        J::Number(number) => number.as_f64().map(|number| MaterialValue::Number(number as f32)),
        J::Array(_) => numbers::<3>(&serde_json::json!({ "color": given }), "color").ok().flatten().map(MaterialValue::Color),
        _ => None,
    };

    value.and_then(|value| parameter.kind.accepted(value)).ok_or_else(|| rejected(format!("`{}` does not take {given}", parameter.id)))
}

fn known(id: &str) -> Result<&'static MaterialParameter> {
    MaterialParameter::of(id).ok_or_else(|| rejected(format!("unknown material parameter `{id}`")))
}

pub(crate) fn updated(session: &Session, parameters: &J) -> Result<View3d> {
    let mut view = session.view3d.clone();

    if let Some(scene) = strp(parameters, "scene") {
        if scene == "custom" && session.custom_mesh.is_none() {
            return Err(rejected("no mesh is loaded: use view.3d.loadMesh"));
        }

        if scene != "custom" && !SCENES.iter().any(|(id, _)| *id == scene) {
            return Err(rejected(format!("unknown scene `{scene}`")));
        }
        view.scene = scene.into();
    }
    if let Some(environment) = strp(parameters, "environment") {
        if !ENVIRONMENTS.iter().any(|(id, _)| *id == environment) {
            return Err(rejected(format!("unknown environment `{environment}`")));
        }
        view.environment = environment.into();
    }
    if parameters.get("target").is_some() {
        let target = numbers::<3>(parameters, "target").map_err(|_| rejected("`target` is [x, y, z]"))?;
        view.target = session.camera.clamp_target(target.unwrap_or(view.target));
    }

    // Numbers are held in their ranges
    let number = |key: &str, current: f32, range: std::ops::RangeInclusive<f32>| {
        f32p(parameters, key).map_or(current, |given| given.clamp(*range.start(), *range.end()))
    };
    view.tiling = number("tiling", view.tiling, View3d::REPEATS);
    view.pitch = number("pitch", view.pitch, -89.0..=89.0);
    view.distance = number("distance", view.distance, View3d::DISTANCE);
    view.exposure = number("exposure", view.exposure, View3d::EXPOSURE);
    view.height_scale = number("heightScale", view.height_scale, View3d::HEIGHT_SCALE);
    view.height_level = number("heightLevel", view.height_level, 0.0..=1.0);
    view.focal_length = number("focalLength", view.focal_length, View3d::FOCAL_LENGTH);
    view.tessellation = number("tessellation", view.tessellation, View3d::TESSELLATION);
    view.near_clip = number("nearClip", view.near_clip, View3d::CLIP);
    view.far_clip = number("farClip", view.far_clip, View3d::CLIP).max(view.near_clip);
    view.yaw = f32p(parameters, "yaw").map_or(view.yaw, |yaw| yaw.rem_euclid(360.0));
    view.env_rotation = f32p(parameters, "envRotation").map_or(view.env_rotation, |rotation| rotation.rem_euclid(360.0));
    view.samples = u64p(parameters, "samples").map_or(view.samples, |samples| samples.clamp(1, u64::from(View3d::MAX_SAMPLES)) as u32);
    view.ambient_occlusion =
        u64p(parameters, "ambientOcclusion").map_or(view.ambient_occlusion, |rays| rays.min(u64::from(View3d::MAX_AMBIENT_OCCLUSION)) as u32);
    if let Some(scale) = u64p(parameters, "renderScale") {
        view.render_scale =
            View3d::RENDER_SCALES.into_iter().find(|allowed| u64::from(*allowed) == scale).ok_or_else(|| rejected("`renderScale` is 25, 50, 100 or 200"))?;
    }
    if let Some(resolution) = numbers::<2>(parameters, "renderResolution")? {
        view.render_resolution = resolution.map(|side| (side.round() as u32).clamp(View3d::MIN_RENDER_SIDE, View3d::MAX_RENDER_SIDE));
    }
    if let Some(scale) = numbers::<2>(parameters, "uvScale")? {
        view.uv_scale = scale.map(|repeats| repeats.clamp(*View3d::REPEATS.start(), *View3d::REPEATS.end()));
    }
    view.background_color = color(parameters, "backgroundColor")?.unwrap_or(view.background_color);

    for (key, flag) in [
        ("normalYFlip", &mut view.normal_y_flip),
        ("axis", &mut view.show_axis),
        ("grid", &mut view.show_grid),
        ("transparency", &mut view.transparency),
        ("alphaView", &mut view.alpha_view),
        ("overrideResolution", &mut view.override_resolution),
        ("viewportResolution", &mut view.viewport_resolution),
        ("autoClip", &mut view.auto_clip),
        ("envEnabled", &mut view.env_enabled),
        ("wireframe", &mut view.wireframe),
        ("backfaceCulling", &mut view.backface_culling),
        ("boundingBox", &mut view.bounding_box),
        ("lightHelpers", &mut view.light_helpers),
        ("vertexTangentSpace", &mut view.vertex_tangent_space),
        ("resolution", &mut view.show_resolution),
        ("sceneStats", &mut view.scene_stats),
        ("fps", &mut view.fps),
        ("physicalSize", &mut view.physical_size),
        ("shadows", &mut view.shadows),
        ("displacement", &mut view.displacement),
    ] {
        *flag = boolp(parameters, key).unwrap_or(*flag);
    }

    view.projection = choice(parameters, "projection", Projection::from_id)?.unwrap_or(view.projection);
    view.background = choice(parameters, "background", Background::from_id)?.unwrap_or(view.background);
    view.shader = choice(parameters, "shader", Shader::from_id)?.unwrap_or(view.shader);
    view.technique = choice(parameters, "technique", HeightTechnique::from_id)?.unwrap_or(view.technique);
    view.mesh_info = choice(parameters, "meshInfo", MeshInfo::from_id)?.unwrap_or(view.mesh_info);
    view.view_channels = choice(parameters, "viewChannels", ViewChannels::from_id)?.unwrap_or(view.view_channels);
    view.color_space = choice(parameters, "colorSpace", ColorSpace::from_id)?.unwrap_or(view.color_space);

    if object(parameters, "ambient")?.is_some() {
        let given = &parameters["ambient"];
        view.ambient =
            AmbientLight { enabled: boolp(given, "enabled").unwrap_or(view.ambient.enabled), color: color(given, "color")?.unwrap_or(view.ambient.color) };
    }
    view.lights = [point_light(parameters, "light1", view.lights[0])?, point_light(parameters, "light2", view.lights[1])?];

    for (id, given) in object(parameters, "material")?.into_iter().flatten() {
        let parameter = known(id)?;
        if given.is_null() {
            view.material.remove(id);
            continue;
        }

        // Only what differs from the default is kept
        let value = material_value(parameter, given)?;
        if value == parameter.kind.default() {
            view.material.remove(id);
        } else {
            view.material.insert(id.clone(), value);
        }
    }
    for (id, given) in object(parameters, "channels")?.into_iter().flatten() {
        known(id)?;
        match given.as_bool() {
            Some(true) => view.disabled.remove(id),
            Some(false) => view.disabled.insert(id.clone()),
            None => return Err(rejected(format!("`channels.{id}` is true or false"))),
        };
    }

    Ok(view)
}
