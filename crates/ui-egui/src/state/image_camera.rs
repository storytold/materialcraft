use serde::{Deserialize, Serialize};

/// The 2D view's camera: image zoom (1 = one image pixel per point) and center offset
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageCamera {
    pub zoom: f32,
    pub offset: [f32; 2],
    /// Fit the image to the view (until the user zooms)
    pub fit: bool,
}

impl ImageCamera {
    pub const ZOOM: (f32, f32) = (0.02, 64.0);
    /// How much one wheel notch or one click on a zoom button changes the zoom
    pub const STEP: f32 = 1.2;

    /// This camera zoomed by `steps` steps, keeping the image point under `anchor` in place.
    /// `anchor` is a position relative to the center of the view
    pub fn zoomed(self, steps: f32, anchor: [f32; 2]) -> ImageCamera {
        let zoom = (self.zoom * Self::STEP.powf(steps)).clamp(Self::ZOOM.0, Self::ZOOM.1);
        let scale = zoom / self.zoom;
        let offset = [anchor[0] - (anchor[0] - self.offset[0]) * scale, anchor[1] - (anchor[1] - self.offset[1]) * scale];

        ImageCamera { zoom, offset, fit: false }.sanitized()
    }

    /// The camera with a finite offset and a zoom inside [`Self::ZOOM`].
    pub fn sanitized(self) -> ImageCamera {
        let off = |v: f32| if v.is_finite() { v.clamp(-1.0e9, 1.0e9) } else { 0.0 };
        let zoom = if self.zoom.is_finite() { self.zoom.clamp(Self::ZOOM.0, Self::ZOOM.1) } else { 1.0 };
        ImageCamera { zoom, offset: [off(self.offset[0]), off(self.offset[1])], fit: self.fit }
    }
}

impl Default for ImageCamera {
    fn default() -> Self {
        ImageCamera { zoom: 1.0, offset: [0.0, 0.0], fit: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zoom_step_scales_by_the_step_and_keeps_the_anchored_point_in_place() {
        let camera = ImageCamera { zoom: 1.0, offset: [10.0, -20.0], fit: true };

        let zoomed = camera.zoomed(1.0, [50.0, 30.0]);

        assert_eq!(zoomed.zoom, ImageCamera::STEP);
        assert!(!zoomed.fit, "zooming leaves the fitted view");
        // The image point under the anchor is the same before and after
        let before = [(50.0 - camera.offset[0]) / camera.zoom, (30.0 - camera.offset[1]) / camera.zoom];
        let after = [(50.0 - zoomed.offset[0]) / zoomed.zoom, (30.0 - zoomed.offset[1]) / zoomed.zoom];
        assert!((before[0] - after[0]).abs() < 1e-4 && (before[1] - after[1]).abs() < 1e-4, "{before:?} {after:?}");

        let back = zoomed.zoomed(-1.0, [50.0, 30.0]);
        assert!((back.zoom - 1.0).abs() < 1e-6);
        assert!((back.offset[0] - 10.0).abs() < 1e-3 && (back.offset[1] + 20.0).abs() < 1e-3);
    }

    #[test]
    fn zooming_stops_at_the_limits() {
        let camera = ImageCamera { zoom: 1.0, offset: [0.0, 0.0], fit: false };

        assert_eq!(camera.zoomed(1000.0, [0.0, 0.0]).zoom, ImageCamera::ZOOM.1);
        assert_eq!(camera.zoomed(-1000.0, [0.0, 0.0]).zoom, ImageCamera::ZOOM.0);
    }
}
