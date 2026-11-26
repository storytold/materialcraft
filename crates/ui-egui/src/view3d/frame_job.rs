use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;

use super::frame::Frame;

/// A full-quality frame rendering on a worker thread. Dropping it cancels the render
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(super) struct FrameJob {
    /// What the frame shows (the view's content key)
    pub(super) key: u64,
    /// The view size, in points, the frame is rendered for
    pub(super) size: (u32, u32),
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) rx: Receiver<Option<Frame>>,
}

impl Drop for FrameJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
