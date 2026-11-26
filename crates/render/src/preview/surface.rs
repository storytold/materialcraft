use materialcraft_geom::glam::Vec3;

/// The surface at a ray's hit, ready for a shading model
pub(super) struct Surface {
    /// Shading normal, toward the viewer
    pub(super) n: Vec3,
    /// Toward the viewer
    pub(super) v: Vec3,
    pub(super) bitangent: Vec3,
    /// A point just off the true surface, for rays that start on it
    pub(super) p: Vec3,
    /// The true surface's normal, toward the viewer
    pub(super) face: Vec3,
    pub(super) base: Vec3,
    pub(super) rough: f32,
    pub(super) metal: f32,
    pub(super) ao: f32,
}
