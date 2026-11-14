use materialcraft_core::NodeId;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::commands::{bad, boolp, strp, u64p};
use crate::{HistogramChannel, Result};

const COMMAND: &str = "view.2d";

/// What the 2D view shows and how it shows it
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct View2d {
    pub node: Option<NodeId>,
    pub port: usize,
    /// Repeat the image across the view to check tiling
    pub tiled: bool,
    /// `rgba`, `r`, `g`, `b`, `a`
    pub channel: String,
    /// Show the histogram panel
    pub histogram: bool,
    /// Show the information panel: the pixel under the pointer and the color sampler
    pub information: bool,
    /// Draw a checkerboard behind transparent pixels instead of black
    pub checkerboard: bool,
    /// The colors are already multiplied by their alpha
    pub premultiplied: bool,
    /// The values are sRGB encoded; off shows them as linear light, which looks brighter
    pub srgb: bool,
    /// Stretch the image to the proportions of the graph's physical size
    pub physical_size: bool,
    /// Keep the zoom when the image size changes; off keeps the image's size on screen
    pub maintain_zoom: bool,
    /// Draw the UV layout of the 3D view's mesh over the image
    pub uv: bool,
    /// Where the color sampler sits, as fractions of the image from its top-left corner
    pub sampler: Option<[f32; 2]>,
    pub histogram_channel: HistogramChannel,
    /// 8, 16 or 32 bits; 0 follows the image's format
    pub histogram_bits: u32,
    /// Plot the square root of the counts, so small counts stay visible
    pub histogram_sqrt: bool,
    /// The selected part of the histogram, as fractions of its range
    pub histogram_selection: Option<[f32; 2]>,
    /// An image file drawn behind transparent pixels
    pub background: Option<String>,
}

impl Default for View2d {
    fn default() -> Self {
        View2d {
            node: None,
            port: 0,
            tiled: false,
            channel: "rgba".into(),
            histogram: false,
            information: false,
            checkerboard: false,
            premultiplied: false,
            srgb: true,
            physical_size: false,
            maintain_zoom: true,
            uv: false,
            sampler: None,
            histogram_channel: HistogramChannel::default(),
            histogram_bits: 0,
            histogram_sqrt: false,
            histogram_selection: None,
            background: None,
        }
    }
}

impl View2d {
    pub const CHANNELS: [&'static str; 5] = ["rgba", "r", "g", "b", "a"];
    pub const HISTOGRAM_BITS: [u32; 4] = [0, 8, 16, 32];

    /// These settings with the `view.2d` parameters applied; a rejected parameter changes nothing.
    /// `node` is the validated `node` parameter
    pub(crate) fn updated(&self, node: Option<NodeId>, params: &Json) -> Result<View2d> {
        let mut view = self.clone();
        if let Some(node) = node {
            view.node = Some(node);
            view.port = 0;
        }
        if let Some(port) = u64p(params, "port") {
            view.port = usize::try_from(port).unwrap_or(usize::MAX);
        }

        if let Some(channel) = strp(params, "channel") {
            if !Self::CHANNELS.contains(&channel) {
                return Err(bad(COMMAND, "`channel` is rgba, r, g, b or a"));
            }
            view.channel = channel.into();
        }
        if let Some(channel) = strp(params, "histogramChannel") {
            view.histogram_channel =
                HistogramChannel::from_id(channel).ok_or_else(|| bad(COMMAND, "`histogramChannel` is luminance, red, green, blue, rgb or alpha"))?;
        }
        if let Some(bits) = u64p(params, "histogramBits") {
            view.histogram_bits = Self::HISTOGRAM_BITS
                .into_iter()
                .find(|allowed| u64::from(*allowed) == bits)
                .ok_or_else(|| bad(COMMAND, "`histogramBits` is 8, 16 or 32, or 0 to follow the image"))?;
        }

        for (key, setting) in [
            ("tiled", &mut view.tiled),
            ("histogram", &mut view.histogram),
            ("information", &mut view.information),
            ("checkerboard", &mut view.checkerboard),
            ("premultiplied", &mut view.premultiplied),
            ("srgb", &mut view.srgb),
            ("physicalSize", &mut view.physical_size),
            ("maintainZoom", &mut view.maintain_zoom),
            ("uv", &mut view.uv),
            ("histogramSqrt", &mut view.histogram_sqrt),
        ] {
            if let Some(value) = boolp(params, key) {
                *setting = value;
            }
        }

        if let Some(sampler) = params.get("sampler") {
            view.sampler = fractions(sampler, "sampler", "`sampler` is [x, y] in 0..1, or null to remove it")?;
        }
        if let Some(selection) = params.get("histogramSelection") {
            view.histogram_selection = fractions(selection, "histogramSelection", "`histogramSelection` is [low, high] in 0..1, or null to clear it")?
                .map(|[first, second]| [first.min(second), first.max(second)]);
        }
        if let Some(background) = params.get("background") {
            view.background = match background {
                Json::Null => None,
                Json::String(path) if !path.is_empty() => Some(path.clone()),
                _ => return Err(bad(COMMAND, "`background` is an image file path, or null to remove it")),
            };
        }

        Ok(view)
    }
}

/// Two finite numbers held within 0..1, or `None` for null
fn fractions(value: &Json, key: &str, expected: &str) -> Result<Option<[f32; 2]>> {
    if value.is_null() {
        return Ok(None);
    }

    let numbers: Option<Vec<f32>> =
        value.as_array().filter(|list| list.len() == 2).map(|list| list.iter().filter_map(Json::as_f64).map(|number| number as f32).collect());
    match numbers.as_deref() {
        Some(&[first, second]) if first.is_finite() && second.is_finite() => Ok(Some([first.clamp(0.0, 1.0), second.clamp(0.0, 1.0)])),
        _ => Err(bad(COMMAND, format!("{expected} (got `{key}`: {value})"))),
    }
}
