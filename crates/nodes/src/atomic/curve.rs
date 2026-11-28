use super::ATOMIC;
use crate::{CookContext, CookResult, NodeDefinition, node, texture_sampler};
use craft_texture::{Image, Kind, Limits};
use materialcraft_core::{Curve, OutputFormat};

const TABLE_WIDTH: usize = 1024;

pub(super) fn definition() -> NodeDefinition {
    node("curve", "Curve", ATOMIC).desc("Remaps values through a tone curve.").atomic().any_in("input", "Input").curve("curve", "Curve").any_out().cook(curve)
}

fn curve(context: &CookContext) -> CookResult {
    let image = context.any_or(0, Kind::Gray, [0.0; 4]);
    let table = lookup_table(&context.curve("curve"))?;
    let lookup = |value: f32| {
        let u = (value.clamp(0.0, 1.0) * (TABLE_WIDTH - 1) as f32 + 0.5) / TABLE_WIDTH as f32;
        texture_sampler::sample(&table, [u, 0.5], [false; 2], OutputFormat::F32, None)[0]
    };
    Ok(vec![image.map(image.kind(), |p| if image.is_gray() { [lookup(p[0]); 4] } else { [lookup(p[0]), lookup(p[1]), lookup(p[2]), p[3]] })])
}

fn lookup_table(curve: &Curve) -> Result<Image, String> {
    let step = 1.0 / (TABLE_WIDTH - 1) as f32;
    let position = |i: usize| i as f32 * step;
    let mut entries = Vec::with_capacity(TABLE_WIDTH);
    let fill_anchor = |entries: &mut Vec<f32>, x, y| {
        while entries.len() < TABLE_WIDTH && position(entries.len()) <= x + 0.5 * step {
            entries.push(y);
        }
    };
    if let Some(first) = curve.points.first() {
        fill_anchor(&mut entries, first.x, first.y);
        let mut last_y = first.y;
        for pair in curve.points.windows(2) {
            let [left, right] = pair else { continue };
            if entries.len() == TABLE_WIDTH {
                break;
            }
            if right.x <= left.x {
                continue;
            }
            let slope = (right.y - left.y) / (right.x - left.x);
            // Even anchors closer than one table texel leave one sample of their segment.
            loop {
                let x = position(entries.len());
                let y = if left.right_broken && right.left_broken { (x - left.x) * slope + left.y } else { curve.eval(x) };
                entries.push(y);
                if entries.len() == TABLE_WIDTH || position(entries.len()) >= right.x - 0.5 * step {
                    break;
                }
            }
            fill_anchor(&mut entries, right.x, right.y);
            last_y = right.y;
        }
        entries.resize(TABLE_WIDTH, last_y);
    } else {
        entries.extend((0..TABLE_WIDTH).map(position));
    }
    Image::from_data(TABLE_WIDTH as u32, 1, Kind::Gray, entries, &Limits::default()).map_err(|error| error.to_string())
}
