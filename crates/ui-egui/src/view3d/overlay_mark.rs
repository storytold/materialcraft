use egui::Color32;

/// A filled square drawn over the frame: a corner of the bounding box or a light
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct OverlayMark {
    /// Fractions of the frame from its top-left corner
    pub at: [f32; 2],
    pub color: Color32,
    /// Points
    pub side: f32,
}
