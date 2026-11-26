//! View commands: the 2D and 3D views, the preview size, the library listing and inspection.

use serde_json::{Value as J, json};

use materialcraft_core::NodeId;
use materialcraft_geom::view::View3d;

use crate::commands::{CommandSpec, always, bad, has_graph, strp, u64p};
use crate::{Result, Session, cmd};

/// Built-in 3D view scenes: (id, label), the plain shapes first.
pub const SCENES: &[(&str, &str)] = &[
    ("cube", "Cube"),
    ("cylinder", "Cylinder"),
    ("hollow_box", "Hollow box"),
    ("inner_box", "Inner box"),
    ("plane", "Plane"),
    ("plane_hi_res", "Plane (hi-res)"),
    ("sphere", "Sphere"),
    ("cloth", "Cloth"),
    ("material_ball", "Mat ball"),
    ("rounded_cube", "Rounded cube"),
    ("rounded_cylinder", "Rounded cylinder"),
    ("sphere_2_tiles", "Sphere 2 tiles"),
    ("torus", "Torus"),
];

/// How many of [`SCENES`] are the plain shapes; the menu draws a line after them
pub const PLAIN_SCENES: usize = 7;

/// Environments: (id, label).
pub const ENVIRONMENTS: &[(&str, &str)] = &[("studio", "Studio"), ("sunset", "Sunset"), ("overcast", "Overcast"), ("night", "Night")];

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "view.2d",
            "View output in 2D view",
            [],
            None,
            "{node?, port?, channel? (rgba|r|g|b|a), tiled? (repeat the image across the view), checkerboard? (behind transparent pixels), premultiplied? (colors already multiplied by alpha), srgb? (values are sRGB encoded; false shows them as linear), physicalSize? (proportions of the graph's physical size), maintainZoom? (keep the zoom when the image size changes), uv? (overlay the 3D mesh's UV layout), information? (pixel readout panel), sampler? ([x, y] in 0..1 from the top-left corner, or null), histogram?, histogramChannel? (luminance|red|green|blue|rgb|alpha), histogramBits? (8|16|32, 0 follows the image), histogramSqrt?, histogramSelection? ([low, high] in 0..1, or null), background? (image file path drawn behind transparent pixels, or null)}",
            has_graph,
            view2d
        ),
        cmd!(
            "view.3d",
            "3D view settings",
            [],
            None,
            "{scene? (cube|cylinder|hollow_box|inner_box|plane|plane_hi_res|sphere|cloth|material_ball|rounded_cube|rounded_cylinder|sphere_2_tiles|torus|custom), tiling?, uvScale? ([u, v]), physicalSize? (size the material by the graph's physical size), yaw?, pitch?, distance?, target? ([x, y, z] orbit point in mesh radii from its center, within the pan limit preference), focalLength? (mm, 0.01..100), projection? (perspective|orthographic), background? (studio|color|environment; studio is a gray backdrop that darkens toward the corners), backgroundColor? ([r, g, b] linear), overrideResolution?, viewportResolution?, renderResolution? ([width, height]), renderScale? (25|50|100|200 percent), autoClip?, nearClip?, farClip? (scene units; a mesh is 100 across), environment? (studio|sunset|overcast|night), envEnabled?, envRotation?, exposure? (-10..10 stops), ambient? ({enabled?, color?}), light1?, light2? ({enabled?, color?, intensity?, yaw?, pitch?, distance?}), wireframe?, axis?, backfaceCulling?, boundingBox?, grid?, lightHelpers?, vertexTangentSpace?, resolution?, sceneStats?, fps?, shader? (openPbr|blinn|lambert|meshInfo|metallicRoughness|metallicRoughnessCoated|metallicRoughnessSss|specularGlossiness|unlit), technique? (tessellation|parallax), meshInfo? (normal|tangent|binormal|uv|uvTile|vertexColor|position), material? ({parameter: number|[r, g, b]|bool, or null for its default}), channels? ({parameter: bool} switches a parameter and its image on or off), heightScale? (displacement depth in mesh radii, 0..0.5), heightLevel? (height that stays on the surface, 0..1; 0 = rises out), tessellation? (1..8), normalYFlip? (DirectX normal maps; default false/OpenGL), samples? (1..4), ambientOcclusion? (rays, 0..32), shadows?, displacement?, viewChannels? (rgb|r|g|b), colorSpace? (aces|srgb), transparency? (see through where alpha < 1), alphaView? (show the alpha channel)}",
            always,
            view3d
        ),
        cmd!("view.3d.loadMesh", "Load mesh...", [], None, "{path} shows an OBJ, glTF or GLB mesh in the 3D view (scene \"custom\")", always, load_mesh),
        cmd!("view.3d.reset", "Reset scene", [], None, "{} back to the saved default view, or the built-in one", always, |s, _| {
            let mut view = s.default_view3d.clone().unwrap_or_default();
            if view.scene == "custom" && s.custom_mesh.is_none() {
                view.scene = View3d::default().scene;
            }
            s.view3d = view;
            Ok(J::Null)
        }),
        cmd!("view.previewSize", "Preview size", [], None, "{size: 64..8192, or 0 for full size} caps node outputs while editing", always, preview_size),
        cmd!(
            "library.list",
            "Library",
            [],
            None,
            "{filter?, category?} every node definition: id, label, category, inputs, outputs, parameters",
            always,
            library
        ),
        cmd!(
            "session.inspect",
            "Inspect",
            [],
            None,
            "{graph?} the package, the open graph (nodes, wires, parameters, errors, timings), views and history",
            always,
            |s, p| { Ok(crate::inspect::session(s, u64p(p, "graph").map(|g| g as usize))) }
        ),
    ]
}

fn view2d(session: &mut Session, params: &J) -> Result<J> {
    let node = match u64p(params, "node") {
        Some(id) => {
            session.graph()?.node(NodeId(id)).map_err(|_| bad("view.2d", format!("no node {id}")))?;
            Some(NodeId(id))
        }
        None => None,
    };

    // The settings are replaced as a whole, so a rejected parameter leaves all of them unchanged
    session.view2d = session.view2d.updated(node, params)?;
    Ok(serde_json::to_value(&session.view2d).unwrap_or(J::Null))
}

fn view3d(session: &mut Session, parameters: &J) -> Result<J> {
    // The settings are replaced as a whole, so a rejected parameter leaves all of them unchanged
    session.view3d = crate::view3d_update::updated(session, parameters)?;
    Ok(serde_json::to_value(&session.view3d).unwrap_or(J::Null))
}

#[cfg(not(target_arch = "wasm32"))]
fn load_mesh(s: &mut Session, p: &J) -> Result<J> {
    let path = strp(p, "path").ok_or_else(|| bad("view.3d.loadMesh", "`path` is required"))?;

    // Opening a named pipe may wait indefinitely for a writer
    if !std::fs::metadata(path).map_err(|error| bad("view.3d.loadMesh", format!("{path}: {error}")))?.is_file() {
        return Err(bad("view.3d.loadMesh", format!("{path}: not a regular file")));
    }

    let bytes = std::fs::read(path).map_err(|e| bad("view.3d.loadMesh", format!("{path}: {e}")))?;
    let name = std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path).to_string();
    s.load_mesh_bytes(&name, &bytes)?;
    let tris = s.custom_mesh.as_ref().map_or(0, |(_, m)| m.triangle_count());
    Ok(json!({"name": name, "triangles": tris}))
}

#[cfg(target_arch = "wasm32")]
fn load_mesh(_: &mut Session, _: &J) -> Result<J> {
    Err(bad("view.3d.loadMesh", "the browser loads meshes with the file picker"))
}

fn preview_size(s: &mut Session, p: &J) -> Result<J> {
    let size = u64p(p, "size").ok_or_else(|| bad("view.previewSize", "`size` is required"))?;
    s.preview_size = if size == 0 { None } else { Some(size.clamp(64, 8192) as u32) };
    s.dirty = true;
    Ok(json!({"previewSize": s.preview_size}))
}

fn library(_: &mut Session, p: &J) -> Result<J> {
    let f = strp(p, "filter").map(str::to_ascii_lowercase);
    let cat = strp(p, "category").map(str::to_ascii_lowercase);
    Ok(J::Array(
        materialcraft_nodes::catalog()
            .iter()
            .filter(|d| f.as_ref().is_none_or(|f| d.id.contains(f.as_str()) || d.label.to_ascii_lowercase().contains(f.as_str())))
            .filter(|d| cat.as_ref().is_none_or(|c| d.category.to_ascii_lowercase().contains(c.as_str())))
            .map(|d| {
                json!({
                    "id": d.id, "label": d.label, "category": d.category, "description": d.description,
                    "inputs": d.inputs.iter().map(|p| json!({"id": p.id, "label": p.label, "kind": format!("{:?}", p.kind)})).collect::<Vec<_>>(),
                    "outputs": d.outputs.iter().map(|p| json!({"id": p.id, "label": p.label, "kind": format!("{:?}", p.kind)})).collect::<Vec<_>>(),
                    "params": d.params.iter().map(|p| json!({"id": p.id, "label": p.label, "default": p.default, "min": p.min, "max": p.max, "options": p.options})).collect::<Vec<_>>(),
                })
            })
            .collect(),
    ))
}
