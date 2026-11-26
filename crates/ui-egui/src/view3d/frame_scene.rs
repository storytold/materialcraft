use std::sync::Arc;

use materialcraft_engine::View3d;
use materialcraft_render::Scene;

/// Geometry and camera retained with the pixels they produced
pub(super) struct FrameScene {
    pub scene: Arc<Scene>,
    pub view: View3d,
    pub label: String,
}
