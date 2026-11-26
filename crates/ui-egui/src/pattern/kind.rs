//! The FX-Map editor's node kind.

use craft_ui::graph::NodeKind;

/// An FX-Map node as the framework node graph draws it: input on top, outputs on the bottom, or a Dot.
#[allow(dead_code)] // P3(graph-w15): ge.A20
pub(crate) struct FxMapKind {
    _private: (),
}

impl NodeKind<()> for FxMapKind {}

#[allow(dead_code)] // P3(graph-w15): ge.A20
impl FxMapKind {
    /// The slot of the port with this index.
    pub(crate) fn port_slot(&self, port: usize) -> Option<usize> {
        let _ = port;
        todo!("ge.A20")
    }

    /// The published id of the port with this index: `input`, `output0..` or `output`.
    pub(crate) fn port_id(&self, port: usize) -> String {
        let _ = port;
        todo!("ge.A20")
    }

    /// The 96 x 96 card or the 20 x 20 Dot at `pos`.
    pub(crate) fn rect(&self, pos: [f32; 2]) -> egui::Rect {
        let _ = pos;
        todo!("ge.A20")
    }
}
