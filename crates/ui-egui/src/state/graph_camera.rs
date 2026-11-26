use craft_behavior::canvas::{CanvasView, ZoomFloor, ZoomScheme};
use craft_behavior::kurbo::{Point, Size};
use serde::{Deserialize, Serialize};

/// The graph view's camera: the graph point at the view's top-left corner, and the zoom in points per graph unit
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphCamera {
    pub pan: [f32; 2],
    pub zoom: f32,
}

/// The zoom rungs of the graph canvas, in points per graph unit.
const RUNGS: [f64; 7] = [0.08, 0.125, 0.25, 0.5, 1.0, 2.0, 4.0];

/// The material graph's zoom range in points per graph unit: 0.08 to 4.
pub const GRAPH_ZOOM: ZoomScheme = ZoomScheme::new(&RUNGS, ZoomFloor::Fixed(0.08), 4.0);

/// `GRAPH_ZOOM` whose 100 % is one point per graph unit at any display scale.
pub fn graph_zoom(pixels_per_point: f32) -> ZoomScheme {
    GRAPH_ZOOM.with_hundred(f64::from(pixels_per_point))
}

impl GraphCamera {
    /// The camera with a finite pan and a zoom inside [`GRAPH_ZOOM`]'s range (state set from outside can hold a zoom
    /// of 0 or NaN, which the view divides by).
    pub fn sanitized(self) -> GraphCamera {
        let d = GraphCamera::default();
        let pan = |v: f32, d: f32| if v.is_finite() { v.clamp(-1.0e9, 1.0e9) } else { d };
        // `GRAPH_ZOOM` unscaled counts in points per unit, the unit of `zoom`; its floor is fixed, so no size applies.
        let zoom = if self.zoom.is_finite() { GRAPH_ZOOM.clamp(f64::from(self.zoom), Size::ZERO) as f32 } else { d.zoom };
        GraphCamera { pan: [pan(self.pan[0], d.pan[0]), pan(self.pan[1], d.pan[1])], zoom }
    }

    /// The live view of this camera in `rect`: both zooms are points per graph unit, so only the anchor moves from
    /// the top-left corner to the centre.
    pub fn view(self, rect: egui::Rect) -> CanvasView {
        let camera = self.sanitized();
        let half = half_size(rect) / camera.zoom;
        let center = Point::new(f64::from(camera.pan[0] + half.x), f64::from(camera.pan[1] + half.y));
        CanvasView::new(rect, center, f64::from(camera.zoom))
    }

    /// The saved camera of a live view.
    pub fn from_view(view: &CanvasView) -> GraphCamera {
        let zoom = view.zoom as f32;
        if !(zoom.is_finite() && zoom > 0.0) {
            return GraphCamera::default();
        }
        let half = half_size(view.rect) / zoom;
        GraphCamera { pan: [view.center.x as f32 - half.x, view.center.y as f32 - half.y], zoom }.sanitized()
    }
}

/// Half the size of `rect`, or nothing for a rect with no area (a view never shown).
fn half_size(rect: egui::Rect) -> egui::Vec2 {
    if rect.is_positive() && rect.is_finite() { rect.size() * 0.5 } else { egui::Vec2::ZERO }
}

impl Default for GraphCamera {
    fn default() -> Self {
        GraphCamera { pan: [-60.0, -60.0], zoom: 0.8 }
    }
}
