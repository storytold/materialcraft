pub(crate) mod anisotropic_blur;
pub(crate) mod blur_hq;
pub(crate) mod non_uniform_blur;
pub(crate) mod radial_blur;
pub(crate) mod slope_blur;
pub(crate) mod slope_blur_color;

#[cfg(test)]
mod tests;

pub(crate) const CATEGORY: &str = "Filters/Blurs";

pub(crate) fn defs() -> Vec<crate::NodeDefinition> {
    vec![
        slope_blur::definition(),
        slope_blur_color::definition(),
        non_uniform_blur::grayscale_definition(),
        non_uniform_blur::color_definition(),
        radial_blur::definition(),
        anisotropic_blur::definition(),
        blur_hq::definition(),
    ]
}
