use serde::{Deserialize, Serialize};

/// The graph view's camera: the graph point at the view's top-left corner, and the zoom
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphCamera {
    pub pan: [f32; 2],
    pub zoom: f32,
}

impl GraphCamera {
    pub const ZOOM: (f32, f32) = (0.08, 4.0);

    /// The camera with a finite pan and a zoom inside [`Self::ZOOM`] (state set from outside can
    /// hold a zoom of 0 or NaN, which the view divides by).
    pub fn sanitized(self) -> GraphCamera {
        let d = GraphCamera::default();
        let pan = |v: f32, d: f32| if v.is_finite() { v.clamp(-1.0e9, 1.0e9) } else { d };
        let zoom = if self.zoom.is_finite() { self.zoom.clamp(Self::ZOOM.0, Self::ZOOM.1) } else { d.zoom };
        GraphCamera { pan: [pan(self.pan[0], d.pan[0]), pan(self.pan[1], d.pan[1])], zoom }
    }
}

/// The zoom rungs of the graph canvas, in points per graph unit.
const RUNGS: [f64; 7] = [0.08, 0.125, 0.25, 0.5, 1.0, 2.0, 4.0];

/// The material graph's zoom range in points per graph unit: 0.08 to 4.
#[allow(dead_code)] // P3(graph-w6): ge.A1
pub const GRAPH_ZOOM: craft_behavior::canvas::ZoomScheme = craft_behavior::canvas::ZoomScheme::new(&RUNGS, craft_behavior::canvas::ZoomFloor::Fixed(0.08), 4.0);

/// `GRAPH_ZOOM` whose 100 % is one point per graph unit at any display scale.
#[allow(dead_code)] // P3(graph-w6): ge.A1
pub fn graph_zoom(pixels_per_point: f32) -> craft_behavior::canvas::ZoomScheme {
    GRAPH_ZOOM.with_hundred(f64::from(pixels_per_point))
}

#[allow(dead_code)] // P3(graph-w6): ge.A1
impl GraphCamera {
    /// The live view of this camera in `rect`.
    pub fn view(self, rect: egui::Rect) -> craft_behavior::canvas::CanvasView {
        let _ = rect;
        todo!("ge.A1")
    }

    /// The saved camera of a live view.
    pub fn from_view(view: &craft_behavior::canvas::CanvasView) -> GraphCamera {
        let _ = view;
        todo!("ge.A1")
    }
}

impl Default for GraphCamera {
    fn default() -> Self {
        GraphCamera { pan: [-60.0, -60.0], zoom: 0.8 }
    }
}
