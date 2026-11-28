use super::ATOMIC;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(super) fn definition() -> NodeDefinition {
    node("grayscale_conversion", "Grayscale conversion", ATOMIC)
        .desc("Weighted sum of the RGBA channels.")
        .atomic()
        .color_in("input", "Input")
        .float("red", "Red weight", 1.0 / 3.0, 0.0, 1.0)
        .float("green", "Green weight", 1.0 / 3.0, 0.0, 1.0)
        .float("blue", "Blue weight", 1.0 / 3.0, 0.0, 1.0)
        .float("alpha", "Alpha weight", 0.0, 0.0, 1.0)
        .boolean("flatten_alpha", "Flatten alpha", false)
        .float("matte", "Matte level", 1.0, 0.0, 1.0)
        .gray_out()
        .cook(grayscale_conversion)
}

fn grayscale_conversion(context: &CookContext) -> CookResult {
    let image = context.any_or(0, Kind::Color, [0.0, 0.0, 0.0, 1.0]).to_color();
    let weights = context
        .value("weights")
        .and_then(materialcraft_core::Value::as_vec4)
        .unwrap_or_else(|| [context.f("red"), context.f("green"), context.f("blue"), context.f("alpha")]);
    let flatten = context.b("flatten_alpha");
    let matte = context.f("matte");
    Ok(vec![image.map(Kind::Gray, |pixel| {
        // Preserve the fused, ordered dot product through float image storage
        let mut value = pixel.into_iter().zip(weights).fold(0.0, |sum, (channel, weight)| channel.mul_add(weight, sum));
        if flatten {
            value = (matte - value).mul_add(1.0 - pixel[3], value);
        }
        [value, 0.0, 0.0, 0.0]
    })])
}
