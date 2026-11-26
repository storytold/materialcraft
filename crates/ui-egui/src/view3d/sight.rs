use materialcraft_engine::View3d;
use materialcraft_engine::geom::glam::{Vec2, Vec3};
use materialcraft_engine::geom::{OrbitCamera, Ray};
use materialcraft_render::{Scene, Settings};

/// How near a point may be to the surface in front of it and still count as seen, in scene radii
const SURFACE_TOLERANCE: f32 = 0.004;

/// The view's camera looking at its scene: where a point of the scene is on the frame, and whether
/// the mesh hides it
pub(super) struct Sight<'a> {
    pub scene: &'a Scene,
    pub camera: OrbitCamera,
    /// Width of the frame over its height
    pub aspect: f32,
}

impl<'a> Sight<'a> {
    pub fn of(scene: &'a Scene, view: &View3d, aspect: f32) -> Sight<'a> {
        let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
        Sight { scene, camera: materialcraft_render::camera(scene, &Settings::for_view(view)), aspect }
    }

    /// Where a point is on the frame, as fractions of its width and height from the top-left
    /// corner; `None` for a point behind the camera
    pub fn place(&self, point: Vec3) -> Option<[f32; 2]> {
        if self.camera.ortho && (point - self.camera.eye()).dot(self.camera.forward()) <= 0.0 {
            return None;
        }

        let device = self.camera.project(point, self.aspect)?;
        let at = [(device.x + 1.0) * 0.5, (1.0 - device.y) * 0.5];
        at.iter().all(|fraction| fraction.is_finite()).then_some(at)
    }

    /// The ray from the camera through a place on the frame
    pub fn ray(&self, at: [f32; 2]) -> Ray {
        self.camera.ray(Vec2::new(at[0] * 2.0 - 1.0, 1.0 - at[1] * 2.0), self.aspect)
    }

    /// The mesh does not hide the point from the camera
    pub fn sees(&self, point: Vec3) -> bool {
        let Some(at) = self.place(point) else { return false };

        let ray = self.ray(at);
        let reach = (point - ray.origin).length() - self.scene.radius * SURFACE_TOLERANCE;
        reach <= 0.0 || !self.scene.bvh.occluded(ray.origin, ray.dir, reach)
    }

    /// Screen points per scene unit at the distance the camera orbits, for a frame `height` points tall
    pub fn points_per_unit(&self, height: f32) -> f32 {
        let visible = 2.0 * self.camera.distance * (self.camera.fov.to_radians() * 0.5).tan();
        if visible.is_finite() && visible > 0.0 { height / visible } else { 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use materialcraft_engine::geom::view::Projection;

    #[test]
    fn an_orthographic_point_behind_the_camera_is_not_placed() {
        let scene = Scene::builtin("sphere");
        let view = View3d { projection: Projection::Orthographic, ..View3d::default() };
        let sight = Sight::of(&scene, &view, 1.0);
        let behind = sight.camera.eye() - sight.camera.forward();

        assert_eq!(sight.place(behind), None);
    }
}
