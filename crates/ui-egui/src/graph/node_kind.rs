//! The material graph's node kind and its data colours.

use craft_theme::{Rgba, Tokens};
use craft_ui::graph::NodeKind;
use craft_ui::style::TokenGroup;
use materialcraft_engine::core::{PatternNodeKind, ValueType};

use crate::MaterialCraftApp;

/// The width of a node card.
#[allow(dead_code)] // P3(graph-w7): ge.A6
pub(crate) const NODE_W: f32 = super::node_card::WIDTH;

/// The data colours of the node canvases, one set for every theme: node headers, ports, Function nodes and FX-Map
/// nodes.
#[derive(Clone, Debug, TokenGroup)]
#[group(path = "materialcraft.node")]
#[allow(dead_code)] // P3(graph-w7): ge.A6
pub(crate) struct NodeColors {
    /// The header of an output node.
    pub header_output: Rgba,
    /// The header of a node of no known category.
    pub header_fallback: Rgba,
    /// A Function graph's output node.
    pub function_output: Rgba,
    /// A Function node whose operation is missing.
    pub function_missing: Rgba,
    /// An FX-Map node's symbol.
    pub fx_symbol: Rgba,
    /// An FX-Map node's port.
    pub fx_port: Rgba,
}

#[allow(dead_code)] // P3(graph-w7): ge.A6
impl NodeColors {
    fn derive(tokens: &Tokens) -> Self {
        let _ = tokens;
        todo!("ge.A6")
    }

    /// The header colour of the node definition `def`.
    pub(crate) fn header(&self, def: &str) -> Rgba {
        let _ = def;
        todo!("ge.A6")
    }

    /// The colour of a port of `kind`.
    pub(crate) fn port(&self, kind: ValueType) -> Rgba {
        let _ = kind;
        todo!("ge.A6")
    }

    /// The header colour of a Function node of category `cat`.
    pub(crate) fn function_category(&self, cat: &str) -> Rgba {
        let _ = cat;
        todo!("ge.A6")
    }

    /// The colour of a Function graph's output node.
    pub(crate) fn function_output(&self) -> Rgba {
        todo!("ge.A6")
    }

    /// The colour of a Function node whose operation is missing.
    pub(crate) fn function_missing(&self) -> Rgba {
        todo!("ge.A6")
    }

    /// The colour of an FX-Map node of `kind`.
    pub(crate) fn fx_kind(&self, kind: PatternNodeKind) -> Rgba {
        let _ = kind;
        todo!("ge.A6")
    }

    /// The colour of an FX-Map node's symbol.
    pub(crate) fn fx_symbol(&self) -> Rgba {
        todo!("ge.A6")
    }

    /// The colour of an FX-Map node's port.
    pub(crate) fn fx_port(&self) -> Rgba {
        todo!("ge.A6")
    }
}

/// A material graph node as the framework node graph draws it.
#[allow(dead_code)] // P3(graph-w7): ge.A6
pub(crate) struct MaterialKind {
    _private: (),
}

impl NodeKind<()> for MaterialKind {}

#[allow(dead_code)] // P3(graph-w7): ge.A6
impl MaterialKind {
    /// The node card's rect with its top-left corner at `pos`.
    pub(crate) fn rect(&self, pos: [f32; 2]) -> egui::Rect {
        let _ = pos;
        todo!("ge.A6")
    }
}

/// The tooltip of a node: its definition's description.
#[allow(dead_code)] // P3(graph-w7): ge.A6
pub(crate) fn tooltip(app: &MaterialCraftApp, node: materialcraft_engine::core::NodeId) -> Option<String> {
    let _ = (app, node);
    todo!("ge.A6")
}
