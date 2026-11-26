/// What dragging in the 3D view does
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// Orbit, pan and zoom the camera
    #[default]
    Camera,
    /// Orbit the first point light and move it nearer or farther
    Light,
    /// Click the mesh to select it
    Select,
}
