use super::{
    footprint::Footprint,
    pattern_stamp::{DrawRun, PatternStamp},
    region::Region,
    tile_runs::COPIES_PER_BATCH,
};
use crate::CookContext;

pub(super) fn runs<'a>(context: &CookContext, stamps: &'a [PatternStamp], repeats: usize, depth: u32) -> Option<Vec<DrawRun<'a>>> {
    if !(6..=10).contains(&depth) || repeats == 0 {
        return None;
    }
    let first_level = repeats.checked_mul(4)?;
    if first_level > COPIES_PER_BATCH as usize || first_level > stamps.len() {
        return None;
    }
    let first = stamps.iter().find(|stamp| stamp.depth == 1)?;
    if !(2..=16).contains(&first.pattern) {
        return None;
    }

    let axes = context.tiling_axes();
    let render = Region::new(context.v4("render_region")).ok()?;
    let tiling = Region::tiling(context.v4("tiling_region"), axes, context.b("cull_outside")).ok()?;
    let mut submitted = 0_u64;
    let mut indices = Vec::with_capacity(first_level);
    for (index, stamp) in stamps.iter().enumerate() {
        if stamp.depth == 0 {
            return None;
        }
        if stamp.depth == 1 {
            if stamp.pattern != first.pattern || stamp.blend != 0 || indices.len() == first_level {
                return None;
            }
            let frame = Footprint::new(stamp).ok()?;
            let center = tiling.copy_origin(stamp.center, axes);
            let [x, y] = tiling.repeats(&render, center, frame.radius, axes).ok()?;
            submitted = submitted.checked_add(x.count.checked_mul(y.count)?)?;
            if submitted > u64::from(COPIES_PER_BATCH) {
                return None;
            }
            indices.push(index);
        } else if stamp.pattern == first.pattern {
            // A shared shape also needs the submission order of its deeper draws.
            return None;
        }
    }
    // Without a nested Iterate, four retained first-level stamps per repetition
    // account for every child, including the two slots belonging to the lower row.
    if indices.len() != first_level {
        return None;
    }

    // Other shapes keep their relative order. The raster sorts each shape separately.
    let mut runs = Vec::new();
    let mut start = 0;
    for &index in &indices {
        if start < index {
            runs.push(DrawRun::whole(stamps.get(start..index)?));
        }
        start = index + 1;
    }
    if let Some(rest) = stamps.get(start..).filter(|rest| !rest.is_empty()) {
        runs.push(DrawRun::whole(rest));
    }
    for slot in (0..4).rev() {
        for iteration in indices.as_chunks::<4>().0 {
            let index = *iteration.get(slot)?;
            runs.push(DrawRun::whole(stamps.get(index..=index)?));
        }
    }
    Some(runs)
}
