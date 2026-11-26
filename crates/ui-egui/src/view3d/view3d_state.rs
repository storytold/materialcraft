use std::sync::Arc;

use egui::TextureHandle;
use materialcraft_render::{Lighting, Scene};

use super::frame_job::FrameJob;
use super::frame_scene::FrameScene;
use super::overlay::Overlay;
use super::relief_job::ReliefJob;
use super::{Subject, Tool};

/// What the 3D view keeps between frames
#[derive(Default)]
pub struct View3dState {
    pub(super) scene: Option<(String, Arc<Scene>)>,
    /// The scene displaced by the height output, keyed by what it was built from
    pub(super) displaced: Option<(u64, Arc<Scene>)>,
    /// The displaced scene building on a worker thread
    pub(super) relief_job: Option<ReliefJob>,
    pub(super) light: Option<Arc<Lighting>>,
    pub(super) tex: Option<TextureHandle>,
    /// What the texture shows: (content key, full quality)
    pub(super) key: Option<(u64, bool)>,
    /// The view size (points) the texture was rendered for
    pub(super) tex_size: Option<(u32, u32)>,
    /// The view size on the last frame
    pub(super) size: Option<(u32, u32)>,
    /// When the view's size or the camera last changed (app seconds)
    pub(super) changed_at: Option<f64>,
    /// The full-quality frame rendering on a worker thread
    pub(super) job: Option<FrameJob>,
    pub render_ms: f32,
    /// Frames rendered so far (previews and full quality)
    pub renders: u64,
    /// The last frame shown (RGBA8, for `view.screenshot`)
    pub shown: Option<(u32, u32, Arc<Vec<u8>>)>,
    /// The scene and camera that produced the displayed pixels
    pub(super) shown_scene: Option<FrameScene>,
    /// Index of the quality level of the last moving frame
    pub(super) move_level: usize,
    pub tool: Tool,
    /// What the Properties panel shows of the 3D view, with what else was inspected when it was
    /// chosen: picking a node or a graph afterwards takes the panel back
    pub(super) inspected: Option<(Subject, u64)>,
    /// The scene browser is open beside the view
    pub scene_browser: bool,
    /// The display buttons of the tool strip are folded away
    pub display_tools_hidden: bool,
    /// The mesh is selected
    pub selected: bool,
    /// The lines drawn over the frame, with the key of what they were made for
    pub(super) overlay: Option<(u64, Arc<Overlay>)>,
}

impl View3dState {
    /// A frame or its displaced geometry is still being built in the background
    pub fn busy(&self) -> bool {
        self.job.is_some() || self.relief_job.is_some()
    }
}
