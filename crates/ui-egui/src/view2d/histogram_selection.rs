/// What lies inside a selected range of a histogram
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HistogramSelection {
    pub(super) pixels: u64,
    /// The tallest column of the range
    pub(super) largest: u32,
}
