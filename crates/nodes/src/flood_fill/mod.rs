pub(crate) mod ccl;
pub(crate) mod mapper;
pub(crate) mod node;
pub(crate) mod to_bbox_size;
pub(crate) mod to_gradient;
pub(crate) mod to_index;
pub(crate) mod to_position;
pub(crate) mod to_random_color;
pub(crate) mod to_random_grayscale;

#[cfg(test)]
mod tests;

pub(crate) const CATEGORY: &str = "Filters/Effects";

pub(crate) fn defs() -> Vec<crate::NodeDefinition> {
    vec![
        node::definition(),
        to_random_color::definition(),
        to_random_grayscale::definition(),
        to_gradient::definition(),
        to_bbox_size::definition(),
        mapper::definition(),
        to_index::definition(),
        to_position::definition(),
    ]
}
