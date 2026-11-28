#[derive(Clone)]
pub(super) struct PatternStamp {
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub rotation: f32,
    pub literal_geometry: bool,
    pub color: [f32; 4],
    pub depth: u32,
    pub pattern: i32,
    pub variation: f32,
    pub blend: i32,
    pub filtering: i32,
    pub premultiplied: bool,
}

/// A position between tile copies in traversal order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct DrawPosition {
    pub stamp: usize,
    pub copy: u32,
}

/// Boundaries in the complete tile stream, including draws discarded before rasterization.
#[derive(Default)]
pub(super) struct CopyPositions {
    pub total: u32,
    retained: Vec<std::ops::Range<u32>>,
}

impl CopyPositions {
    pub fn record(&mut self, count: u32, retained: bool) -> Option<std::ops::Range<u32>> {
        let start = self.total;
        self.total = start.checked_add(count)?;
        let range = start..self.total;
        if retained {
            self.retained.push(range.clone());
        }
        Some(range)
    }

    pub fn at(&self, cut: u32) -> DrawPosition {
        let stamp = self.retained.partition_point(|range| range.end <= cut);
        // Across discarded draws the next retained stamp starts at its first copy.
        let copy = self.retained.get(stamp).map_or(0, |range| cut.saturating_sub(range.start));
        DrawPosition { stamp, copy }
    }
}

#[derive(Clone, Copy)]
pub(super) struct DrawRun<'a> {
    stamps: &'a [PatternStamp],
    first_copy: u32,
    end_copy: u32,
}

impl<'a> DrawRun<'a> {
    pub fn whole(stamps: &'a [PatternStamp]) -> Self {
        Self { stamps, first_copy: 0, end_copy: 0 }
    }

    pub fn between(stamps: &'a [PatternStamp], start: DrawPosition, end: DrawPosition) -> Option<Self> {
        if start > end {
            return None;
        }
        if start == end {
            return Some(Self::whole(&[]));
        }
        let stop = end.stamp.checked_add(usize::from(end.copy != 0))?;
        Some(Self { stamps: stamps.get(start.stamp..stop)?, first_copy: start.copy, end_copy: end.copy })
    }

    pub fn is_empty(&self) -> bool {
        self.stamps.is_empty()
    }

    pub fn draws(&self) -> impl Iterator<Item = (&'a PatternStamp, std::ops::Range<u64>)> + Clone + '_ {
        self.stamps.iter().enumerate().map(|(i, stamp)| {
            let first = if i == 0 { u64::from(self.first_copy) } else { 0 };
            let end = if i + 1 == self.stamps.len() && self.end_copy != 0 { u64::from(self.end_copy) } else { u64::MAX };
            (stamp, first..end)
        })
    }
}
