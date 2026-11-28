use super::CATEGORY;
use crate::{CookContext, CookResult, NodeDefinition, node};
use craft_texture::Kind;

pub(crate) fn definition() -> NodeDefinition {
    node("auto_levels", "Auto Levels", CATEGORY)
        .desc("Automatically stretches the tonal range so that the darkest pixel becomes black and the brightest becomes white.")
        .any_in("input", "Input")
        .primary_input("input")
        .any_out()
        .cook(cook)
}

fn cook(context: &CookContext<'_>) -> CookResult {
    let image = context.any_or(0, Kind::Gray, [0.0; 4]);
    let kind = image.kind();
    let channels = kind.channels();
    let data = image.data();

    if data.is_empty() {
        return Ok(vec![image]);
    }

    match kind {
        Kind::Gray => {
            let mut min_val = f32::INFINITY;
            let mut max_val = f32::NEG_INFINITY;

            for &val in data {
                if val.is_finite() {
                    if val < min_val {
                        min_val = val;
                    }
                    if val > max_val {
                        max_val = val;
                    }
                }
            }

            if !min_val.is_finite() || !max_val.is_finite() || (max_val - min_val) < 1e-6 {
                // If completely flat, preserve clamped values without division by zero
                return Ok(vec![image.map(Kind::Gray, |pixel| [pixel[0].clamp(0.0, 1.0); 4])]);
            }

            let span = max_val - min_val;
            let inv_span = 1.0 / span;

            Ok(vec![image.map(Kind::Gray, |pixel| {
                let v = ((pixel[0] - min_val) * inv_span).clamp(0.0, 1.0);
                [v, v, v, 1.0]
            })])
        }
        Kind::Color => {
            // Find global min and max across R, G, B channels to preserve color hue balance
            let mut min_val = f32::INFINITY;
            let mut max_val = f32::NEG_INFINITY;

            for chunk in data.chunks_exact(channels) {
                for c in 0..channels.min(3) {
                    if let Some(&val) = chunk.get(c)
                        && val.is_finite()
                    {
                        if val < min_val {
                            min_val = val;
                        }
                        if val > max_val {
                            max_val = val;
                        }
                    }
                }
            }

            if !min_val.is_finite() || !max_val.is_finite() || (max_val - min_val) < 1e-6 {
                return Ok(vec![
                    image.map(Kind::Color, |pixel| [pixel[0].clamp(0.0, 1.0), pixel[1].clamp(0.0, 1.0), pixel[2].clamp(0.0, 1.0), pixel[3].clamp(0.0, 1.0)]),
                ]);
            }

            let span = max_val - min_val;
            let inv_span = 1.0 / span;

            Ok(vec![image.map(Kind::Color, |pixel| {
                let r = ((pixel[0] - min_val) * inv_span).clamp(0.0, 1.0);
                let g = ((pixel[1] - min_val) * inv_span).clamp(0.0, 1.0);
                let b = ((pixel[2] - min_val) * inv_span).clamp(0.0, 1.0);
                let a = pixel[3].clamp(0.0, 1.0);
                [r, g, b, a]
            })])
        }
    }
}
