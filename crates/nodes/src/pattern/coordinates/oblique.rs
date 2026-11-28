use super::{Footprint, Triangle, aligned_sum, gradient_product, gradient_significand_product, leading_power, plane_origin};
use crate::function::{pixel_divide, truncate_float};

pub(super) struct Plane {
    base: f64,
    step: [f64; 2],
    pair_offset: [[f64; 2]; 2],
    quantum: f64,
    inverse_quantum: f64,
}

impl Plane {
    pub(super) fn y_term(&self, y: u32, dy: f64) -> f64 {
        dy * self.step[1] + self.pair_offset[1][(y & 1) as usize]
    }

    pub(super) fn sample(&self, x: u32, dx: f64, y_term: f64) -> f32 {
        let value = self.base + dx * self.step[0] + self.pair_offset[0][(x & 1) as usize] + y_term;
        let fixed = (value * self.inverse_quantum).floor();
        truncate_float((if fixed < 0.0 { fixed + 1.0 } else { fixed }) * self.quantum)
    }
}

pub(super) fn planes(triangle: &Triangle, frame: &Footprint, size: [f32; 2], [[left, right], [top, bottom]]: [[f32; 2]; 2]) -> Option<[Plane; 2]> {
    let scale = size.map(|s| pixel_divide(2.0, s));
    let vertices = [[left, top], [right, top], [left, bottom]]
        .map(|[x, y]| [y.mul_add(frame.sine, x * frame.cosine) * scale[0], y.mul_add(frame.cosine, -x * frame.sine) * scale[1]]);
    if vertices.iter().flatten().any(|v| !v.is_finite()) {
        return None;
    }
    let anchor = triangle.far_axis.map_or(0, |axis| axis + 1);
    let square = triangle.far[0] - triangle.near[0] == triangle.far[1] - triangle.near[1];
    let planes: [Plane; 2] = std::array::from_fn(|component| {
        let gradient: [f32; 2] = std::array::from_fn(|axis| {
            let start = vertices[0][component];
            let end = vertices[axis + 1][component];
            let basis = triangle.basis[axis];
            if square && anchor != 0 && anchor != axis + 1 {
                // Across the far edge of a square, the two slope terms share the
                // opposite vertex. Align their wide products before subtracting.
                let vertex = vertices[anchor][component];
                let (first, first_scale) = wide_slope(basis, half_span(end, vertex));
                let (second, second_scale) = wide_slope(basis, half_span(start, vertex));
                return truncate_float(aligned_sum([first, -second], first_scale.max(second_scale), 2));
            }
            let span = half_span(end, start);
            let exponents = (span.abs().to_bits() >> 23) + (basis.to_bits() >> 23);
            // The partial-product path assumes a normal, finite result at the stored exponent.
            if !span.is_normal() || !(128..=380).contains(&exponents) {
                return truncate_float(f64::from(basis) * f64::from(span));
            }
            gradient_product(basis, span.abs()).copysign(span)
        });
        let units = gradient.map(|v| f64::from(v.abs().next_up()) - f64::from(v.abs()));
        let base = plane_origin(vertices[anchor][component], triangle.offset, gradient, units);
        let step = gradient.map(f64::from);
        let unit = units[0].max(units[1]);
        // The two samples of a pair share the value at their midpoint. Its signed
        // half-step discards gradient bits below a quarter of the larger gradient's ULP.
        let quarter = unit * 0.25;
        let pair_offset = step.map(|v| {
            let half_remainder = (v - (v / quarter).trunc() * quarter) * 0.5;
            [half_remainder, -half_remainder]
        });
        let quantum = unit / 64.0;
        let inverse_quantum = quantum.recip();
        // A narrow stamp can have a tiny origin between steep steps. Discard its
        // sub-quantum bits before adding either step, including for negative origins.
        let base = (f64::from(base) * inverse_quantum).trunc() * quantum;
        Plane { base, step, pair_offset, quantum, inverse_quantum }
    });
    planes.iter().all(|p| p.quantum.is_finite()).then_some(planes)
}

fn half_span(end: f32, start: f32) -> f32 {
    let (end, start) = (f64::from(end), f64::from(start));
    truncate_float(aligned_sum([end, -start], leading_power(start.abs().max(end.abs())), 1) * 0.5)
}

fn wide_slope(basis: f32, span: f32) -> (f64, f64) {
    let scale = leading_power(f64::from(basis)) * leading_power(f64::from(span));
    if !span.is_normal() || !basis.is_normal() {
        return (f64::from(span) * f64::from(basis), scale);
    }
    let product = gradient_significand_product(basis, span.abs()) as f64;
    let slope = product * (scale / (1_u64 << 46) as f64);
    (slope.copysign(f64::from(span)), scale)
}
