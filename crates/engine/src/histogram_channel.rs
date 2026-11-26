use serde::{Deserialize, Serialize};

/// The values the 2D view's histogram counts
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistogramChannel {
    #[default]
    Luminance,
    Red,
    Green,
    Blue,
    /// Red, green and blue drawn over each other
    Rgb,
    Alpha,
}

impl HistogramChannel {
    pub const ALL: [HistogramChannel; 6] =
        [HistogramChannel::Luminance, HistogramChannel::Red, HistogramChannel::Green, HistogramChannel::Blue, HistogramChannel::Rgb, HistogramChannel::Alpha];

    pub fn id(self) -> &'static str {
        match self {
            HistogramChannel::Luminance => "luminance",
            HistogramChannel::Red => "red",
            HistogramChannel::Green => "green",
            HistogramChannel::Blue => "blue",
            HistogramChannel::Rgb => "rgb",
            HistogramChannel::Alpha => "alpha",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            HistogramChannel::Luminance => "Luminance",
            HistogramChannel::Red => "Red",
            HistogramChannel::Green => "Green",
            HistogramChannel::Blue => "Blue",
            HistogramChannel::Rgb => "RGB",
            HistogramChannel::Alpha => "Alpha",
        }
    }

    pub fn from_id(id: &str) -> Option<HistogramChannel> {
        Self::ALL.into_iter().find(|channel| channel.id() == id)
    }
}
