//! The displayed pixels or mesh handed to one command without recooking or changing the view

use std::path::PathBuf;
use std::sync::Arc;

use materialcraft_geom::Mesh;

use crate::Result;
use crate::commands::bad;

/// One immutable view snapshot, shared with the view only until its command returns
pub enum DisplaySnapshot {
    Render { width: u32, height: u32, pixels: Arc<Vec<u8>>, path: Option<PathBuf> },
    Geometry { mesh: Arc<Mesh>, path: Option<PathBuf> },
    Image { width: u32, height: u32, pixels: Arc<Vec<u8>>, premultiplied: bool },
}

impl DisplaySnapshot {
    /// The UI's retained render buffer, captured before a save dialog opens
    pub fn render(width: u32, height: u32, pixels: Arc<Vec<u8>>) -> Self {
        Self::Render { width, height, pixels, path: None }
    }

    /// Keep the exact native destination selected by a file dialog
    pub fn render_to(width: u32, height: u32, pixels: Arc<Vec<u8>>, path: PathBuf) -> Self {
        Self::Render { width, height, pixels, path: Some(path) }
    }

    /// The mesh belonging to the displayed frame, including any relief
    pub fn geometry(mesh: Arc<Mesh>) -> Self {
        Self::Geometry { mesh, path: None }
    }

    /// Keep the exact native destination selected by a file dialog
    pub fn geometry_to(mesh: Arc<Mesh>, path: PathBuf) -> Self {
        Self::Geometry { mesh, path: Some(path) }
    }

    /// The RGBA bytes already prepared for the 2D view and clipboard
    pub fn image(width: u32, height: u32, pixels: Arc<Vec<u8>>, premultiplied: bool) -> Self {
        Self::Image { width, height, pixels, premultiplied }
    }

    pub fn into_image(self) -> Option<(u32, u32, Arc<Vec<u8>>, bool)> {
        match self {
            Self::Image { width, height, pixels, premultiplied } => Some((width, height, pixels, premultiplied)),
            _ => None,
        }
    }

    pub(crate) fn valid_rgba(command: &str, width: u32, height: u32, pixels: &[u8]) -> Result<()> {
        const MAX_IMAGE_SIDE: u32 = 8192;
        if width == 0 || height == 0 || width > MAX_IMAGE_SIDE || height > MAX_IMAGE_SIDE {
            return Err(bad(command, format!("displayed image dimensions must be 1..={MAX_IMAGE_SIDE}")));
        }

        let expected = usize::try_from(u64::from(width) * u64::from(height) * 4).map_err(|_| bad(command, "displayed image is too large"))?;
        if pixels.len() != expected {
            return Err(bad(command, format!("displayed RGBA has {} bytes; expected {expected}", pixels.len())));
        }
        Ok(())
    }

    pub(crate) fn valid_geometry(command: &str, mesh: &Mesh) -> Result<()> {
        const MAX_EXPORT_TRIANGLES: usize = 1_000_000;
        const MAX_EXPORT_VERTICES: usize = 3_000_000;
        if mesh.positions.is_empty()
            || mesh.indices.is_empty()
            || mesh.indices.len() > MAX_EXPORT_TRIANGLES * 3
            || mesh.positions.len() > MAX_EXPORT_VERTICES
            || mesh.uvs.len() > MAX_EXPORT_VERTICES
            || mesh.normals.len() > MAX_EXPORT_VERTICES
            || !mesh.indices.len().is_multiple_of(3)
            || mesh.indices.iter().any(|index| *index as usize >= mesh.positions.len())
            || mesh.positions.iter().any(|position| !position.is_finite())
            || mesh.uvs.iter().any(|uv| !uv.is_finite())
            || mesh.normals.iter().any(|normal| !normal.is_finite())
        {
            return Err(bad(command, "displayed mesh is empty, invalid or exceeds the export size limit"));
        }
        Ok(())
    }
}
