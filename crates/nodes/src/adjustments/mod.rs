pub const CATEGORY: &str = "Filters/Adjustments";

mod auto_levels;
mod clamp;
mod color_temperature;
mod color_transforms;
mod contrast_luminosity;
mod frequency_filters;
mod highpass;
mod histogram_ops;
mod histogram_range;
mod histogram_scan;
mod invert;
mod min_max;
mod quantize;
mod replace_color;
mod threshold;

use crate::NodeDefinition;

pub(crate) fn defs() -> Vec<NodeDefinition> {
    vec![
        auto_levels::definition(),
        clamp::definition(),
        contrast_luminosity::definition(),
        highpass::definition(),
        histogram_range::definition(),
        histogram_scan::definition(),
        invert::definition(),
        quantize::definition(),
        replace_color::definition(),
        threshold::definition(),
        min_max::definition(),
        color_temperature::definition(),
        color_transforms::channel_mixer_definition(),
        color_transforms::chrominance_extract_definition(),
        color_transforms::color_match_definition(),
        color_transforms::color_to_mask_definition(),
        color_transforms::convert_to_linear_definition(),
        color_transforms::convert_to_srgb_definition(),
        color_transforms::grayscale_conversion_advanced_definition(),
        color_transforms::hald_clut_definition(),
        color_transforms::color_temperature_adj_definition(),
        color_transforms::replace_color_range_definition(),
        histogram_ops::histogram_scan_non_uniform_definition(),
        histogram_ops::histogram_select_definition(),
        histogram_ops::histogram_shift_definition(),
        histogram_ops::hdr_range_viewer_definition(),
        histogram_ops::pow_definition(),
        frequency_filters::height_map_frequencies_mapper_definition(),
        frequency_filters::lighting_cancel_high_definition(),
        frequency_filters::lighting_cancel_low_definition(),
        frequency_filters::luminance_highpass_definition(),
    ]
}

#[cfg(test)]
mod tests;
