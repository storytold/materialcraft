//! Commands that consume the pixels or mesh captured from the displayed view

use serde_json::{Value as J, json};

#[cfg(not(target_arch = "wasm32"))]
use crate::commands::strp;
use crate::commands::{CommandSpec, always, bad};
use crate::{DisplaySnapshot, Result, Session, cmd};

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("view.3d.saveRender", "Save displayed render", [], None, "{path} writes the captured displayed RGBA frame as PNG", always, save_render),
        cmd!("view.3d.exportGeometry", "Export displayed geometry", [], None, "{path} writes the captured displayed mesh as OBJ", always, export_geometry),
        cmd!(
            "view.2d.copyImage",
            "Copy displayed image",
            [],
            None,
            "{} returns the captured displayed RGBA image to the UI clipboard handler",
            always,
            copy_image
        ),
    ]
}

fn take(session: &mut Session, command: &str) -> Result<DisplaySnapshot> {
    session.display_snapshot.take().ok_or_else(|| bad(command, "no displayed snapshot was supplied"))
}

fn copy_image(session: &mut Session, _: &J) -> Result<J> {
    const COMMAND: &str = "view.2d.copyImage";
    let snapshot = take(session, COMMAND)?;
    let DisplaySnapshot::Image { width, height, ref pixels, premultiplied } = snapshot else {
        return Err(bad(COMMAND, "a displayed 2D image is required"));
    };
    DisplaySnapshot::valid_rgba(COMMAND, width, height, pixels)?;

    session.display_output = Some(snapshot);
    Ok(json!({"width": width, "height": height, "premultiplied": premultiplied}))
}

#[cfg(not(target_arch = "wasm32"))]
fn save_render(session: &mut Session, parameters: &J) -> Result<J> {
    use image::ImageEncoder as _;

    const COMMAND: &str = "view.3d.saveRender";
    let snapshot = take(session, COMMAND)?;
    let DisplaySnapshot::Render { width, height, pixels, path } = snapshot else { return Err(bad(COMMAND, "a displayed 3D frame is required")) };
    let path = path
        .or_else(|| strp(parameters, "path").filter(|path| !path.is_empty()).map(std::path::PathBuf::from))
        .ok_or_else(|| bad(COMMAND, "`path` is required"))?;
    DisplaySnapshot::valid_rgba(COMMAND, width, height, &pixels)?;

    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&pixels, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|error| bad(COMMAND, format!("the render could not be encoded: {error}")))?;
    materialcraft_formats::write_atomic(&path, &png)?;
    Ok(json!({"path": path.display().to_string(), "width": width, "height": height}))
}

#[cfg(target_arch = "wasm32")]
fn save_render(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("view.3d.saveRender", "saving a render by path needs the desktop app"))
}

#[cfg(not(target_arch = "wasm32"))]
fn export_geometry(session: &mut Session, parameters: &J) -> Result<J> {
    const COMMAND: &str = "view.3d.exportGeometry";
    let snapshot = take(session, COMMAND)?;
    let DisplaySnapshot::Geometry { mesh, path } = snapshot else { return Err(bad(COMMAND, "a displayed 3D mesh is required")) };
    let path = path
        .or_else(|| strp(parameters, "path").filter(|path| !path.is_empty()).map(std::path::PathBuf::from))
        .ok_or_else(|| bad(COMMAND, "`path` is required"))?;
    DisplaySnapshot::valid_geometry(COMMAND, &mesh)?;

    let text = materialcraft_formats::mesh::obj_text(&mesh)?;
    materialcraft_formats::write_atomic(&path, text.as_bytes())?;
    Ok(json!({"path": path.display().to_string(), "triangles": mesh.triangle_count()}))
}

#[cfg(target_arch = "wasm32")]
fn export_geometry(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("view.3d.exportGeometry", "exporting geometry by path needs the desktop app"))
}
