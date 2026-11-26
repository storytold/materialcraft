//! The UV layout of the 3D view's mesh, drawn over the image

use std::collections::HashSet;
use std::sync::Arc;

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};

use crate::MaterialCraftApp;

/// The most edges drawn; a denser mesh shows every nth triangle so a frame stays cheap
const MAX_EDGES: usize = 40_000;
/// How opaque an edge is, so a dense layout still shows the image under it
const EDGE_OPACITY: f32 = 0.4;

/// The UV edges of the mesh the 3D view shows, as fractions of the image, built once per mesh
pub(super) fn edges(app: &mut MaterialCraftApp) -> Arc<Vec<[Pos2; 2]>> {
    let scene = app.session.view3d.scene.clone();
    let custom = app.session.custom_mesh.as_ref().filter(|_| scene == "custom");
    let key = match custom {
        Some((name, _)) => format!("custom:{name}:{}", app.session.custom_mesh_generation),
        None => scene.clone(),
    };
    if let Some((cached, edges)) = &app.view2d.uv_edges
        && *cached == key
    {
        return edges.clone();
    }

    let edges = Arc::new(match custom {
        Some((_, mesh)) => triangle_edges(|corner| mesh.uvs.get(corner).map(|uv| [uv.x, uv.y]), &mesh.indices),
        None => {
            let mesh = materialcraft_render::builtin_mesh(&scene);
            triangle_edges(|corner| mesh.uvs.get(corner).map(|uv| [uv.x, uv.y]), &mesh.indices)
        }
    });
    app.view2d.uv_edges = Some((key, edges.clone()));
    edges
}

/// Each triangle edge once, by its two corners in UV space. `uv` gives the UV of a corner
fn triangle_edges(uv: impl Fn(usize) -> Option<[f32; 2]>, indices: &[u32]) -> Vec<[Pos2; 2]> {
    let triangles = indices.len() / 3;
    let stride = (triangles * 3).div_ceil(MAX_EDGES).max(1);
    let corner = |index: u32| uv(index as usize).filter(|uv| uv.iter().all(|coordinate| coordinate.is_finite())).map(|[u, v]| pos2(u, v));
    let mut seen = HashSet::new();
    let mut edges = Vec::new();

    for triangle in indices.as_chunks::<3>().0.iter().step_by(stride) {
        for (from, to) in [(triangle[0], triangle[1]), (triangle[1], triangle[2]), (triangle[2], triangle[0])] {
            let (Some(start), Some(end)) = (corner(from), corner(to)) else { continue };

            if seen.insert((from.min(to), from.max(to))) {
                edges.push([start, end]);
            }
        }
    }
    edges
}

/// Draw `edges` over the image `tile`, leaving out the ones outside `clip`
pub(super) fn paint(painter: &Painter, edges: &[[Pos2; 2]], tile: Rect, clip: Rect, color: Color32) {
    let on_screen = |uv: Pos2| pos2(tile.left() + uv.x * tile.width(), tile.top() + uv.y * tile.height());
    let stroke = Stroke::new(1.0, color.gamma_multiply(EDGE_OPACITY));
    let lines = edges.iter().filter_map(|[start, end]| {
        let (start, end) = (on_screen(*start), on_screen(*end));
        Rect::from_two_pos(start, end).intersects(clip).then(|| Shape::line_segment([start, end], stroke))
    });

    painter.extend(lines);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reloading_a_mesh_with_new_uvs_rebuilds_the_overlay() {
        let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), crate::UiState::default());
        let first = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nf 1/1 2/2 3/3\n";
        let second = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 0.5 0\nvt 0 0.5\nf 1/1 2/2 3/3\n";

        app.session.load_mesh_bytes("same.obj", first).unwrap();
        let old_edges = edges(&mut app);
        let old_edges_weak = Arc::downgrade(&old_edges);
        app.session.load_mesh_bytes("same.obj", second).unwrap();
        let new_edges = edges(&mut app);

        assert!(!Arc::ptr_eq(&old_edges, &new_edges));
        assert_ne!(*old_edges, *new_edges);
        drop(old_edges);
        assert!(old_edges_weak.upgrade().is_none());
    }

    #[test]
    fn a_shared_edge_is_listed_once_and_bad_corners_are_left_out() {
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [f32::NAN, 0.5]];
        let uv = |corner: usize| uvs.get(corner).copied();

        // Two triangles of a square share its diagonal
        let edges = triangle_edges(uv, &[0, 1, 2, 0, 2, 3]);
        assert_eq!(edges.len(), 5);
        assert!(edges.contains(&[pos2(0.0, 0.0), pos2(1.0, 0.0)]));

        // A non-finite corner and an index past the corners draw nothing
        assert_eq!(triangle_edges(uv, &[0, 1, 4, 0, 1, 99]).len(), 1);
    }

    #[test]
    fn a_dense_mesh_is_thinned_to_the_edge_budget() {
        let indices: Vec<u32> = (0..300_000).collect();

        let edges = triangle_edges(|corner| Some([corner as f32, 0.0]), &indices);

        assert!(edges.len() <= MAX_EDGES && edges.len() > MAX_EDGES / 2, "{}", edges.len());
    }
}
