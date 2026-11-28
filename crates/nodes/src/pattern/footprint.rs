use super::{
    coordinates::PixelPlanes,
    pattern_stamp::PatternStamp,
    region::{Copies, Region},
};
use crate::CookContext;

pub(super) struct Footprint {
    pub sine: f32,
    pub cosine: f32,
    pub radius: [f32; 2],
}

impl Footprint {
    pub fn new(stamp: &PatternStamp) -> Result<Self, String> {
        if stamp.rotation == 0.0 {
            return Ok(Self { sine: stamp.rotation, cosine: 1.0, radius: stamp.size.map(|s| s.abs() * 0.5) });
        }
        let angle = stamp.rotation * std::f32::consts::TAU;
        if !angle.is_finite() {
            return Err("pattern rotation exceeds the finite angle range".into());
        }
        // Sampling uses the pixel phase even when the draw bounds are folded from literals.
        let (sine, cosine) = crate::function::pixel_sin_cos(angle);
        let (bounds_sine, bounds_cosine) = if stamp.literal_geometry && (stamp.rotation * 4.0).fract() != 0.0 {
            let (s, c) = f64::from(angle).sin_cos();
            (s as f32, c as f32)
        } else {
            crate::function::uniform_sin_cos_turns(stamp.rotation)
        };
        let radius = [
            (stamp.size[0] * bounds_cosine).abs() * 0.5 + (stamp.size[1] * bounds_sine).abs() * 0.5,
            (stamp.size[0] * bounds_sine).abs() * 0.5 + (stamp.size[1] * bounds_cosine).abs() * 0.5,
        ];
        Ok(Self { sine, cosine, radius })
    }

    pub fn local(&self, point: [f32; 2], center: [f32; 2], size: [f32; 2]) -> Option<[f32; 2]> {
        let delta = [point[0] - center[0], point[1] - center[1]];
        let local = [(delta[0] * self.cosine + delta[1] * self.sine) * 2.0 / size[0], (-delta[0] * self.sine + delta[1] * self.cosine) * 2.0 / size[1]];
        // For axis-aligned stamps the preceding interval owns a sample on the shared edge.
        let outside = if self.sine == 0.0 || self.cosine == 0.0 {
            // Subtract the radius from the center before testing the open edge at pixel precision.
            (0..2).any(|axis| !self.contains_axis(axis, point[axis], center[axis]))
        } else {
            local.iter().any(|v| *v <= -1.0 || *v > 1.0)
        };
        (!outside).then_some(local)
    }

    pub(super) fn contains_axis(&self, axis: usize, point: f32, center: f32) -> bool {
        point > center - self.radius[axis] && point - center <= self.radius[axis]
    }

    pub(super) fn is_axis_aligned(&self) -> bool {
        (self.sine == 0.0 && self.cosine.abs() == 1.0) || (self.cosine == 0.0 && self.sine.abs() == 1.0)
    }
}

pub(super) struct SampleGrid {
    render: Region,
    tiling: Region,
    dimensions: [u32; 2],
    axes: [bool; 2],
    cull_outside: bool,
}

impl SampleGrid {
    pub fn new(context: &CookContext, opacities: &[f32]) -> Option<Self> {
        // Keep stamps for raster validation if a depth weight overflowed.
        if opacities.iter().any(|weight| !weight.is_finite()) {
            return None;
        }
        let (width, height) = craft_texture::clamp_size(context.w, context.h);
        let axes = context.tiling_axes();
        let cull_outside = context.b("cull_outside");
        let render = Region::new(context.v4("render_region")).ok()?;
        let tiling = Region::tiling(context.v4("tiling_region"), axes, cull_outside).ok()?;
        Some(Self { render, tiling, dimensions: [width, height], axes, cull_outside })
    }

    fn repeats(&self, stamp: &PatternStamp, frame: &Footprint) -> Result<[Copies; 2], String> {
        let center = self.tiling.copy_origin(stamp.center, self.axes);
        self.tiling.repeats(&self.render, center, frame.radius, self.axes)
    }

    pub fn copies_per_axis(&self, stamp: &PatternStamp) -> Option<[u64; 2]> {
        if self.axes == [false; 2] {
            return Some([1; 2]);
        }
        let frame = Footprint::new(stamp).ok()?;
        Some(self.repeats(stamp, &frame).ok()?.map(|axis| axis.count))
    }

    pub fn copy_count(&self, stamp: &PatternStamp) -> Option<u32> {
        let [x, y] = self.copies_per_axis(stamp)?;
        // The measured terminal runs allow an original and one adjacent copy along each axis.
        ((1..=2).contains(&x) && (1..=2).contains(&y)).then(|| (x * y) as u32)
    }

    pub fn misses(&self, stamp: &PatternStamp) -> bool {
        if !(2..=18).contains(&stamp.pattern)
            || !(0..=2).contains(&stamp.blend)
            || !stamp.center.iter().chain(&stamp.size).chain(&stamp.color).chain([&stamp.rotation, &stamp.variation]).all(|v| v.is_finite())
        {
            return false;
        }
        if self.cull_outside && !self.tiling.contains(stamp.center) {
            return true;
        }
        let Ok(frame) = Footprint::new(stamp) else { return false };
        // A footprint wider than two sample spacings in both directions usually covers a pixel.
        if (0..2).all(|i| frame.radius[i] * self.dimensions[i] as f32 > self.render.period()[i]) {
            return false;
        }
        let Ok(repeats) = self.repeats(stamp, &frame) else { return false };
        let copies = repeats[0].count.saturating_mul(repeats[1].count);
        if copies == 0 {
            return true;
        }
        // A small tiling period can place many copies in a single pixel. Leave those to the raster budget.
        if copies > 4 {
            return false;
        }
        let circular = super::shape::circular(stamp.pattern);
        let columns = repeats[0].centers();
        for y_center in repeats[1].centers() {
            for x_center in columns.clone() {
                let center = [x_center, y_center];
                let bounds = self.render.bounds(center, frame.radius, self.dimensions);
                let [[left, right], [top, bottom]] = bounds;
                if left == right || top == bottom {
                    continue;
                }
                // Bound the search for long, thin stamps that cross the render window.
                if u64::from(right - left) * u64::from(bottom - top) > 16 {
                    return false;
                }
                // Skip the copy when even the final sample precedes the snapped triangle
                // and lies outside the ordinary footprint used for extreme vertex spans.
                if frame.is_axis_aligned()
                    && (0..2).any(|axis| {
                        let low = center[axis] - frame.radius[axis];
                        let end = bounds[axis][1];
                        let pixels = self.dimensions[axis];
                        let near = self.render.stamp_sample_vertices(axis, center[axis], frame.radius[axis], pixels)[0];
                        f64::from(near) + 1.0 >= f64::from(end) && self.render.pixel_center(axis, end - 1, pixels) <= low
                    })
                {
                    continue;
                }
                if let Some(planes) = PixelPlanes::new(&frame, &self.render, center, stamp.size, self.dimensions) {
                    if planes.covers_any(bounds, circular) {
                        return false;
                    }
                    continue;
                }
                for y in top..bottom {
                    for x in left..right {
                        if frame
                            .local(self.render.point([x, y], self.dimensions), center, stamp.size)
                            .is_some_and(|uv| !(circular && super::shape::radius_squared(uv) > 1.0))
                        {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }
}
