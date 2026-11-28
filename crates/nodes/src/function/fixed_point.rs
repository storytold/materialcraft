pub(super) struct Quadratic {
    pub constant: i64,
    pub slope: i64,
    pub curvature: i64,
}

impl Quadratic {
    // Fit one interval in local coordinates t=0..1. The derivatives are taken at t=1/2.
    pub fn fit(value: impl Fn(f64) -> f64, [first, second, third]: [f64; 3], [a, b, c]: [i32; 3]) -> Self {
        let curvature_scale = 2_f64.powi(c);
        let ratio = 1_i64 << (c - b);
        // On [-1/2,1/2] the linear minimax approximation of the centered cubic is 3t/16.
        let center_slope = curvature_scale * (first + third / 32.0);
        let slope = ((center_slope - curvature_scale * second * 0.5) / ratio as f64).round() as i64;
        let curvature = center_slope.round() as i64 - ratio * slope;
        let constant_scale = 2_f64.powi(a);
        let linear = 2_f64.powi(a - b) * slope as f64;
        let square = 2_f64.powi(a - c) * curvature as f64;
        let mut low = f64::INFINITY;
        let mut high = f64::NEG_INFINITY;
        for step in 0..=64 {
            let t = f64::from(step) / 64.0;
            let residual = constant_scale * value(t) - linear * t - square * t * t;
            low = low.min(residual);
            high = high.max(residual);
        }
        Self { constant: ((low + high) * 0.5).round() as i64, slope, curvature }
    }
}

pub(crate) fn truncate_float(value: f64) -> f32 {
    let nearest = value as f32;
    if f64::from(nearest).abs() > value.abs() { f32::from_bits(nearest.to_bits().saturating_sub(1)) } else { nearest }
}

// Square a 16-bit fraction, retaining partial products in the top 15 fractional columns.
// A pair of distinct bits contributes twice. A diagonal bit contributes once.
pub(super) fn square_fraction(fraction: u32) -> u32 {
    let mut square = 0;
    for bit in 1..=7 {
        if fraction & (1 << (16 - bit)) != 0 {
            let width = 16 - 2 * bit;
            square += (1 << (width - 1)) + ((fraction >> bit) & ((1 << width) - 1));
        }
    }
    square
}
