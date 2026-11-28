use super::ATOMIC;
use super::input_resize::sampled_input;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Image;

pub(super) fn definition() -> NodeDefinition {
    node("sharpen", "Sharpen", ATOMIC)
        .desc("Sharpens local contrast with a four-neighbor Laplacian.")
        .atomic()
        .any_in("input", "Input")
        .float("intensity", "Intensity", 1.0, 0.0, 10.0)
        .any_out()
        .cook(sharpen)
}

fn sharpen(context: &CookContext) -> CookResult {
    let (input, format) = sampled_input(context, 0);
    let (width, height) = craft_texture::clamp_size(context.w, context.h);
    let same_size = (width, height) == (input.width(), input.height());
    let intensity = context.f("intensity");
    let tile = context.tiling_axes();
    let short_side = context.w.min(context.h) as f32;
    let horizontal = intensity * short_side / context.w.max(1) as f32;
    let vertical = intensity * short_side / context.h.max(1) as f32;
    let center_weight = 1.0 + 2.0 * (horizontal + vertical);

    Ok(vec![Image::generate(context.w, context.h, input.kind(), |x, y| {
        let offsets = [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)];
        let [center, left, right, top, bottom] = if same_size {
            offsets.map(|(dx, dy)| input.get_axes(i64::from(x) + dx, i64::from(y) + dy, tile))
        } else {
            let u = (x as f32 + 0.5) / width as f32;
            let v = (y as f32 + 0.5) / height as f32;
            offsets.map(|(dx, dy)| {
                let uv = [u + dx as f32 / input.width() as f32, v + dy as f32 / input.height() as f32];
                crate::texture_sampler::sample(&input, uv, tile, format, None)
            })
        };
        std::array::from_fn(|channel| {
            let [center, left, right, top, bottom] = [center[channel], left[channel], right[channel], top[channel], bottom[channel]];
            let sum = center.mul_add(center_weight, -horizontal * left);
            let sum = right.mul_add(-horizontal, sum);
            let sum = top.mul_add(-vertical, sum);
            let result = bottom.mul_add(-vertical, sum);
            if result.is_finite() || ![center, left, right, top, bottom].iter().all(|v| v.is_finite()) {
                return result;
            }
            // A finite HDR plateau can overflow the weighted center before its neighbors cancel.
            let center = f64::from(center);
            let across = 2.0 * center - (f64::from(left) + f64::from(right));
            let down = 2.0 * center - (f64::from(top) + f64::from(bottom));
            (center + across * f64::from(horizontal) + down * f64::from(vertical)) as f32
        })
    })])
}
