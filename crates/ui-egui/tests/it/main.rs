//! One integration-test executable avoids linking the same dependencies for every feature.
//! Keep snapshot.rs standalone.
//! resize.rs isolates its bounded render wait from unrelated tests sharing the global Rayon pool.

mod clipboard_shortcuts;
mod export_window;
mod graph_properties;
mod interface_scale;
mod menu_bar;
mod new_graph_dialog;
mod node_look;
mod normal_maps;
mod numeric_input;
mod preferences_window;
mod preview_resolution;
mod resolution;
mod slider_round_values;
mod view2d;
mod view3d;
