//! Render package font outlines in normalized text coordinates.
use std::collections::BTreeMap;

use craft_font_catalog::{FontOutline, RunGlyph, RunInput, ShapePlanCache, shape_run};
use harfrust::{Direction, Shaper};
use read_fonts::{TableProvider, tables::glyf::Glyph as StoredGlyph, types::GlyphId};
use vello_common::{
    fearless_simd::Level,
    geometry::RectU16,
    kurbo::{Affine, BezPath, ParamCurve, PathEl, Point, QuadBez, Rect, Shape, Vec2},
    peniko::Fill,
    strip::{StripFillSegment, visit_strip_fill_segments},
    strip_generator::{StripGenerator, StripStorage},
    tile::Tile,
};

use crate::{CookContext, CookResult, raster::composite_text};

// A short string can repeat a complex glyph across many wrapping tiles. Bound the
// outline storage and submitted path elements independently of the character count.
const MAX_ELEMENTS: usize = 1_000_000;

struct Glyph {
    path: BezPath,
    bounds: Rect,
}

impl Glyph {
    fn new(mut path: BezPath, simple_contour: bool, pixels_per_unit: f64, remaining: &mut usize) -> Result<Self, String> {
        let budget_error = "font exceeds the outline storage budget";
        *remaining = remaining.checked_sub(path.elements().len()).ok_or(budget_error)?;
        if simple_contour {
            // Stored TrueType points have integer design coordinates. The join between
            // adjacent controls, including a contour's starting join, uses the lower unit.
            for element in path.elements_mut() {
                if let PathEl::MoveTo(join) | PathEl::QuadTo(_, join) = element {
                    *join = join.floor();
                }
            }
        }
        let bounds = path.bounding_box();
        if !path.elements().iter().any(|element| matches!(element, PathEl::QuadTo(..))) {
            return Ok(Self { path, bounds });
        }

        let mut raster = BezPath::new();
        let mut current = Point::ORIGIN;
        let mut start = Point::ORIGIN;
        for &element in path.elements() {
            match element {
                PathEl::MoveTo(point) => {
                    start = point;
                    current = point;
                }
                PathEl::LineTo(point) | PathEl::CurveTo(_, _, point) => current = point,
                PathEl::ClosePath => current = start,
                PathEl::QuadTo(control, end) => {
                    // Each bisection quarters the distance between the curve midpoint and the chord.
                    let mut deviation = control.distance(current.midpoint(end)) * 0.5 * pixels_per_unit;
                    let mut segments = 1;
                    while deviation > 0.35 {
                        *remaining = remaining.checked_sub(segments).ok_or(budget_error)?;
                        segments *= 2;
                        deviation *= 0.25;
                    }
                    let curve = QuadBez { p0: current, p1: control, p2: end };
                    for step in 1..=segments {
                        raster.line_to(curve.eval(step as f64 / segments as f64));
                    }
                    current = end;
                    continue;
                }
            }
            raster.push(element);
        }
        Ok(Self { path: raster, bounds })
    }
}

fn shape_line(line: &str, shaper: &Shaper<'_>, plans: &ShapePlanCache) -> Result<Vec<RunGlyph>, String> {
    let bidi = unicode_bidi::BidiInfo::new(line, None);
    let mut glyphs = Vec::new();
    for paragraph in &bidi.paragraphs {
        let (levels, runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
        for range in runs {
            let (Some(text), Some(before), Some(after)) = (line.get(range.clone()), line.get(..range.start), line.get(range.end..)) else {
                return Err("invalid text shaping range".into());
            };
            let direction = if levels.get(range.start).is_some_and(|level| level.is_rtl()) { Direction::RightToLeft } else { Direction::LeftToRight };
            let input = RunInput {
                text,
                source_start: range.start,
                pre_context: before,
                post_context: after,
                direction,
                language: None,
                features: &[],
                character_clusters: false,
                uppercase: false,
            };
            let run = shape_run(shaper, &input, |ch| ch, |face, dir, script, lang, features| plans.get_or_create(face, None, dir, script, lang, features))
                .map_err(|e| e.to_string())?;
            glyphs.extend(run);
        }
    }
    Ok(glyphs)
}

fn copies(low: f64, high: f64, period: f64, tiled: bool) -> Result<std::ops::RangeInclusive<i32>, String> {
    if !tiled {
        return Ok(0..=0);
    }
    let first = (-high / period).ceil();
    let last = ((period - low) / period).floor();
    if first < f64::from(i32::MIN) || last > f64::from(i32::MAX) {
        return Err("text transform exceeds the tile coordinate range".into());
    }
    Ok(first as i32..=last as i32)
}

struct Drawing {
    coverage: Vec<u8>,
    generator: StripGenerator,
    strips: StripStorage,
    width: u16,
    height: u16,
    tiled: [bool; 2],
    remaining: usize,
}

/// A solid span is opaque. An edge span stores one column of byte coverage at a time.
fn paint_strip(image: &mut [u8], stride: usize, clip: RectU16, span: StripFillSegment, alphas: Option<&[u8]>) {
    let rect = span.pixel_rect().intersect(clip);
    for y in rect.y0..rect.y1 {
        let offset = usize::from(y) * stride;
        let Some(row) = image.get_mut(offset + usize::from(rect.x0)..offset + usize::from(rect.x1)) else { continue };
        if let Some(alphas) = alphas {
            let first = usize::from(rect.x0 - span.x0()) * usize::from(Tile::HEIGHT) + usize::from(y - span.y());
            for (dst, &src) in row.iter_mut().zip(alphas.iter().skip(first).step_by(usize::from(Tile::HEIGHT))) {
                let alpha = u16::from(src);
                *dst = (alpha + (u16::from(*dst) * (255 - alpha) + 127) / 255) as u8;
            }
        } else {
            row.fill(255);
        }
    }
}

impl Drawing {
    fn composite_glyph(&mut self, path: &BezPath, transform: Affine, window: Rect) {
        let clip = RectU16::new(window.x0 as u16, window.y0 as u16, window.x1 as u16, window.y1 as u16);
        if clip.is_empty() {
            return;
        }
        // Generate coverage in canvas coordinates. Moving an edge into a temporary
        // bitmap can change the last coverage bit at a fractional transform.
        self.generator.generate_filled_path(path.iter(), Fill::NonZero, transform, None, &mut self.strips, None);
        let tiles = RectU16::new(clip.x0 / Tile::WIDTH, clip.y0 / Tile::HEIGHT, clip.x1.div_ceil(Tile::WIDTH), clip.y1.div_ceil(Tile::HEIGHT));
        let stride = usize::from(self.width);
        let alphas = &self.strips.alphas;
        visit_strip_fill_segments(
            &self.strips.strips,
            tiles,
            &mut self.coverage,
            |image, span| {
                let bytes = alphas.get(span.alpha_idx as usize..).unwrap_or_default();
                paint_strip(image, stride, clip, span.fill, Some(bytes));
            },
            |image, span| paint_strip(image, stride, clip, span, None),
        );
    }

    fn glyph(&mut self, ink: &Glyph, transform: Affine, text_bounds: Rect) -> Result<(), String> {
        if ink.path.elements().is_empty() {
            return Ok(());
        }
        let (width, height) = (f64::from(self.width), f64::from(self.height));
        let bounds = transform.transform_rect_bbox(ink.bounds);
        if !bounds.is_finite() {
            return Err("non-finite text outline transform".into());
        }
        let bounds = bounds.intersect(text_bounds);
        if bounds.is_zero_area() {
            return Ok(());
        }
        let elements = ink.path.elements().len();
        let columns = copies(bounds.x0, bounds.x1, width, self.tiled[0])?;
        let rows = copies(bounds.y0, bounds.y1, height, self.tiled[1])?;
        if (!self.tiled[0] && (bounds.x1 <= 0.0 || bounds.x0 >= width)) || (!self.tiled[1] && (bounds.y1 <= 0.0 || bounds.y0 >= height)) {
            return Ok(());
        }
        let canvas = Rect::new(0.0, 0.0, width, height);
        for row in rows {
            for column in columns.clone() {
                self.remaining = self.remaining.checked_sub(elements).ok_or("text outlines exceed the drawing budget")?;
                let offset = Vec2::new(f64::from(column) * width, f64::from(row) * height);
                let window = (bounds + offset).expand().intersect(canvas);
                self.composite_glyph(&ink.path, Affine::translate(offset) * transform, window);
            }
        }
        Ok(())
    }
}

/// Draw a Text node with the bytes of its resolved package font.
pub fn text_with_font(context: &CookContext<'_>, bytes: &[u8]) -> CookResult {
    let face = FontOutline::from_bytes(bytes, 0, &[]).map_err(|e| e.to_string())?;
    if face.metrics().units_per_em == 0 {
        return Err("font has no design units per em".into());
    }
    let font = harfrust::FontRef::from_index(bytes, 0).map_err(|e| e.to_string())?;
    let contours = font.glyf().ok().zip(font.loca(None).ok());
    let header = font.hhea().map_err(|e| e.to_string())?;
    let ascent = f64::from(header.ascender().to_i16());
    let descent = f64::from(header.descender().to_i16());
    let height_units = ascent - descent;
    if height_units <= 0.0 {
        return Err("font has no positive horizontal line height".into());
    }
    let pitch = height_units + f64::from(header.line_gap().to_i16());
    let data = harfrust::ShaperData::new(&font);
    let shaper = data.shaper(&font).build();
    let plans = ShapePlanCache::default();
    let (w, h) = craft_texture::clamp_size(context.w, context.h);
    let width = u16::try_from(w).map_err(|_| "text width exceeds the raster size")?;
    let height = u16::try_from(h).map_err(|_| "text height exceeds the raster size")?;
    let mut drawing = Drawing {
        coverage: vec![0; usize::from(width) * usize::from(height)],
        generator: StripGenerator::new(width, height, Level::try_detect().unwrap_or(Level::baseline())),
        strips: StripStorage::default(),
        width,
        height,
        tiled: context.tiling_axes(),
        remaining: MAX_ELEMENTS,
    };

    let [a, b, c, d] = context.v4("matrix").map(f64::from);
    let inverse = if a * d == b * c { Affine::IDENTITY } else { Affine::new([a, b, c, d, 0.0, 0.0]).inverse() };
    let [x, y] = context.v2("position").map(f64::from);
    let scale = f64::from(context.f("fontsize").max(0.0)) / height_units;
    let frame = Affine::scale_non_uniform(f64::from(width), f64::from(height))
        * Affine::translate((0.5 + x, 0.5 + y))
        * inverse
        * Affine::scale_non_uniform(scale, -scale);
    if !frame.as_coeffs().into_iter().all(f64::is_finite) {
        return Err("non-finite text transform".into());
    }
    let [xx, yx, xy, yy, _, _] = frame.as_coeffs();
    let pixels_per_unit = xx.hypot(xy).min(yx.hypot(yy));
    let text = context.text("text").chars().take(super::text::MAX_CHARS).collect::<String>().replace("\\n", "\n");
    let mut cache = BTreeMap::new();
    let mut outline_budget = MAX_ELEMENTS;
    let mut positioned = Vec::new();
    let mut text_bounds: Option<Rect> = None;
    for (line_index, line) in text.split('\n').enumerate() {
        let glyphs = shape_line(line, &shaper, &plans)?;
        let mut cursor = 0.0;
        let mut positions = Vec::with_capacity(glyphs.len());
        let baseline = -(line_index as f64) * pitch;
        let mut left = 0.0;
        let mut right = 0.0;
        for glyph in glyphs {
            if let std::collections::btree_map::Entry::Vacant(entry) = cache.entry(glyph.glyph_id) {
                let path = face.outline(glyph.glyph_id).map_err(|e| e.to_string())?;
                let simple_contour =
                    contours.as_ref().is_some_and(|(glyf, loca)| matches!(loca.get_glyf(GlyphId::new(glyph.glyph_id), glyf), Ok(Some(StoredGlyph::Simple(_)))));
                entry.insert(Glyph::new(path, simple_contour, pixels_per_unit, &mut outline_budget)?);
            }
            let ink = cache.get(&glyph.glyph_id).ok_or("missing glyph outline")?;
            let x = cursor + f64::from(glyph.x_offset);
            if positions.is_empty() {
                left = x + ink.bounds.x0;
            }
            right = x + ink.bounds.x1;
            positions.push((glyph.glyph_id, x, baseline + f64::from(glyph.y_offset)));
            cursor += f64::from(glyph.x_advance);
        }
        let shift = match context.i("alignment") {
            0 => 0.0,
            2 => left - right,
            _ => (left - right) * 0.5,
        };
        let line_bounds = Rect::new(left + shift, baseline + descent, right + shift, baseline + ascent).abs();
        text_bounds = Some(text_bounds.map_or(line_bounds, |bounds| bounds.union(line_bounds)));
        positioned.extend(positions.into_iter().map(|(id, x, y)| (id, x + shift, y)));
    }
    // The transformed text block occupies whole pixels. Ink may extend into another
    // line within the block, including when a tiled copy crosses the image edge.
    let text_bounds = frame.transform_rect_bbox(text_bounds.unwrap_or_default()).expand();
    if !text_bounds.is_finite() {
        return Err("non-finite text bounds".into());
    }
    for (id, x, y) in positioned {
        if let Some(ink) = cache.get(&id) {
            drawing.glyph(ink, frame * Affine::translate((x, y)), text_bounds)?;
        }
    }
    Ok(vec![composite_text(context, |x, y| {
        let index = y as usize * usize::from(width) + x as usize;
        drawing.coverage.get(index).map_or(0.0, |&alpha| f32::from(alpha) / 255.0)
    })])
}
