use egui::Color32;
use materialcraft_engine::View3d;
use materialcraft_engine::geom::glam::Vec3;
use materialcraft_engine::geom::view::PointLight;

use super::overlay_line::OverlayLine;
use super::overlay_mark::OverlayMark;
use super::sight::Sight;

/// Half the side of the grid, in mesh units: two meters each way
const GRID_REACH: f32 = 4.0;
/// Grid lines each way from the middle one, ten centimeters apart
const GRID_LINES: i32 = 20;
/// Every this many lines one is drawn stronger: half a meter
const GRID_MAJOR: i32 = 5;
/// How opaque a grid line is in the middle of the grid, and how much of that is left at its edge
const GRID_OPACITY: f32 = 96.0;
const GRID_EDGE_SHARE: f32 = 0.2;
/// The pieces a line is cut in to find where the mesh hides it
const PIECES: usize = 32;
/// The most vertices whose tangent space is drawn, spread evenly over the mesh
const TANGENT_VERTICES: usize = 6000;
/// Length of a tangent space line, in scene radii
const TANGENT_LENGTH: f32 = 0.04;
/// How much larger than the mesh the selection box is, so the bounding box shows inside it
const SELECTION_MARGIN: f32 = 1.03;

const AXIS_X: Color32 = Color32::from_rgb(0xe0, 0x52, 0x52);
const AXIS_Y: Color32 = Color32::from_rgb(0x62, 0xc4, 0x62);
const AXIS_Z: Color32 = Color32::from_rgb(0x52, 0x84, 0xea);
const BOUNDS: Color32 = Color32::from_rgb(0x7f, 0xc8, 0xff);
/// The lines of a vertex's tangent space
const TANGENT: Color32 = AXIS_X;
const BINORMAL: Color32 = AXIS_Z;
const NORMAL: Color32 = AXIS_Y;
const SELECTION: Color32 = Color32::from_rgb(0xff, 0x9a, 0x2e);
const LIGHT: Color32 = Color32::WHITE;

/// The colors of the world axes, X, Y and Z
pub(super) const AXIS_COLORS: [Color32; 3] = [AXIS_X, AXIS_Y, AXIS_Z];

/// What the 3D view draws over its frame for one camera position: the parts of the grid, the
/// bounding box, the light helpers and the tangent space the mesh does not hide
#[derive(Default)]
pub(super) struct Overlay {
    pub lines: Vec<OverlayLine>,
    pub marks: Vec<OverlayMark>,
}

impl Overlay {
    pub fn of(sight: &Sight, view: &View3d, selected: bool) -> Overlay {
        let mut overlay = Overlay::default();
        let (low, high) = sight.scene.mesh.bounds();

        if view.show_grid {
            overlay.grid(sight);
        }
        if view.bounding_box {
            overlay.cuboid(sight, low, high, BOUNDS, true);
        }
        if selected {
            let (center, half) = ((low + high) * 0.5, (high - low) * 0.5 * SELECTION_MARGIN);
            overlay.cuboid(sight, center - half, center + half, SELECTION, false);
        }
        if view.light_helpers {
            for light in &view.lights {
                overlay.light(sight, light);
            }
        }
        if view.vertex_tangent_space {
            overlay.tangent_space(sight);
        }
        overlay
    }

    /// The parts of the line from `from` to `to` that the camera sees
    fn line(&mut self, sight: &Sight, from: Vec3, to: Vec3, color: Color32, width: f32) {
        let point = |piece: usize| from.lerp(to, piece as f32 / PIECES as f32);
        let mut start: Option<[f32; 2]> = None;

        for piece in 0..PIECES {
            let (near, far) = (sight.place(point(piece)), sight.place(point(piece + 1)));
            let seen = near.is_some() && far.is_some() && sight.sees(from.lerp(to, (piece as f32 + 0.5) / PIECES as f32));
            if seen && start.is_none() {
                start = near;
            }

            // A run of seen pieces ends where the mesh hides the line, or at its end
            let ends_here = !seen || piece + 1 == PIECES;
            let end = if seen { far } else { near };
            if let (true, Some(from), Some(to)) = (ends_here, start, end) {
                self.lines.push(OverlayLine { from, to, color, width });
                start = None;
            }
        }
    }

    /// A grid through the middle of the scene, fading toward its edge, with the world axes in their colors
    fn grid(&mut self, sight: &Sight) {
        let step = GRID_REACH / GRID_LINES as f32;

        for index in -GRID_LINES..=GRID_LINES {
            let offset = index as f32 * step;
            let fade = 1.0 - (1.0 - GRID_EDGE_SHARE) * (index.abs() as f32 / GRID_LINES as f32);
            let strength = if index % GRID_MAJOR == 0 { 1.0 } else { 0.5 };
            let faint = Color32::from_rgba_unmultiplied(0xa0, 0xa0, 0xa0, (GRID_OPACITY * fade * strength) as u8);
            let (along_x, along_z, width) = if index == 0 { (AXIS_X, AXIS_Z, 1.5) } else { (faint, faint, 1.0) };

            self.line(sight, Vec3::new(-GRID_REACH, 0.0, offset), Vec3::new(GRID_REACH, 0.0, offset), along_x, width);
            self.line(sight, Vec3::new(offset, 0.0, -GRID_REACH), Vec3::new(offset, 0.0, GRID_REACH), along_z, width);
        }
        self.line(sight, Vec3::new(0.0, -GRID_REACH, 0.0), Vec3::new(0.0, GRID_REACH, 0.0), AXIS_Y, 1.5);
    }

    /// The twelve edges of a box, with a mark on each corner the camera sees when `corners` is set
    fn cuboid(&mut self, sight: &Sight, low: Vec3, high: Vec3, color: Color32, corners: bool) {
        let corner = |index: usize| {
            Vec3::new(if index & 1 == 0 { low.x } else { high.x }, if index & 2 == 0 { low.y } else { high.y }, if index & 4 == 0 { low.z } else { high.z })
        };

        for index in 0..8usize {
            // Each edge once: from a corner to the three that differ from it in one higher axis
            for axis in [1usize, 2, 4] {
                if index & axis == 0 {
                    self.line(sight, corner(index), corner(index | axis), color, 1.0);
                }
            }
            if corners
                && sight.sees(corner(index))
                && let Some(at) = sight.place(corner(index))
            {
                self.marks.push(OverlayMark { at, color, side: 5.0 });
            }
        }
    }

    /// A mark where a point light that is switched on is, joined to the middle of the scene
    fn light(&mut self, sight: &Sight, light: &PointLight) {
        let Some(lit) = materialcraft_render::Light::of(light) else { return };

        let position = sight.scene.center + Vec3::from(lit.position) * sight.scene.radius;
        self.line(sight, sight.scene.center, position, LIGHT, 1.0);
        if let Some(at) = sight.place(position) {
            self.marks.push(OverlayMark { at, color: LIGHT, side: 9.0 });
        }
    }

    /// The tangent (red), binormal (blue) and normal (green) of the vertices the camera sees
    fn tangent_space(&mut self, sight: &Sight) {
        let mesh = &sight.scene.mesh;
        let stride = mesh.positions.len().div_ceil(TANGENT_VERTICES).max(1);
        let length = sight.scene.radius * TANGENT_LENGTH;
        let vertices = mesh.positions.iter().zip(&mesh.normals).zip(&mesh.tangents).step_by(stride);

        for ((position, normal), tangent) in vertices {
            // A vertex on the far side of the mesh, or behind the camera, shows nothing
            let Some(from) = sight.place(*position) else { continue };
            let toward_camera = if sight.camera.ortho { -sight.camera.forward() } else { sight.camera.eye() - *position };
            if normal.dot(toward_camera) <= 0.0 || !sight.sees(*position + *normal * (length * 0.25)) {
                continue;
            }

            let along = tangent.truncate();
            let across = normal.cross(along) * tangent.w;
            for (direction, color) in [(along, TANGENT), (across, BINORMAL), (*normal, NORMAL)] {
                if let Some(to) = sight.place(*position + direction * length) {
                    self.lines.push(OverlayLine { from, to, color, width: 1.0 });
                }
            }
        }
    }
}
