use super::ATOMIC;
use crate::{NodeDefinition, node};
use materialcraft_core::{PatternGraph, PatternNodeKind};

pub(super) fn definition() -> NodeDefinition {
    node("fx_map", "FX-Map", ATOMIC)
        .desc("Places patterns using Quadrant, Switch and Iterate nodes.")
        .atomic()
        .inherit_from("background")
        .any_in("background", "Background")
        .any_in("pattern0", "Input image 0")
        .pattern_graph("pattern", "FX-Map graph", PatternGraph::with_root(PatternNodeKind::Quadrant))
        .float("roughness", "Roughness", 1.0, 0.0, 2.0)
        .float("opacity", "Opacity", 1.0, 0.0, 10.0)
        .boolean("color_mode", "Color output", true)
        .color("background", "Background color", [0.0; 4])
        .float4("render_region", "Render region", [0.0, 1.0, 0.0, 1.0], -65536.0, 65536.0)
        .float4("tiling_region", "Tiling region", [0.0, 1.0, 0.0, 1.0], 0.0, 1.0)
        .boolean("cull_outside", "Cull outside", false)
        .any_out()
        .cook(crate::pattern::cook)
}
