pub(crate) mod bevel;
pub(crate) mod diffusion;
pub(crate) mod edge_detect;
pub(crate) mod shape_ops;

#[cfg(test)]
mod tests;

pub(crate) const CATEGORY: &str = "Filters/Effects";

pub(crate) fn defs() -> Vec<crate::NodeDefinition> {
    vec![
        bevel::definition(),
        edge_detect::definition(),
        shape_ops::definition_rtao(),
        shape_ops::definition_cross_section(),
        shape_ops::definition_curvature_smooth(),
        shape_ops::definition_curvature_sobel(),
        shape_ops::definition_emboss_with_gloss(),
        shape_ops::definition_extend_shape(),
        diffusion::definition_diffusion_color(),
        diffusion::definition_diffusion_grayscale(),
        diffusion::definition_diffusion_uv(),
    ]
}
