pub const CATEGORY: &str = "Filters/Normal map";

mod ambient_occlusion;
mod curvature;
mod normal_combine;
mod normal_invert;
mod normal_normalize;
mod normal_ops;
mod normal_to_height;

use crate::NodeDefinition;

pub(crate) fn defs() -> Vec<NodeDefinition> {
    vec![
        ambient_occlusion::definition(),
        curvature::definition(),
        normal_combine::definition(),
        normal_invert::definition(),
        normal_normalize::definition(),
        normal_to_height::definition(),
        normal_ops::definition_facing_normal(),
        normal_ops::definition_height_normal_blender(),
        normal_ops::definition_height_to_normal_world_units(),
        normal_ops::definition_normal_blend(),
        normal_ops::definition_normal_sobel(),
        normal_ops::definition_normal_to_height_hq(),
        normal_ops::definition_normal_transform(),
        normal_ops::definition_normal_vector_rotation(),
        normal_ops::definition_bent_normal(),
    ]
}

#[cfg(test)]
mod tests;
