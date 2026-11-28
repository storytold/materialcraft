pub const CATEGORY: &str = "Material filters/Effects";

mod height_blend;
mod material_blend;
mod material_environment;
mod material_pbr;
mod water_drops;

use crate::NodeDefinition;

pub(crate) fn defs() -> Vec<NodeDefinition> {
    vec![
        height_blend::definition(),
        material_blend::definition(),
        water_drops::definition(),
        material_pbr::definition_material_height_blend(),
        material_pbr::definition_material_color_blend(),
        material_pbr::definition_material_adjustment_blend(),
        material_pbr::definition_material_switch(),
        material_pbr::definition_multi_material_blend(),
        material_environment::definition_bitmap_to_material_light(),
        material_environment::definition_season_filter(),
        material_environment::definition_snow_cover(),
        material_environment::definition_water_level(),
        material_environment::definition_material_transform(),
    ]
}

#[cfg(test)]
mod tests;
