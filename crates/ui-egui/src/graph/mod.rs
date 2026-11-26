pub(crate) mod actions;
pub(crate) mod add_node_menu;
// The Function and FX-Map hosts build and apply their annotations through it too.
pub(crate) mod annotations;
pub(crate) mod canvas;
mod canvas_events;
mod context_menu;
#[cfg(test)]
mod frames;
#[cfg(test)]
mod function_search_tests;
pub(crate) mod graph_parent;
mod graph_view;
mod instance_context_menu;
#[cfg(test)]
mod interaction_tests;
mod library_drag;
pub(crate) mod node_colors;
mod node_kind;
pub(crate) mod node_search;
pub(crate) mod node_selection;
pub(crate) mod port_hit;
pub(crate) mod subgraph_navigation;

pub use canvas::show;
pub use graph_view::{GraphView, wire_id};
pub use library_drag::LibraryDrag;
pub use navigation::{add_entry_in_view, add_node_near_selection};
use node_search::NodeSearch;
// The card metrics are `pub(crate)`, and every reader is in this crate.
pub(crate) use node_kind::{FOOTER_HEIGHT, HEADER_HEIGHT, NODE_W};
pub use node_style::{node_label, shows_preview};
pub use port_hit::PortHit;
pub type WireRef = (materialcraft_engine::core::NodeId, String);

#[cfg(test)]
mod canvas_tests;
mod navigation;
mod node_style;
pub(crate) mod search;
pub(crate) mod selection_commands;
pub(crate) mod toolbar;
pub(crate) mod value_text;
pub(crate) mod wire_actions;

#[cfg(test)]
mod review_tests;

pub(crate) mod edit_menu;
pub(crate) mod editor_actions;
pub(crate) mod editor_target;

pub use navigation::import_bitmap_bytes;
#[cfg(not(target_arch = "wasm32"))]
pub use navigation::import_bitmap_file;
