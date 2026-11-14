use std::sync::Arc;

use egui::{Pos2, TextureHandle};

use super::histogram::Histogram;

/// What the 2D view keeps between frames
#[derive(Default)]
pub struct View2dState {
    /// The texture of the shown image, keyed by everything it was made from
    pub(super) texture: Option<(u64, TextureHandle)>,
    /// The last shown image (RGBA8, for `view.screenshot`)
    pub shown: Option<(u32, u32, Vec<u8>)>,
    /// The histogram of the shown image, keyed by the image and the counted channel
    pub(super) histogram: Option<(u64, Histogram)>,
    /// The background image file and its texture; no texture when the file could not be shown
    pub(super) background: Option<(String, Option<TextureHandle>)>,
    /// Width and height of the image shown last frame
    pub(super) image_size: Option<[u32; 2]>,
    /// The pixel the pointer was last over, from the top-left corner
    pub(super) hover: Option<[u32; 2]>,
    /// The UV edges of the 3D view's mesh, keyed by the mesh
    pub(super) uv_edges: Option<(String, Arc<Vec<[Pos2; 2]>>)>,
    /// Where a drag across the histogram started, as a fraction of its range
    pub(super) histogram_drag: Option<f32>,
}
