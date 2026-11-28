use crate::{CookContext, find, resolve_params};
use craft_texture::Image;
use std::collections::BTreeMap;

#[test]
fn test_flood_fill_pipeline() {
    // 32x32 image with two separate white squares
    let mask = Image::gray(32, 32, |x, y| {
        let in_sq1 = (4..12).contains(&x) && (4..12).contains(&y);
        let in_sq2 = (20..28).contains(&x) && (20..28).contains(&y);
        if in_sq1 || in_sq2 { 1.0 } else { 0.0 }
    });

    let ff_def = find("flood_fill").expect("flood_fill definition");
    let no_res = |_: &str| None;
    let vars = BTreeMap::new();
    let overrides = BTreeMap::new();
    let params = resolve_params(ff_def, &overrides);
    let inputs = [Some(mask)];

    let ctx = CookContext {
        format: materialcraft_core::OutputFormat::F32,
        w: 32,
        h: 32,
        seed: 0,
        tile: false,
        inputs: &inputs,
        params: &params,
        resource: &no_res,
        vars: &vars,
    };

    let ff_res = (ff_def.cook)(&ctx).expect("flood fill cook");
    assert_eq!(ff_res.len(), 1);
    let ff_data = &ff_res[0];

    // Background at (0, 0) should be zero
    let bg = ff_data.get(0, 0, false);
    assert_eq!(bg, [0.0, 0.0, 0.0, 0.0]);

    // Square 1 center should have centroid near (8/32, 8/32) = (0.25, 0.25)
    let p1 = ff_data.get(8, 8, false);
    assert!((p1[0] - 0.25).abs() < 0.05);
    assert!((p1[1] - 0.25).abs() < 0.05);

    // Square 2 center should have centroid near (24/32, 24/32) = (0.75, 0.75)
    let p2 = ff_data.get(24, 24, false);
    assert!((p2[0] - 0.75).abs() < 0.05);
    assert!((p2[1] - 0.75).abs() < 0.05);

    // Test flood_fill_to_random_color
    let rand_def = find("flood_fill_to_random_color").expect("random color def");
    let rand_inputs = [Some(ff_data.clone())];
    let rand_params = resolve_params(rand_def, &overrides);
    let rand_ctx = CookContext {
        format: materialcraft_core::OutputFormat::F32,
        w: 32,
        h: 32,
        seed: 0,
        tile: false,
        inputs: &rand_inputs,
        params: &rand_params,
        resource: &no_res,
        vars: &vars,
    };
    let rand_res = (rand_def.cook)(&rand_ctx).expect("random color cook");
    let rand_img = &rand_res[0];
    let c1 = rand_img.get(8, 8, false);
    let c2 = rand_img.get(24, 24, false);
    assert_ne!(c1[0], c2[0], "Different islands should receive different random values");

    // Test flood_fill_to_gradient
    let grad_def = find("flood_fill_to_gradient").expect("gradient def");
    let grad_params = resolve_params(grad_def, &overrides);
    let grad_ctx = CookContext {
        format: materialcraft_core::OutputFormat::F32,
        w: 32,
        h: 32,
        seed: 0,
        tile: false,
        inputs: &rand_inputs,
        params: &grad_params,
        resource: &no_res,
        vars: &vars,
    };
    let grad_res = (grad_def.cook)(&grad_ctx).expect("gradient cook");
    let grad_img = &grad_res[0];
    let g_left = grad_img.get_gray(5, 8, false);
    let g_right = grad_img.get_gray(11, 8, false);
    assert!(g_right > g_left, "Horizontal gradient should increase along X: left={g_left}, right={g_right}");
}
