use crate::function::pixel_sqrt;
use craft_texture::{Image, Kind};
use rayon::prelude::*;

type Site = [u16; 2];
const EMPTY: Site = [u16::MAX; 2];

struct Axis {
    positions: Vec<f32>,
    origin: f32,
    tiled: bool,
    units_per_texel: f32,
}

impl Axis {
    fn new(size: u32, tiled: bool, units_per_texel: f32) -> Self {
        let extent = size as f32;
        // Wrapped coordinates are relative to the midpoint before subtraction.
        let origin = if tiled { 0.5 } else { 0.0 };
        let positions = (0..size)
            .map(|i| {
                // Rounding the scaled center to F32 first can move a site across the half-word boundary.
                let word = ((f64::from(i) + 0.5) * f64::from(u16::MAX) / f64::from(size)).round_ties_even();
                (word as f32 / f32::from(u16::MAX) - origin) * extent
            })
            .collect();
        Self { positions, origin: origin * extent, tiled, units_per_texel }
    }

    fn delta(&self, pixel: usize, site: u16) -> f32 {
        let distance = ((pixel as f32 + 0.5 - self.origin) - self.positions[usize::from(site)]).abs();
        let distance = if self.tiled { distance.min(self.positions.len() as f32 - distance) } else { distance };
        distance * self.units_per_texel
    }

    fn neighbors(&self, step: f64) -> Vec<[Option<usize>; 3]> {
        let extent = self.positions.len() as i32;
        // Sample at texel centers. A half-texel jump can address different offsets in each direction.
        let offsets = [(0.5 - step).floor() as i32, 0, (0.5 + step).floor() as i32];
        (0..extent)
            .map(|i| {
                offsets.map(|offset| {
                    let n = i + offset;
                    if self.tiled { Some(n.rem_euclid(extent) as usize) } else { (0..extent).contains(&n).then_some(n as usize) }
                })
            })
            .collect()
    }
}

struct Geometry {
    columns: Axis,
    rows: Axis,
    metric: i32,
}

impl Geometry {
    fn squared_distance(&self, x: usize, y: usize, site: Site) -> f32 {
        let dx = self.columns.delta(x, site[0]);
        let dy = self.rows.delta(y, site[1]);
        match self.metric {
            1 => (dx + dy) * (dx + dy),
            2 => dx.max(dy) * dx.max(dy),
            _ => dy.mul_add(dy, dx * dx),
        }
    }

    fn propagate(&self, mut sites: Vec<Site>, first_jump: u32) -> Vec<Site> {
        let width = self.columns.positions.len();
        let height = self.rows.positions.len();
        let longest = width.max(height) as f64;
        let mut next = vec![EMPTY; sites.len()];
        let jumps = (0..=first_jump.ilog2()).rev().map(|power| {
            let fraction = f64::from(1u32 << power) / longest;
            [fraction * width as f64, fraction * height as f64]
        });
        // A final unit step fills missed sites on open fields. It changes measured winners on tiled fields.
        let refine = (!self.columns.tiled && !self.rows.tiled).then_some([1.0; 2]);
        for [step_x, step_y] in jumps.chain(refine) {
            let xs = self.columns.neighbors(step_x);
            let ys = self.rows.neighbors(step_y);
            next.par_chunks_mut(width).enumerate().for_each(|(y, output)| {
                let [above, row, below] = ys[y];
                for (x, dest) in output.iter_mut().enumerate() {
                    let mut best = sites[y * width + x];
                    if best != [x as u16, y as u16] {
                        let mut nearest = if best == EMPTY { f32::INFINITY } else { self.squared_distance(x, y, best) };
                        let [left, column, right] = xs[x];
                        // Equal distances keep the current winner. Visit opposing axial pairs before diagonal pairs.
                        let neighbors =
                            [(right, row), (left, row), (column, below), (column, above), (right, below), (left, above), (right, above), (left, below)];
                        for (sx, sy) in neighbors {
                            let Some((sx, sy)) = sx.zip(sy) else { continue };
                            let candidate = sites[sy * width + sx];
                            if candidate != EMPTY && candidate != best {
                                let d = self.squared_distance(x, y, candidate);
                                if d < nearest {
                                    nearest = d;
                                    best = candidate;
                                }
                            }
                        }
                    }
                    *dest = best;
                }
            });
            std::mem::swap(&mut sites, &mut next);
        }
        sites
    }
}

fn attenuate(mut pixel: [f32; 4], gray: bool, weight: f32) -> [f32; 4] {
    pixel[if gray { 0 } else { 3 }] *= weight;
    pixel
}

pub(super) fn render(mask: &Image, source: Option<&Image>, radius: f32, tile: [bool; 2], metric: i32, combine: bool, stretched: bool) -> Result<Image, String> {
    let (width, height) = (mask.width(), mask.height());
    // Reserve the final coordinate pair as the missing-site marker.
    if width == 0 || height == 0 || width > u32::from(u16::MAX) || height > u32::from(u16::MAX) {
        return Err("distance mask dimensions exceed the supported coordinate range".into());
    }
    let empty = !mask.data().iter().any(|v| *v >= 0.5);
    let tiled = tile[0] || tile[1];
    if empty || radius == 0.0 {
        let result = match source {
            None => Image::filled(width, height, Kind::Gray, [0.0; 4]),
            Some(src) if empty && !tiled => Image::filled(width, height, src.kind(), src.get(0, 0, false)),
            Some(src) if combine => Image::generate(width, height, src.kind(), |x, y| attenuate(src.get(x.into(), y.into(), false), src.is_gray(), 0.0)),
            Some(src) => src.clone(),
        };
        return Ok(result);
    }
    if mask.data().iter().all(|v| *v >= 0.5) {
        return Ok(source.cloned().unwrap_or_else(|| Image::filled(width, height, Kind::Gray, [1.0; 4])));
    }
    // In stretched space the same UV displacement spans different texel counts on the two axes.
    let short_side = width.min(height) as f32;
    let units = if stretched { [short_side / width as f32, short_side / height as f32] } else { [1.0; 2] };
    let reach = radius / units[0].min(units[1]);
    // With a subpixel reach on both axes no site can reach a neighboring texel. This also keeps a
    // tiny positive radius from overflowing its reciprocal at a site of distance zero.
    if reach < 0.5 {
        let weight = |x: u32, y: u32| if mask.get_gray(x.into(), y.into(), false) >= 0.5 { 1.0 } else { 0.0 };
        return Ok(match source {
            None => Image::gray(width, height, weight),
            Some(src) if combine => {
                Image::generate(width, height, src.kind(), |x, y| attenuate(src.get(x.into(), y.into(), false), src.is_gray(), weight(x, y)))
            }
            Some(src) => src.clone(),
        });
    }

    let w = width as usize;
    let initial = mask.data().iter().enumerate().map(|(i, &v)| if v >= 0.5 { [(i % w) as u16, (i / w) as u16] } else { EMPTY }).collect();
    let geometry = Geometry { columns: Axis::new(width, tile[0], units[0]), rows: Axis::new(height, tile[1], units[1]), metric };
    let long_side = width.max(height);
    let search_span = radius * (long_side as f32 / short_side);
    let first_jump = (search_span.ceil() as u32).clamp(1, long_side).next_power_of_two();
    let sites = geometry.propagate(initial, first_jump);
    let inverse_radius = radius.recip();
    let site_distance = |x: u32, y: u32| {
        let site = sites[y as usize * w + x as usize];
        let distance = if site == [x as u16, y as u16] {
            0.0
        } else if site == EMPTY {
            f32::INFINITY
        } else {
            pixel_sqrt(geometry.squared_distance(x as usize, y as usize, site))
        };
        (site, distance)
    };
    let mask_weight = |distance: f32| (-distance).mul_add(inverse_radius, 1.0).clamp(0.0, 1.0);
    let Some(source) = source else {
        return Ok(Image::gray(width, height, |x, y| mask_weight(site_distance(x, y).1)));
    };
    Ok(Image::generate(width, height, source.kind(), |x, y| {
        let (site, distance) = site_distance(x, y);
        let pixel = if distance < radius { source.get(site[0].into(), site[1].into(), false) } else { source.get(x.into(), y.into(), false) };
        if !combine {
            return pixel;
        }
        // The tiled path stores the complement after rounding the mask gradient.
        let weight = if tiled { 1.0 - (1.0 - mask_weight(distance)) } else { (1.0 - distance * inverse_radius).clamp(0.0, 1.0) };
        attenuate(pixel, source.is_gray(), weight)
    }))
}
