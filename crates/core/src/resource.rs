use crate::{OutputFormat, ResourceBytes};
use serde::{Deserialize, Serialize};

/// An asset retained in memory until the package is saved: a bitmap or an SVG.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    /// Unique name, referenced by nodes (`resource` parameter)
    pub name: String,
    #[serde(default = "default_folder")]
    pub folder: String,
    /// File name inside the package's `resources/` folder
    pub file: String,
    /// External origin retained for linked library saves and repair when bytes are unavailable
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// Original bytes, saved beside native SBS or inside a MaterialCraft archive.
    #[serde(skip)]
    pub bytes: ResourceBytes,
}

impl Resource {
    /// A resource named `name`, stored as `file` in the package
    pub fn new(name: impl Into<String>, file: impl Into<String>, bytes: ResourceBytes) -> Resource {
        Resource { name: name.into(), folder: default_folder(), file: file.into(), source: String::new(), bytes }
    }

    pub fn sbs_path(&self) -> String {
        if self.folder.is_empty() { self.name.clone() } else { format!("{}/{}", self.folder, self.name) }
    }
}

fn default_folder() -> String {
    "Resources".into()
}

/// Original normalized channel precision of a grayscale or truecolor PNG resource.
/// The decoded float values alone cannot distinguish a byte from the equivalent repeated-byte word.
pub fn png_sample_format(bytes: &[u8]) -> Option<OutputFormat> {
    png_raster_info(bytes).map(|info| info.format)
}

/// Raster properties declared in the header of a normalized grayscale or truecolor PNG.
pub struct PngRasterInfo {
    pub size: [u32; 2],
    pub format: OutputFormat,
    pub grayscale: bool,
}

pub fn png_raster_info(bytes: &[u8]) -> Option<PngRasterInfo> {
    let header = bytes.strip_prefix(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR")?;
    let format = match header.get(8..10)? {
        [8, 0 | 2 | 4 | 6] => OutputFormat::U8,
        [16, 0 | 2 | 4 | 6] => OutputFormat::U16,
        _ => return None,
    };
    let width = u32::from_be_bytes(*header.first_chunk()?);
    let height = u32::from_be_bytes(*header.get(4..)?.first_chunk()?);
    Some(PngRasterInfo { size: [width, height], format, grayscale: matches!(header.get(9), Some(0 | 4)) })
}
