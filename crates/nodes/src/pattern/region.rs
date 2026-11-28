/// FX-Map regions use left, right, top, bottom coordinates in pattern space.
pub(super) struct Region {
    start: [f32; 2],
    end: [f32; 2],
    span: [f32; 2],
}

#[derive(Clone, Copy)]
pub(super) struct Copies {
    pub count: u64,
    outermost: f32,
    period: f32,
}

impl Copies {
    pub fn centers(self) -> impl Iterator<Item = f32> + Clone {
        (0..self.count).scan(self.outermost, move |next, _| {
            let current = *next;
            *next -= self.period;
            Some(current)
        })
    }
}

impl Region {
    pub fn tiling(mut value: [f32; 4], axes: [bool; 2], cull_outside: bool) -> Result<Self, String> {
        for (interval, enabled) in value.as_chunks_mut::<2>().0.iter_mut().zip(axes) {
            if !enabled && !cull_outside {
                *interval = [0.0, 1.0];
            }
        }
        Self::new(value)
    }

    pub fn new(value: [f32; 4]) -> Result<Self, String> {
        let mut start = [value[0], value[2]];
        let mut end = [value[1], value[3]];
        let mut span = [end[0] - start[0], end[1] - start[1]];
        if value.iter().any(|value| !value.is_finite()) || span.iter().any(|value| !value.is_finite() || value.abs() < f32::EPSILON) {
            return Err("pattern regions must be finite with nonzero width and height".into());
        }

        for axis in 0..2 {
            if span[axis] < 0.0 {
                // A reversed interval selects the full axis in pattern space.
                start[axis] = 0.0;
                end[axis] = 1.0;
                span[axis] = 1.0;
            }
        }
        Ok(Self { start, end, span })
    }

    pub fn contains(&self, point: [f32; 2]) -> bool {
        point.into_iter().enumerate().all(|(axis, value)| (self.start[axis]..self.end[axis]).contains(&value))
    }

    pub fn point(&self, pixel: [u32; 2], size: [u32; 2]) -> [f32; 2] {
        std::array::from_fn(|axis| self.pixel_center(axis, pixel[axis], size[axis]))
    }

    pub fn pixel_center(&self, axis: usize, pixel: u32, size: u32) -> f32 {
        self.start[axis] + (pixel as f32 + 0.5) / size as f32 * self.span[axis]
    }

    pub fn stamp_sample_vertices(&self, axis: usize, center: f32, radius: f32, pixels: u32) -> [f32; 2] {
        let scale = pixels as f32 / self.span[axis];
        // Sample zero is the first pixel center. Rounding before its half-pixel offset
        // changes the snapped edge and the interpolation slope in scaled windows.
        [center - radius, center + 3.0 * radius].map(|v| (v - self.start[axis]).mul_add(scale, -0.5))
    }

    pub fn bounds(&self, center: [f32; 2], radius: [f32; 2], size: [u32; 2]) -> [[u32; 2]; 2] {
        std::array::from_fn(|axis| {
            let a = (center[axis] - radius[axis] - self.start[axis]) / self.span[axis];
            let b = (center[axis] + radius[axis] - self.start[axis]) / self.span[axis];
            [
                (a.min(b) * size[axis] as f32).floor().clamp(0.0, size[axis] as f32) as u32,
                (a.max(b) * size[axis] as f32).ceil().clamp(0.0, size[axis] as f32) as u32,
            ]
        })
    }

    pub fn period(&self) -> [f32; 2] {
        self.span
    }

    pub fn copy_origin(&self, center: [f32; 2], axes: [bool; 2]) -> [f32; 2] {
        std::array::from_fn(|i| {
            let start = f64::from(self.start[i]);
            let offset = f64::from(center[i]) - start;
            let period = f64::from(self.span[i]);
            // Adjacent copy indices cease to be distinct F32 translations at 2^24.
            if axes[i] && offset.abs() >= 16_777_216.0 * period { (start + offset.rem_euclid(period)) as f32 } else { center[i] }
        })
    }

    pub fn repeats(&self, render: &Self, center: [f32; 2], radius: [f32; 2], axes: [bool; 2]) -> Result<[Copies; 2], String> {
        let mut result = center.map(|value| Copies { count: 1, outermost: value, period: 0.0 });
        for axis in 0..2 {
            if !axes[axis] {
                continue;
            }
            let period = f64::from(self.span[axis]);
            let start = f64::from(self.start[axis]);
            let visible = f64::from(render.start[axis]);
            let center = f64::from(center[axis]);
            let radius = f64::from(radius[axis]);
            // A copy must overlap both windows. Touching an edge alone does not draw it.
            let first = ((start.max(visible) - center - radius) / period).floor() + 1.0;
            let last_at = |end| ((f64::from(end) - center + radius) / period).ceil() - 1.0;
            let last = last_at(self.end[axis].min(render.end[axis]));

            // A tiny period or huge pattern cannot create unbounded work, even outside the image
            let coordinates = f64::from(i32::MIN)..=f64::from(i32::MAX);
            if !coordinates.contains(&first) || !coordinates.contains(&last) {
                return Err("pattern repetition exceeds its coordinate budget".into());
            }

            let count = (last - first + 1.0).max(0.0) as u64;
            // The first translation belongs to the full tiling window, even if that copy is offscreen.
            let outer = if count > 0 && self.end[axis] > render.end[axis] { last_at(self.end[axis]) } else { last };
            let period = self.span[axis];
            let position = center as f32 + outer as f32 * period;
            let outermost = subtract_periods(position, period, (outer - last) as u64);
            result[axis] = Copies { count, outermost, period };
        }

        Ok(result)
    }
}

/// Advance a run of offscreen copies with the same rounding as individual F32 subtractions.
fn subtract_periods(mut position: f32, period: f32, mut steps: u64) -> f32 {
    const FRACTION: u32 = (1 << 23) - 1;
    while steps > 0 {
        let next = position - period;
        if steps == 1 || next == position || !next.is_finite() {
            return next;
        }
        let [a, b, c] = [position, next, next - period].map(f32::to_bits);
        let stride = a.abs_diff(b);
        // Within one signed exponent the floats are equally spaced. Two equal strides also
        // resolve a possible ties-to-even step from an odd significand. Stop at the boundary.
        if a >> 23 == b >> 23 && b >> 23 == c >> 23 && stride != 0 && stride == b.abs_diff(c) {
            let fraction = a & FRACTION;
            // At a positive power of two the next lower float is closer. Cross that boundary one step at a time.
            let room = if position.is_sign_negative() { FRACTION - fraction } else { fraction.saturating_sub(1) };
            let jump = steps.min(u64::from(room / stride));
            if jump > 1 {
                let delta = jump as u32 * stride;
                position = f32::from_bits(if position.is_sign_negative() { a + delta } else { a - delta });
                steps -= jump;
                continue;
            }
        }
        position = next;
        steps -= 1;
    }
    position
}

#[cfg(test)]
mod tests {
    use super::subtract_periods;

    #[test]
    fn skipping_copies_keeps_rounding_across_exponents_and_ties() {
        for (start, period, count) in [(16.5, 0.612, 1157), (0.5, 0.125, 451), (-123456.0, 1.5, 8193), (16_777_212.0, 1.5, 530)] {
            let expected = (0..count).fold(start, |position, _| position - period);
            assert_eq!(subtract_periods(start, period, count).to_bits(), expected.to_bits());
        }
        // The float immediately below a positive power of two is only half an ulp away.
        let start = 1.0 + 20.0 * f32::EPSILON;
        assert_eq!(subtract_periods(start, 1.3125 * f32::EPSILON, 20), 1.0_f32.next_down());
        assert_eq!(subtract_periods(0.0, f32::EPSILON, 1 << 31), -2.0);
    }
}
