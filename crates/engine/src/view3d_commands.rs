//! Commands of the 3D view's menus beyond `view.3d`: viewpoints, framing, resets and state files.

use serde_json::Value as J;
#[cfg(not(target_arch = "wasm32"))]
use serde_json::json;

use crate::commands::{CommandSpec, always, bad, boolp, f32p, strp};
use crate::{Result, Session, Viewpoint, cmd};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "view.3d.viewpoint",
            "Viewpoint",
            [],
            None,
            "{name: default|front|right|back|left|top|bottom} turns the camera to look at the mesh from there, at the same distance",
            always,
            viewpoint
        ),
        cmd!(
            "view.3d.frame",
            "Frame",
            [],
            None,
            "{aspect? (viewport width / height, default 1)} centers the mesh and moves the camera to where all of it is in view",
            always,
            |session, parameters| {
                session.view3d.target = [0.0; 3];
                session.view3d.distance = session.view3d.framing_distance_for_aspect(f32p(parameters, "aspect").unwrap_or(1.0));
                Ok(view(session))
            }
        ),
        cmd!(
            "view.3d.resetCamera",
            "Reset Position",
            [],
            None,
            "{properties? (also the lens, projection, background, resolution and clip planes)} puts the camera back where a new view has it",
            always,
            |session, parameters| {
                let properties = boolp(parameters, "properties").unwrap_or(false);
                session.view3d = if properties { session.view3d.with_default_camera() } else { session.view3d.with_default_position() };
                Ok(view(session))
            }
        ),
        cmd!("view.3d.resetLights", "Reset Lights", [], None, "{} the ambient light on and black, the point lights off", always, |session, _| {
            session.view3d = session.view3d.with_default_lights();
            Ok(view(session))
        }),
        cmd!("view.3d.resetEnvironment", "Reset Environment", [], None, "{} the first environment, on, unturned, at exposure 0", always, |session, _| {
            session.view3d = session.view3d.with_default_environment();
            Ok(view(session))
        }),
        cmd!(
            "view.3d.resetMaterial",
            "Reset Material",
            [],
            None,
            "{common? (only the parameters several shaders share; the shader stays)} puts the material model back to its defaults; the assigned images stay",
            always,
            |session, parameters| {
                session.view3d = session.view3d.with_default_material(boolp(parameters, "common").unwrap_or(false));
                Ok(view(session))
            }
        ),
        cmd!(
            "view.3d.saveDefault",
            "Save Current State As Default",
            [],
            None,
            "{} makes the current 3D view the one a reset goes back to",
            always,
            |session, _| {
                session.default_view3d = Some(session.view3d.clone());
                Ok(view(session))
            }
        ),
        cmd!("view.3d.saveState", "Save Current State...", [], None, "{path} writes the 3D view's settings to a state file", always, save_state),
        cmd!(
            "view.3d.loadState",
            "Load State File...",
            [],
            None,
            "{path, mesh? (also switch to the mesh the state was saved with)} applies a saved 3D view state",
            always,
            load_state
        ),
    ]
}

fn view(session: &Session) -> J {
    serde_json::to_value(&session.view3d).unwrap_or(J::Null)
}

fn viewpoint(session: &mut Session, parameters: &J) -> Result<J> {
    const COMMAND: &str = "view.3d.viewpoint";
    let name = strp(parameters, "name").ok_or_else(|| bad(COMMAND, "`name` is required"))?;
    let viewpoint = Viewpoint::of(name).ok_or_else(|| bad(COMMAND, format!("unknown viewpoint `{name}`")))?;

    session.view3d.yaw = viewpoint.yaw;
    session.view3d.pitch = viewpoint.pitch;
    session.view3d.target = [0.0; 3];
    Ok(view(session))
}

#[cfg(not(target_arch = "wasm32"))]
fn save_state(session: &mut Session, parameters: &J) -> Result<J> {
    const COMMAND: &str = "view.3d.saveState";
    let path = strp(parameters, "path").ok_or_else(|| bad(COMMAND, "`path` is required"))?;
    let text =
        serde_json::to_string_pretty(&crate::view3d_state_file::View3dStateFile::of(&session.view3d)).map_err(|error| bad(COMMAND, error.to_string()))?;

    std::fs::write(path, text).map_err(|error| bad(COMMAND, format!("{path}: {error}")))?;
    Ok(json!({ "path": path }))
}

#[cfg(not(target_arch = "wasm32"))]
fn load_state(session: &mut Session, parameters: &J) -> Result<J> {
    use crate::view3d_state_file::View3dStateFile;
    use std::io::Read;

    const COMMAND: &str = "view.3d.loadState";
    let path = strp(parameters, "path").ok_or_else(|| bad(COMMAND, "`path` is required"))?;
    let failed = |reason: String| bad(COMMAND, format!("{path}: {reason}"));

    // Opening a named pipe may wait indefinitely for a writer
    if !std::fs::metadata(path).map_err(|error| failed(error.to_string()))?.is_file() {
        return Err(failed("not a regular file".into()));
    }

    // Read no more than a state can be, so a wrong file cannot fill the memory
    let mut text = String::new();
    let file = std::fs::File::open(path).map_err(|error| failed(error.to_string()))?;
    file.take(View3dStateFile::MAX_BYTES + 1).read_to_string(&mut text).map_err(|error| failed(error.to_string()))?;
    if text.len() as u64 > View3dStateFile::MAX_BYTES {
        return Err(failed("too large for a 3D view state".into()));
    }

    let state = View3dStateFile::parse(&text).map_err(failed)?;

    // The mesh changes only when asked for, and only to one this session can show
    let with_mesh = boolp(parameters, "mesh").unwrap_or(false);
    let known = crate::view_cmds::SCENES.iter().any(|(id, _)| *id == state.view.scene) || (state.view.scene == "custom" && session.custom_mesh.is_some());
    let scene = if with_mesh && known { state.view.scene.clone() } else { session.view3d.scene.clone() };

    // A file can hold anything: its numbers are put in range before they are used
    let mut loaded = state.view.sanitized();
    loaded.scene = scene;
    loaded.target = session.camera.clamp_target(loaded.target);
    if !crate::view_cmds::ENVIRONMENTS.iter().any(|(id, _)| *id == loaded.environment) {
        loaded.environment = session.view3d.environment.clone();
    }
    session.view3d = loaded;
    Ok(view(session))
}

#[cfg(target_arch = "wasm32")]
fn save_state(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("view.3d.saveState", "the browser has no files to save a state to"))
}

#[cfg(target_arch = "wasm32")]
fn load_state(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("view.3d.loadState", "the browser has no files to load a state from"))
}
