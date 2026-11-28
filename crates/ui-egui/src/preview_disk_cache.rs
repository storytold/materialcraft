//! Disposable PNG thumbnails. All filesystem work belongs to the existing preview workers.

use std::fs::{self, File};
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use image::ImageDecoder;
use sha2::{Digest, Sha256};

use super::preview_cache_settings::{PreviewCacheLimits, PreviewCacheSettings, PreviewCacheState};

const MAX_SCAN_ENTRIES: usize = materialcraft_engine::LibraryCachePrefs::MAX_PREVIEWS as usize * 2;
const MAX_FILE_BYTES: u64 = 128 * 1024;
const MAX_EXECUTABLE_BYTES: u64 = 1 << 30;
const MAX_DECODE_BYTES: u64 = 1024 * 1024;
const FILE_TOUCH_INTERVAL_SECS: u64 = 3600;
const MAX_SIDE: u32 = 128;

pub(super) struct PreviewDiskCache {
    root: PathBuf,
    renderer: [u8; 32],
    settings: PreviewCacheSettings,
    epoch: u64,
}

impl PreviewDiskCache {
    pub fn configured(settings: PreviewCacheSettings) -> Option<Self> {
        // Only an explicit override may enable persistence in scripted runs
        let root = match std::env::var_os("MATERIALCRAFT_CACHE_DIR") {
            Some(path) if path.is_empty() => return None,
            Some(path) => PathBuf::from(path),
            None if cfg!(test) || std::env::var_os("MATERIALCRAFT_NO_PREFS").is_some() => return None,
            // A Nightly profile owns its previews, never the stable per-user cache.
            None if crate::data_root::root().is_some() => crate::data_root::sub("cache")?,
            // The shared resolver honors redirected Windows profiles and absolute XDG paths
            None => craft_update::Paths::per_user("materialcraft").ok()?.cache,
        };
        Self::open(root.join("previews"), settings)
    }

    /// Called on a worker. The executable digest also invalidates development builds that keep
    /// the same version number. It is streamed once per process, never on the event-loop thread.
    pub fn open(root: PathBuf, settings: PreviewCacheSettings) -> Option<Self> {
        static RENDERER: OnceLock<Option<[u8; 32]>> = OnceLock::new();
        let renderer = *RENDERER.get_or_init(|| {
            let mut file = File::open(std::env::current_exe().ok()?).ok()?.take(MAX_EXECUTABLE_BYTES + 1);
            let mut hash = Sha256::new();
            let bytes = io::copy(&mut file, &mut hash).ok()?;
            (bytes <= MAX_EXECUTABLE_BYTES).then(|| hash.finalize().into())
        });
        let epoch = settings.get().epoch;
        Some(Self { root, renderer: renderer?, settings, epoch })
    }

    pub fn get_or_render(&self, key: [u8; 32], side: u32, cancel: &AtomicBool, render: impl FnOnce() -> Option<Vec<u8>>) -> Option<Vec<u8>> {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }

        let path = self.path(key, side);
        if let Some(pixels) = self.read(&path, side) {
            return (!cancel.load(Ordering::Relaxed)).then_some(pixels);
        }

        let pixels = render()?;
        if cancel.load(Ordering::Relaxed) {
            return None;
        }

        // Optional storage failure must not hide a rendered tile
        if let Err(error) = self.write(&path, side, &pixels) {
            log::debug!("Library preview persistence skipped: {error}");
        }

        Some(pixels)
    }

    fn path(&self, key: [u8; 32], side: u32) -> PathBuf {
        let mut hash = Sha256::new();
        hash.update(b"materialcraft-preview-png-v1");
        hash.update(self.renderer);
        hash.update(key);
        hash.update(side.to_le_bytes());
        self.root.join(format!("{:x}.png", hash.finalize()))
    }

    fn read(&self, path: &Path, side: u32) -> Option<Vec<u8>> {
        if side == 0 || side > MAX_SIDE || self.limits().is_none_or(|limits| limits.budget_bytes == 0) {
            return None;
        }

        let metadata = fs::symlink_metadata(path).ok()?;
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            return None;
        }

        let mut file = File::open(path).ok()?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file).take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return None;
        }

        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_SIDE);
        limits.max_image_height = Some(MAX_SIDE);
        limits.max_alloc = Some(MAX_DECODE_BYTES);
        let decoder = image::codecs::png::PngDecoder::with_limits(Cursor::new(bytes), limits).ok()?;
        if decoder.dimensions() != (side, side) || decoder.color_type() != image::ColorType::Rgba8 {
            return None;
        }

        let mut pixels = vec![0; (side * side * 4) as usize];
        decoder.read_image(&mut pixels).ok()?;

        // Refresh file age at most once per hour to approximate LRU across launches
        if metadata.modified().ok()?.elapsed().is_ok_and(|age| age >= Duration::from_secs(FILE_TOUCH_INTERVAL_SECS)) {
            let _ = file.set_modified(SystemTime::now());
        }

        Some(pixels)
    }

    fn write(&self, path: &Path, side: u32, pixels: &[u8]) -> io::Result<()> {
        if side == 0 || side > MAX_SIDE || pixels.len() != (side * side * 4) as usize || self.limits().is_none_or(|limits| limits.budget_bytes == 0) {
            return Ok(());
        }

        let mut bytes = Vec::new();
        craft_png::write_8bit(&mut bytes, side, side, craft_png::Layout::Rgba, pixels).map_err(io::Error::other)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Ok(());
        }

        fs::create_dir_all(&self.root)?;

        // A busy writer is a cache miss; preview workers must not wait on another process
        let lock = File::options().read(true).write(true).create(true).truncate(false).open(self.root.join("cache.lock"))?;
        lock.try_lock().map_err(io::Error::other)?;

        // Refuse symlinks because atomic writes otherwise follow them into unrelated files
        if fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_file()) {
            return Err(io::Error::other("preview cache entry is not a regular file"));
        }

        let Some(limits) = self.limits() else { return Ok(()) };

        if bytes.len() as u64 > limits.budget_bytes {
            return Ok(());
        }

        self.reserve(Some(path), bytes.len() as u64, limits)?;
        craft_atomic::WriteOptions::default().with_new_file_permissions(craft_atomic::NewFilePermissions::Private).atomic_write(path, &bytes)
    }

    pub fn enabled(&self) -> bool {
        self.limits().is_some_and(|limits| limits.budget_bytes > 0)
    }

    fn limits(&self) -> Option<PreviewCacheLimits> {
        let state = self.settings.get();
        (!state.disabled && !state.clear_pending && state.epoch == self.epoch).then_some(state.limits)
    }

    /// Returns false only while another process is writing; the coordinator retries later.
    pub fn maintain(&self, state: PreviewCacheState) -> io::Result<bool> {
        if !self.root.try_exists()? {
            return Ok(true);
        }

        let lock = File::options().read(true).write(true).create(true).truncate(false).open(self.root.join("cache.lock"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Ok(false),
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
        }

        let limits = if state.clear_pending || state.limits.budget_bytes == 0 { PreviewCacheLimits { budget_bytes: 0, max_entries: 0 } } else { state.limits };
        self.reserve(None, 0, limits)?;
        Ok(true)
    }

    fn reserve(&self, target: Option<&Path>, incoming: u64, limits: PreviewCacheLimits) -> io::Result<()> {
        let mut entries = Vec::new();
        let mut total = 0u64;

        // Bound scans and metadata allocations for externally populated directories
        for (index, entry) in fs::read_dir(&self.root)?.enumerate() {
            if index >= MAX_SCAN_ENTRIES {
                return Err(io::Error::other("preview cache directory exceeds its entry limit"));
            }

            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }

            let name = entry.file_name();
            let name = name.to_string_lossy();

            // The lock excludes live writers, so matching staging files are abandoned
            if staging_name(&name) {
                fs::remove_file(entry.path())?;
                continue;
            }

            if !cache_name(&name) || target.is_some_and(|target| entry.path() == target) {
                continue;
            }

            let metadata = entry.metadata()?;
            total = total.saturating_add(metadata.len());
            entries.push((metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH), entry.path(), metadata.len()));
        }

        entries.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut count = entries.len();
        for (_, path, size) in entries {
            if total.saturating_add(incoming) <= limits.budget_bytes && count.saturating_add(usize::from(target.is_some())) <= limits.max_entries {
                break;
            }

            fs::remove_file(path)?;
            total = total.saturating_sub(size);
            count = count.saturating_sub(1);
        }

        Ok(())
    }
}

fn cache_name(name: &str) -> bool {
    name.strip_suffix(".png").is_some_and(digest_name)
}

fn digest_name(stem: &str) -> bool {
    stem.len() == 64 && stem.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// craft-atomic's staging file of a cache file: `.{file}.{pid}-{n}.craft_tmp`, where `file` is the cache file's name or
/// its first part (older craft-atomic cut it to 64 characters).
fn staging_name(name: &str) -> bool {
    let Some((file, suffix)) = name.strip_prefix('.').and_then(|name| name.strip_suffix(".craft_tmp")).and_then(|name| name.rsplit_once('.')) else {
        return false;
    };
    let stem = file.split_once('.').map_or(file, |(stem, _)| stem);

    digest_name(stem) && suffix.split_once('-').is_some_and(|(pid, counter)| pid.parse::<u32>().is_ok_and(|pid| pid > 0) && counter.parse::<u64>().is_ok())
}

#[cfg(test)]
#[path = "preview_disk_cache.test.rs"]
mod tests;
