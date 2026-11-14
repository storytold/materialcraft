use materialcraft_engine::core::{NodeId, OutputFormat};
use materialcraft_engine::image::Image;

use crate::MaterialCraftApp;

/// The image the 2D view shows this frame
pub(super) struct Shown {
    pub(super) node: NodeId,
    pub(super) image: Image,
    /// Identifies the cooked image: its result, its size and its port
    pub(super) key: u64,
    /// The largest whole value of the image's format; float formats have none
    pub(super) levels: Option<u32>,
    /// Bits per channel, as the size line names them
    pub(super) bits: &'static str,
}

impl Shown {
    pub(super) fn current(app: &MaterialCraftApp) -> Option<Shown> {
        let (node, image) = app.session.view2d_image()?;
        let result = app.session.cooked.nodes.get(&node);
        let format = result.map_or(OutputFormat::U16, |result| result.base.format);
        let levels = match format {
            OutputFormat::U8 => Some(u32::from(u8::MAX)),
            OutputFormat::Inherit | OutputFormat::U16 => Some(u32::from(u16::MAX)),
            OutputFormat::F16 | OutputFormat::F32 => None,
        };
        // The cooked key alone does not name the size or the port
        let key = result.map_or(0, |result| result.key)
            ^ u64::from(image.w).rotate_left(16)
            ^ u64::from(image.h)
            ^ (app.session.view2d.port as u64).wrapping_mul(0x9e37);

        Some(Shown { node, image: image.clone(), key, levels, bits: format.bits() })
    }

    /// Bits per channel a histogram of this image counts in by default
    pub(super) fn histogram_bits(&self) -> u32 {
        match self.levels {
            Some(levels) if levels <= u32::from(u8::MAX) => 8,
            Some(_) => 16,
            None => 32,
        }
    }
}
