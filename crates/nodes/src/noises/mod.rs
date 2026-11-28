pub(crate) mod anisotropic_noise;
pub(crate) mod blue_noise_fast;
pub(crate) mod bnw_spots;
pub(crate) mod caustics;
pub(crate) mod clouds;
pub(crate) mod cracks;
pub(crate) mod creased;
pub(crate) mod crystal;
pub(crate) mod directional_noise;
pub(crate) mod directional_scratches;
pub(crate) mod dirt;
pub(crate) mod fluid;
pub(crate) mod fractal_sum;
pub(crate) mod fur;
pub(crate) mod gaussian_noise;
pub(crate) mod grunge_map;
pub(crate) mod grunge_special;
pub(crate) mod liquid;
pub(crate) mod messy_fibers;
pub(crate) mod microscope_view;
pub(crate) mod moisture_noise;
pub(crate) mod noise_3d;
pub(crate) mod perlin;
pub(crate) mod plasma;
pub(crate) mod simplex;
pub(crate) mod turbulence;
pub(crate) mod voronoi;
pub(crate) mod voronoi_fractal;
pub(crate) mod waveform;
pub(crate) mod white_noise;
pub(crate) mod white_noise_fast;

#[cfg(test)]
mod tests;

pub(crate) const CATEGORY: &str = "Texture generators/Noises";

pub(crate) fn defs() -> Vec<crate::NodeDefinition> {
    vec![
        anisotropic_noise::definition(),
        blue_noise_fast::definition(),
        bnw_spots::definition(),
        bnw_spots::definition_1(),
        bnw_spots::definition_2(),
        bnw_spots::definition_3(),
        caustics::definition(),
        clouds::definition(),
        clouds::definition_1(),
        clouds::definition_2(),
        clouds::definition_3(),
        cracks::definition(),
        creased::definition(),
        crystal::definition(),
        crystal::definition_1(),
        crystal::definition_2(),
        directional_noise::definition(),
        directional_noise::definition_1(),
        directional_noise::definition_2(),
        directional_noise::definition_3(),
        directional_noise::definition_4(),
        directional_scratches::definition(),
        dirt::definition(),
        dirt::definition_1(),
        dirt::definition_2(),
        dirt::definition_3(),
        dirt::definition_4(),
        dirt::definition_5(),
        dirt::definition_gradient(),
        fluid::definition(),
        fractal_sum::definition(),
        fractal_sum::definition_1(),
        fractal_sum::definition_2(),
        fractal_sum::definition_3(),
        fractal_sum::definition_4(),
        fractal_sum::definition_base(),
        fur::definition(),
        fur::definition_1(),
        fur::definition_2(),
        fur::definition_3(),
        gaussian_noise::definition(),
        gaussian_noise::spots_1_definition(),
        gaussian_noise::spots_2_definition(),
        grunge_map::definition(),
        grunge_map::def_002(),
        grunge_map::def_003(),
        grunge_map::def_004(),
        grunge_map::def_005(),
        grunge_map::def_006(),
        grunge_map::def_007(),
        grunge_map::def_008(),
        grunge_map::def_009(),
        grunge_map::def_010(),
        grunge_map::def_011(),
        grunge_map::def_012(),
        grunge_map::def_013(),
        grunge_map::def_014(),
        grunge_map::def_015(),
        grunge_special::concrete_definition(),
        grunge_special::damas_definition(),
        grunge_special::galvanic_large_definition(),
        grunge_special::galvanic_small_definition(),
        grunge_special::leaks_definition(),
        grunge_special::leaky_paint_definition(),
        grunge_special::rough_dirty_definition(),
        grunge_special::rust_fine_definition(),
        grunge_special::scratches_dirty_definition(),
        grunge_special::scratches_fine_definition(),
        grunge_special::scratches_rough_definition(),
        grunge_special::shavings_definition(),
        grunge_special::splashes_dusty_definition(),
        grunge_special::spots_definition(),
        grunge_special::spots_dirty_definition(),
        liquid::definition(),
        messy_fibers::definition_1(),
        messy_fibers::definition_2(),
        messy_fibers::definition_3(),
        microscope_view::definition(),
        moisture_noise::definition(),
        noise_3d::perlin_3d_definition(),
        noise_3d::perlin_3d_fractal_definition(),
        noise_3d::ridged_3d_fractal_definition(),
        noise_3d::simplex_3d_definition(),
        noise_3d::voronoi_3d_definition(),
        noise_3d::voronoi_3d_fractal_definition(),
        noise_3d::worley_3d_definition(),
        perlin::definition(),
        plasma::definition(),
        simplex::definition(),
        turbulence::definition(),
        voronoi::definition(),
        voronoi::definition_1(),
        voronoi::definition_2(),
        voronoi::definition_3(),
        voronoi::definition_4(),
        voronoi_fractal::definition(),
        waveform::definition(),
        waveform::definition_1(),
        white_noise::definition(),
        white_noise_fast::definition(),
    ]
}
