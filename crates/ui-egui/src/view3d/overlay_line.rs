use egui::Color32;

/// A line drawn over the frame, its ends as fractions of the frame from its top-left corner
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct OverlayLine {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub color: Color32,
    /// Points
    pub width: f32,
}
