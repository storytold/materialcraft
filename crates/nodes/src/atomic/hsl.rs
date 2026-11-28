use super::ATOMIC;
use crate::function::pixel_divide;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(super) fn definition() -> NodeDefinition {
    node("hsl", "HSL", ATOMIC)
        .desc("Adjust hue, saturation and lightness around the midpoint of each slider.")
        .atomic()
        .color_in("input", "Input")
        .float("hue", "Hue", 0.5, 0.0, 1.0)
        .float("saturation", "Saturation", 0.5, 0.0, 1.0)
        .float("lightness", "Lightness", 0.5, 0.0, 1.0)
        .color_out()
        .cook(hsl)
}

fn hsl(context: &CookContext) -> CookResult {
    let image = context.any_or(0, Kind::Color, [0.0; 4]).to_color();
    let hue_shift = context.f("hue") * 12.0 - 6.0;
    let [saturation_shift, lightness_shift] = ["saturation", "lightness"].map(|name| (context.f(name) - 0.5) * 2.0);

    Ok(vec![image.map(Kind::Color, |[red, green, blue, alpha]| {
        let gain = red.max(green).max(blue).max(1.0);
        let scale = pixel_divide(1.0, gain);
        let [r, g, b] = [red * scale, green * scale, blue * scale];
        let low = r.min(g).min(b);
        let high = r.max(g).max(b);
        let chroma = high - low;
        let lightness = (low + high) * 0.5;

        // The nonzero divisors also affect ordinary colors at the slider midpoints.
        const EPSILON: f32 = 1.0 / 4096.0;
        let chroma_range = |lightness: f32| 1.0 - (2.0 * lightness - 1.0).abs();
        let s = if chroma == 0.0 { saturation_shift } else { chroma.mul_add(pixel_divide(1.0, chroma_range(lightness) + EPSILON), saturation_shift) };
        let s = s.clamp(0.0, 1.0);
        let l = (lightness + lightness_shift).clamp(0.0, 1.0);

        let (left, right, sector) = if low == r {
            (b, g, 3.0)
        } else if low == g {
            (r, b, 5.0)
        } else {
            (g, r, 1.0)
        };
        // Round the bias with the subtracted channel. Raising saturation makes
        // this rounding visible even when all three input channels are equal.
        let difference = left - (right + 3.0 * EPSILON);
        let sextants = difference.mul_add(pixel_divide(1.0, chroma + EPSILON), sector) + hue_shift;
        // Remove whole turns in the multiply-add. Rounding the product first loses the residual at the wrap.
        const SIXTH: f32 = 1.0 / 6.0;
        let hue = sextants.mul_add(SIXTH, -(sextants * SIXTH).floor());
        let upper = s.mul_add(0.5 * chroma_range(l), l);
        let lower = 2.0 * l - upper;
        let channel = |weight: f32| (upper - lower).mul_add(weight.clamp(0.0, 1.0), lower) * gain;
        let red = (hue - 0.5).abs().mul_add(6.0, -1.0);
        let green = (hue - 1.0 / 3.0).abs().mul_add(-6.0, 2.0);
        let blue = (hue - 2.0 / 3.0).abs().mul_add(-6.0, 2.0);
        [channel(red), channel(green), channel(blue), alpha]
    })])
}
