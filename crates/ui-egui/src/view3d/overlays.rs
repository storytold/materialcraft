//! What the 3D view draws over its frame: the grid, the bounding box, the light helpers and the
//! vertex tangent space (cut where the mesh hides them), the axes with a scale, the scene
//! statistics, the resolution and the frame rate

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use egui::{Align2, Color32, FontId, Rect, Stroke, Ui, pos2, vec2};
use materialcraft_engine::geom::glam::Vec3;
use materialcraft_engine::geom::view::UNITS_PER_MESH_UNIT;

use super::overlay::{AXIS_COLORS, Overlay};
use super::sight::Sight;
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// Space between the view's edge and what is written or drawn in its corners
const MARGIN: f32 = 8.0;
const TEXT_SIZE: f32 = 11.0;
const LINE_HEIGHT: f32 = 15.0;
/// Length of an arm of the axes in the corner
const AXIS_ARM: f32 = 26.0;
/// The longest the scale bar beside the axes gets
const SCALE_BAR_LIMIT: f32 = 96.0;
/// Scene units in a meter
const UNITS_PER_METER: f32 = 100.0;
/// The lengths the scale bar can show, in meters
const SCALE_LENGTHS: [f32; 13] = [0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0];

/// Draw the view's helpers over the frame shown in `area`, inside `view_rect`
pub(super) fn paint(app: &mut MaterialCraftApp, ui: &Ui, view_rect: Rect, area: Rect) {
    let scene = super::frame::shown_scene(app);
    let view = &app.session.view3d;
    let sight = Sight::of(&scene, super::frame::shown_view(app), area.width() / area.height().max(1.0));
    let painter = ui.painter_at(view_rect.intersect(area));
    let place = |at: [f32; 2]| pos2(area.left() + at[0] * area.width(), area.top() + at[1] * area.height());

    if view.show_grid || view.bounding_box || view.light_helpers || view.vertex_tangent_space || app.view3d.selected {
        let key = overlay_key(app, &scene, sight.aspect);
        let overlay = match &app.view3d.overlay {
            Some((cached, overlay)) if *cached == key => overlay.clone(),
            _ => {
                let mut overlay_view = super::frame::shown_view(app).clone();
                overlay_view.show_grid = view.show_grid;
                overlay_view.bounding_box = view.bounding_box;
                overlay_view.light_helpers = view.light_helpers;
                overlay_view.vertex_tangent_space = view.vertex_tangent_space;
                Arc::new(Overlay::of(&sight, &overlay_view, app.view3d.selected))
            }
        };

        for line in &overlay.lines {
            painter.line_segment([place(line.from), place(line.to)], Stroke::new(line.width, line.color));
        }
        for mark in &overlay.marks {
            painter.rect_filled(Rect::from_center_size(place(mark.at), vec2(mark.side, mark.side)), 1.0, mark.color);
        }
        app.view3d.overlay = Some((key, overlay));
    } else {
        app.view3d.overlay = None;
    }

    let view = &app.session.view3d;
    let tokens = Tokens::get(ui.ctx());
    let painter = ui.painter_at(view_rect);
    if view.show_axis {
        axes(&painter, &sight, view_rect, area.height(), tokens.viewport_text);
    }
    if view.scene_stats {
        for (row, line) in statistics(app, &scene).into_iter().enumerate() {
            let at = view_rect.left_top() + vec2(MARGIN, MARGIN + row as f32 * LINE_HEIGHT);
            write(&painter, at, Align2::LEFT_TOP, &line, TEXT_SIZE, tokens.viewport_text);
        }
    }
    if view.show_resolution {
        let (width, height) = app
            .view3d
            .shown
            .as_ref()
            .map_or_else(|| super::frame::frame_pixels(view, view_rect, ui.ctx().pixels_per_point()), |(width, height, _)| (*width, *height));
        write(
            &painter,
            view_rect.right_bottom() - vec2(MARGIN, MARGIN),
            Align2::RIGHT_BOTTOM,
            &format!("{width} × {height} px"),
            TEXT_SIZE,
            tokens.viewport_text,
        );
    }
    if view.fps {
        let milliseconds = app.view3d.render_ms.max(0.0);
        let rate = if milliseconds > 0.0 { 1000.0 / milliseconds } else { 0.0 };
        let lines = [format!("{}: {rate:.1}", tr("Frames per second")), format!("{}: {milliseconds:.0} ms", tr("Render time"))];
        for (row, line) in lines.into_iter().enumerate() {
            let at = view_rect.right_top() + vec2(-MARGIN, MARGIN + row as f32 * LINE_HEIGHT);
            write(&painter, at, Align2::RIGHT_TOP, &line, TEXT_SIZE, tokens.viewport_text);
        }
    }
}

/// Text that reads on any picture: the letters over a dark copy of themselves
fn write(painter: &egui::Painter, at: egui::Pos2, anchor: Align2, text: &str, size: f32, color: Color32) {
    let font = FontId::proportional(size);
    painter.text(at + vec2(1.0, 1.0), anchor, text, font.clone(), Color32::from_black_alpha(0xc0));
    painter.text(at, anchor, text, font, color);
}

/// What the cached overlay was made for: the camera, the mesh and what is switched on
fn overlay_key(app: &MaterialCraftApp, scene: &Arc<materialcraft_render::Scene>, aspect: f32) -> u64 {
    let view = &app.session.view3d;
    let mut hasher = DefaultHasher::new();
    (Arc::as_ptr(scene) as usize, view.show_grid, view.bounding_box, view.light_helpers, view.vertex_tangent_space, app.view3d.selected).hash(&mut hasher);
    let view = super::frame::shown_view(app);
    (view.projection.id(), view.lights.map(|light| light.enabled)).hash(&mut hasher);

    let lights = view.lights.iter().flat_map(|light| [light.yaw, light.pitch, light.distance]);
    for number in [view.yaw, view.pitch, view.distance, view.focal_length, aspect].into_iter().chain(view.target).chain(lights) {
        number.to_bits().hash(&mut hasher);
    }
    hasher.finish()
}

/// The world axes as the camera sees them, in the view's lower left corner, with a bar that
/// shows how long a round length is where the camera orbits
fn axes(painter: &egui::Painter, sight: &Sight, view_rect: Rect, picture_height: f32, text: Color32) {
    let center = view_rect.left_bottom() + vec2(MARGIN + AXIS_ARM + 8.0, -(MARGIN + AXIS_ARM + 8.0));
    let rotation = sight.camera.view();
    let mut arms: Vec<(Vec3, Color32, &str)> = [Vec3::X, Vec3::Y, Vec3::Z]
        .into_iter()
        .zip(AXIS_COLORS)
        .zip(["X", "Y", "Z"])
        .map(|((axis, color), name)| (rotation.transform_vector3(axis), color, name))
        .collect();

    // The arm that points away from the camera is drawn first, under the others
    arms.sort_by(|a, b| a.0.z.total_cmp(&b.0.z));
    for (direction, color, name) in arms {
        let tip = center + vec2(direction.x, -direction.y) * AXIS_ARM;
        painter.line_segment([center, tip], Stroke::new(2.0, color));
        write(painter, tip + vec2(direction.x, -direction.y) * 7.0, Align2::CENTER_CENTER, name, 10.0, color);
    }

    let points_per_meter = sight.points_per_unit(picture_height) * UNITS_PER_METER / UNITS_PER_MESH_UNIT;
    let Some(meters) = SCALE_LENGTHS.into_iter().rev().find(|meters| meters * points_per_meter <= SCALE_BAR_LIMIT) else { return };

    let start = pos2(center.x + AXIS_ARM + 18.0, view_rect.bottom() - MARGIN - 4.0);
    let end = start + vec2(meters * points_per_meter, 0.0);

    // A dark line under the light one keeps the bar readable on a bright picture
    for (offset, stroke) in [(1.0, Stroke::new(1.5, Color32::from_black_alpha(0xc0))), (0.0, Stroke::new(1.5, text))] {
        let shift = vec2(offset, offset);
        painter.line_segment([start + shift, end + shift], stroke);
        for x in [start.x, end.x] {
            painter.line_segment([pos2(x, start.y - 4.0) + shift, pos2(x, start.y + 4.0) + shift], stroke);
        }
    }
    write(painter, pos2((start.x + end.x) * 0.5, start.y - 6.0), Align2::CENTER_BOTTOM, &length_label(meters), TEXT_SIZE, text);
}

/// A length in the unit that reads best: millimeters, centimeters or meters
fn length_label(meters: f32) -> String {
    if meters < 0.01 {
        format!("{:.0} mm", meters * 1000.0)
    } else if meters < 1.0 {
        format!("{:.0} cm", meters * 100.0)
    } else {
        format!("{meters:.0} m")
    }
}

/// The lines of the scene statistics: what the scene holds and how large it is
fn statistics(app: &MaterialCraftApp, scene: &materialcraft_render::Scene) -> Vec<String> {
    let mesh = &scene.mesh;
    let (low, high) = mesh.bounds();
    let size = ((high - low) * UNITS_PER_MESH_UNIT).to_array().map(|side| format!("{side:.0}"));

    vec![
        format!("{}: 1", tr("Meshes")),
        format!("{}: {}", tr("Triangles"), mesh.triangle_count()),
        format!("{}: {}", tr("Vertices"), mesh.vertex_count()),
        format!("{}: {}", tr("Materials"), mesh.materials.len().max(1)),
        format!("{}: {}", tr("Meters per scene unit"), 1.0 / UNITS_PER_METER),
        format!("{}: {}", tr("Bounding box"), size.join(" × ")),
        format!("{}: {}", tr("Selected"), u8::from(app.view3d.selected)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use materialcraft_engine::View3d;
    use materialcraft_render::Scene;

    fn overlay(view: &View3d, selected: bool) -> Overlay {
        let scene = Scene::builtin("cube");
        Overlay::of(&Sight::of(&scene, view, 1.5), view, selected)
    }

    #[test]
    fn each_helper_adds_its_own_lines() {
        let plain = View3d::default();
        assert!(overlay(&plain, false).lines.is_empty() && overlay(&plain, false).marks.is_empty());

        let grid = overlay(&View3d { show_grid: true, ..plain.clone() }, false);
        assert!(grid.lines.len() >= 83, "41 lines each way and the vertical axis: {}", grid.lines.len());

        let bounds = overlay(&View3d { bounding_box: true, ..plain.clone() }, false);
        assert!(bounds.lines.len() >= 9 && bounds.marks.len() >= 7, "{} edges, {} corners", bounds.lines.len(), bounds.marks.len());

        let mut lit = View3d { light_helpers: true, ..plain.clone() };
        assert!(overlay(&lit, false).marks.is_empty(), "a light that is off has no helper");
        lit.lights[0].enabled = true;
        assert_eq!(overlay(&lit, false).marks.len(), 1);

        let tangents = overlay(&View3d { vertex_tangent_space: true, ..plain.clone() }, false);
        assert!(tangents.lines.len() >= 30 && tangents.lines.len().is_multiple_of(3), "three lines a vertex: {}", tangents.lines.len());

        assert!(overlay(&plain, true).lines.len() >= 9, "the selection box");
    }

    #[test]
    fn the_mesh_hides_what_is_behind_it() {
        // Looking along Z at a cube: the grid line through its middle shows only beside it
        let view = View3d { show_grid: true, yaw: 0.0, pitch: 0.0, ..View3d::default() };
        let scene = Scene::builtin("cube");
        let sight = Sight::of(&scene, &view, 1.0);

        assert!(!sight.sees(Vec3::ZERO), "the middle of the cube");
        assert!(!sight.sees(Vec3::new(0.0, 0.0, -3.0)), "a point behind the cube");
        assert!(sight.sees(Vec3::new(0.0, 0.0, 3.0)) && sight.sees(Vec3::new(3.0, 0.0, 0.0)));

        let middle = sight.place(Vec3::ZERO).unwrap();
        assert!((middle[0] - 0.5).abs() < 1e-3 && (middle[1] - 0.5).abs() < 1e-3, "the orbit point is the middle of the frame");
        let ray = sight.ray(middle);
        assert!(scene.bvh.closest(ray.origin, ray.dir, f32::MAX).is_some(), "the ray through the middle hits the cube");
    }

    #[test]
    fn the_scale_reads_in_the_unit_that_fits() {
        assert_eq!(length_label(0.005), "5 mm");
        assert_eq!(length_label(0.5), "50 cm");
        assert_eq!(length_label(2.0), "2 m");
    }
}
