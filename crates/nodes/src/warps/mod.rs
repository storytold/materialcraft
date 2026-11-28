pub(crate) mod multi_directional_warp;
pub(crate) mod multi_directional_warp_color;
pub(crate) mod non_uniform_directional_warp;
pub(crate) mod swirl;
pub(crate) mod vector_warp;

#[cfg(test)]
mod tests;

pub(crate) const CATEGORY: &str = "Filters/Effects";

pub(crate) fn defs() -> Vec<crate::NodeDefinition> {
    vec![
        multi_directional_warp::definition(),
        multi_directional_warp_color::definition(),
        vector_warp::grayscale_definition(),
        vector_warp::color_definition(),
        non_uniform_directional_warp::definition(),
        swirl::definition(),
    ]
}
