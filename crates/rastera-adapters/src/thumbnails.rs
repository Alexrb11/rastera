//! Disposable, bounded disk cache. Cache failures never prevent viewing originals.
use crate::{
    LocalImages,
    catalog::path_key,
    catalog_path,
    images::{io_error, stamp},
};
use rastera_core::{
    CancellationToken, DecodedImage, ErrorKind, FileStamp, FolderEntry, ImageBackend, ImageInfo,
    RasteraError, Result,
};
use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const MAGIC: &[u8; 8] = b"RASTH002";
const MAX_RECORD: u64 = 256 * 256 * 4 + 65_536 + 40;
const DISK_BUDGET: u64 = 2 * 1024 * 1024 * 1024;
const SWEEP_WRITTEN_BYTES: u64 = 16 * 1024 * 1024;
const RECONCILE_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Maintenance {
    last: Option<Instant>,
    written: u64,
}

pub struct CachedThumbnails {
    hits: AtomicU64,
    misses: AtomicU64,
    root: Option<PathBuf>,
    source: Arc<dyn ImageBackend>,
    budget: u64,
    maintenance: Mutex<Maintenance>,
    maintenance_runs: AtomicU64,
    maintenance_ns: AtomicU64,
    maintenance_files: AtomicU64,
    evicted_files: AtomicU64,
    invalidations: Mutex<HashMap<PathBuf, (u64, bool)>>,
    revision: AtomicU64,
}
#[derive(Clone, Copy, Debug)]
pub struct ThumbnailCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub maintenance_runs: u64,
    pub maintenance_elapsed: Duration,
    pub maintenance_files: u64,
    pub evicted_files: u64,
}
impl Default for CachedThumbnails {
    fn default() -> Self {
        Self::new(
            catalog_path()
                .ok()
                .and_then(|path| path.parent().map(|p| p.join("thumbnails-v1"))),
        )
    }
}
impl CachedThumbnails {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            root,
            source: Arc::new(LocalImages),
            budget: DISK_BUDGET,
            maintenance: Mutex::new(Maintenance::default()),
            maintenance_runs: AtomicU64::new(0),
            maintenance_ns: AtomicU64::new(0),
            maintenance_files: AtomicU64::new(0),
            evicted_files: AtomicU64::new(0),
            invalidations: Mutex::new(HashMap::new()),
            revision: AtomicU64::new(0),
        }
    }
    pub fn stats(&self) -> ThumbnailCacheStats {
        ThumbnailCacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            maintenance_runs: self.maintenance_runs.load(Ordering::Relaxed),
            maintenance_elapsed: Duration::from_nanos(self.maintenance_ns.load(Ordering::Relaxed)),
            maintenance_files: self.maintenance_files.load(Ordering::Relaxed),
            evicted_files: self.evicted_files.load(Ordering::Relaxed),
        }
    }
    /// Called on the folder worker. It never touches originals or scans the cache.
    pub fn invalidate_changed(
        &self,
        entries: &[FolderEntry],
        changed: &HashSet<PathBuf>,
        cancel: &CancellationToken,
    ) {
        let paths: HashSet<_> = entries.iter().map(|entry| &entry.path).collect();
        let mut invalidations = self.invalidations.lock().unwrap_or_else(|e| e.into_inner());
        invalidations.retain(|path, _| paths.contains(path));
        for entry in entries.iter().filter(|entry| changed.contains(&entry.path)) {
            if cancel.check().is_err() {
                break;
            }
            let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
            invalidations.insert(entry.path.clone(), (revision, true));
            if let (Some(root), Some(stamp)) = (&self.root, &entry.stamp) {
                let key = Self::key(&entry.path, stamp, 256);
                let _ = fs::remove_file(root.join(format!("{:016x}.thumb", Self::hash(&key))));
            }
        }
    }
    fn key(path: &Path, stamp: &FileStamp, edge: u32) -> Vec<u8> {
        let mut key = path_key(path);
        key.extend(format!("|{stamp:?}|{edge}|pipeline-3-source-size").bytes());
        key
    }
    fn hash(bytes: &[u8]) -> u64 {
        // Only a filename/checksum, never an image identity: the full key is checked on reads.
        let mut hash = DefaultHasher::new();
        bytes.hash(&mut hash);
        hash.finish()
    }
    fn read(path: &Path, key: &[u8], info: &ImageInfo, edge: u32) -> Option<DecodedImage> {
        let length = fs::metadata(path).ok()?.len();
        if !(36..=MAX_RECORD).contains(&length) {
            return None;
        }
        let bytes = fs::read(path).ok()?;
        if bytes.get(..8)? != MAGIC {
            return None;
        }
        let word = |offset| {
            Some(u32::from_le_bytes(
                bytes.get(offset..offset + 4)?.try_into().ok()?,
            ))
        };
        let key_len = word(8)? as usize;
        let width = word(12)?;
        let height = word(16)?;
        let stored_hash = u64::from_le_bytes(bytes.get(20..28)?.try_into().ok()?);
        let source_size = (word(28)?, word(32)?);
        if source_size != (info.width, info.height) && source_size != (info.height, info.width) {
            return None;
        }
        if width == 0 || height == 0 || width.max(height) > edge {
            return None;
        }
        let pixels = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        let end = 36usize.checked_add(key_len)?;
        if bytes.get(36..end)? != key || bytes.len().checked_sub(end)? != pixels {
            return None;
        }
        let rgba = bytes.get(end..)?;
        if Self::hash(rgba) != stored_hash {
            return None;
        }
        Some(DecodedImage {
            source_size,
            width,
            height,
            rgba: rgba.to_vec(),
            info: info.clone(),
        })
    }
    fn write(&self, path: &Path, key: &[u8], image: &DecodedImage) -> std::io::Result<u64> {
        if key.len() > 65_536 {
            return Ok(0);
        }
        fs::create_dir_all(path.parent().unwrap())?;
        let mut temp = tempfile::Builder::new()
            .prefix(".thumbnail-")
            .tempfile_in(path.parent().unwrap())?;
        temp.write_all(MAGIC)?;
        temp.write_all(&(key.len() as u32).to_le_bytes())?;
        temp.write_all(&image.width.to_le_bytes())?;
        temp.write_all(&image.height.to_le_bytes())?;
        temp.write_all(&Self::hash(&image.rgba).to_le_bytes())?;
        temp.write_all(&image.source_size.0.to_le_bytes())?;
        temp.write_all(&image.source_size.1.to_le_bytes())?;
        temp.write_all(key)?;
        temp.write_all(&image.rgba)?;
        temp.flush()?;
        // Replacing a disposable cache entry is safe; originals are never written.
        temp.persist(path).map_err(|e| e.error)?;
        Ok(36 + key.len() as u64 + image.rgba.len() as u64)
    }
    fn maintain(&self, root: &Path, written: u64, cancel: &CancellationToken) -> Result<()> {
        let mut state = self.maintenance.lock().unwrap_or_else(|e| e.into_inner());
        state.written = state.written.saturating_add(written);
        cancel.check()?;
        if state
            .last
            .is_none_or(|last| last.elapsed() >= RECONCILE_INTERVAL)
            || state.written >= SWEEP_WRITTEN_BYTES.min(self.budget)
        {
            self.sweep(root, cancel)?;
            state.last = Some(Instant::now());
            state.written = 0;
        }
        Ok(())
    }
    fn sweep(&self, root: &Path, cancel: &CancellationToken) -> Result<()> {
        let started = Instant::now();
        self.maintenance_runs.fetch_add(1, Ordering::Relaxed);
        let result = self.sweep_files(root, cancel);
        self.maintenance_ns.fetch_add(
            started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
        result
    }
    fn sweep_files(&self, root: &Path, cancel: &CancellationToken) -> Result<()> {
        let Ok(entries) = fs::read_dir(root) else {
            return Ok(());
        };
        let mut files = Vec::new();
        let mut total = 0u64;
        for entry in entries.flatten() {
            cancel.check()?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
                continue;
            };
            let Some(stem) = name.strip_suffix(".thumb") else {
                continue;
            };
            if stem.len() != 16 || !stem.bytes().all(|b| b.is_ascii_hexdigit()) {
                continue;
            }
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.is_file() || meta.is_symlink() {
                continue;
            }
            total = total.saturating_add(meta.len());
            self.maintenance_files.fetch_add(1, Ordering::Relaxed);
            files.push((meta.modified().ok(), meta.len(), path));
        }
        if total <= self.budget {
            return Ok(());
        }
        cancel.check()?;
        files.sort_unstable_by_key(|a| a.0);
        for (_, size, path) in files {
            cancel.check()?;
            if total <= self.budget {
                break;
            }
            if fs::remove_file(path).is_ok() {
                self.evicted_files.fetch_add(1, Ordering::Relaxed);
                total = total.saturating_sub(size);
            }
        }
        Ok(())
    }
}
impl ImageBackend for CachedThumbnails {
    fn inspect(&self, path: &Path) -> Result<ImageInfo> {
        self.source.inspect(path)
    }
    fn decode(
        &self,
        path: &Path,
        info: &ImageInfo,
        edge: u32,
        cancel: &CancellationToken,
    ) -> Result<DecodedImage> {
        cancel.check()?;
        if edge == 0 || edge > 256 {
            return self.source.decode(path, info, edge, cancel);
        }
        if stamp(fs::metadata(path).map_err(io_error)?) != info.stamp {
            return Err(RasteraError::new(
                ErrorKind::Changed,
                "El original cambió durante la carga",
            ));
        }
        let key = Self::key(path, &info.stamp, edge);
        let invalidation = self
            .invalidations
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(path)
            .copied();
        let cache = self
            .root
            .as_ref()
            .map(|root| root.join(format!("{:016x}.thumb", Self::hash(&key))));
        if let Some(root) = &self.root {
            self.maintain(root, 0, cancel)?;
        }
        if !invalidation.is_some_and(|(_, dirty)| dirty)
            && let Some(image) = cache.as_ref().and_then(|p| Self::read(p, &key, info, edge))
        {
            cancel.check()?;
            if stamp(fs::metadata(path).map_err(io_error)?) == info.stamp {
                self.hits.fetch_add(1, Ordering::Relaxed);
                return Ok(image);
            }
            return Err(RasteraError::new(
                ErrorKind::Changed,
                "El original cambió durante la carga",
            ));
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        let image = self.source.decode(path, info, edge, cancel)?;
        cancel.check()?;
        let mut invalidations = self.invalidations.lock().unwrap_or_else(|e| e.into_inner());
        if invalidations.get(path).map(|(revision, _)| *revision)
            != invalidation.map(|(revision, _)| revision)
        {
            return Err(RasteraError::new(
                ErrorKind::Changed,
                "El original cambió durante la carga",
            ));
        }
        if let Some(cache) = cache
            && let Ok(written) = self.write(&cache, &key, &image)
        {
            if let Some((_, dirty)) = invalidations.get_mut(path) {
                *dirty = false;
            }
            drop(invalidations);
            self.maintain(cache.parent().unwrap(), written, cancel)?;
        }
        Ok(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    struct Counting(AtomicUsize);
    #[test]
    fn notification_invalidates_identical_stamp_and_survives_cache_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        let root = dir.path().join("cache");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]))
            .save(&path)
            .unwrap();
        let cache = CachedThumbnails::new(Some(root.clone()));
        let info = cache.inspect(&path).unwrap();
        let red = cache
            .decode(&path, &info, 256, &CancellationToken::default())
            .unwrap();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([0, 255, 0, 255]))
            .save(&path)
            .unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(info.stamp.modified.unwrap()))
            .unwrap();
        assert_eq!(cache.inspect(&path).unwrap().stamp, info.stamp);
        cache.invalidate_changed(
            &[FolderEntry {
                path: path.clone(),
                stamp: Some(info.stamp.clone()),
            }],
            &HashSet::from([path.clone()]),
            &CancellationToken::default(),
        );
        let green = cache
            .decode(&path, &info, 256, &CancellationToken::default())
            .unwrap();
        assert_ne!(green.rgba, red.rgba);
        assert_eq!(green.rgba[..4], [0, 255, 0, 255]);
        let restarted = CachedThumbnails::new(Some(root));
        assert_eq!(
            restarted
                .decode(&path, &info, 256, &CancellationToken::default())
                .unwrap()
                .rgba,
            green.rgba
        );
        assert_eq!(restarted.stats().hits, 1);
    }
    #[test]
    fn repeated_hits_skip_inventory_and_elapsed_time_reconciles_external_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        image::RgbaImage::new(20, 10).save(&path).unwrap();
        let root = dir.path().join("cache");
        let mut cache = CachedThumbnails::new(Some(root.clone()));
        cache.budget = 4096;
        let info = cache.inspect(&path).unwrap();
        for _ in 0..200 {
            cache
                .decode(&path, &info, 8, &CancellationToken::default())
                .unwrap();
        }
        assert_eq!(cache.stats().hits, 199);
        assert_eq!(cache.stats().maintenance_runs, 1);
        // An external writer can add entries; the time-based reconciliation still enforces quota.
        fs::write(root.join("0000000000000000.thumb"), vec![0; 8192]).unwrap();
        fs::write(root.join("keep.txt"), b"foreign").unwrap();
        cache.maintenance.lock().unwrap().last = Some(Instant::now() - RECONCILE_INTERVAL);
        cache
            .decode(&path, &info, 8, &CancellationToken::default())
            .unwrap();
        let owned_bytes: u64 = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "thumb"))
            .map(|entry| entry.metadata().unwrap().len())
            .sum();
        assert!(owned_bytes <= cache.budget);
        assert!(root.join("keep.txt").exists());
        assert_eq!(cache.stats().maintenance_runs, 2);
        assert!(cache.stats().evicted_files >= 1);
    }
    impl ImageBackend for Counting {
        fn inspect(&self, path: &Path) -> Result<ImageInfo> {
            LocalImages.inspect(path)
        }
        fn decode(
            &self,
            path: &Path,
            info: &ImageInfo,
            edge: u32,
            cancel: &CancellationToken,
        ) -> Result<DecodedImage> {
            self.0.fetch_add(1, Ordering::Relaxed);
            LocalImages.decode(path, info, edge, cancel)
        }
    }
    #[test]
    fn survives_restart_detects_corruption_and_invalidates_changed_original() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ñ.png");
        let root = dir.path().join("cache");
        image::RgbaImage::from_pixel(20, 10, image::Rgba([40, 50, 60, 128]))
            .save(&path)
            .unwrap();
        let source = Arc::new(Counting(AtomicUsize::new(0)));
        for _ in 0..2 {
            let mut cache = CachedThumbnails::new(Some(root.clone()));
            cache.source = source.clone();
            let info = cache.inspect(&path).unwrap();
            assert_eq!(
                cache
                    .decode(&path, &info, 8, &CancellationToken::default())
                    .unwrap()
                    .rgba[3],
                128
            );
        }
        assert_eq!(source.0.load(Ordering::Relaxed), 1);
        let cache = CachedThumbnails::new(Some(root.clone()));
        let info = cache.inspect(&path).unwrap();
        let file = fs::read_dir(&root).unwrap().next().unwrap().unwrap().path();
        let mut corrupt = fs::read(&file).unwrap();
        *corrupt.last_mut().unwrap() ^= 1;
        fs::write(&file, corrupt).unwrap();
        assert_eq!(
            cache
                .decode(&path, &info, 8, &CancellationToken::default())
                .unwrap()
                .rgba[3],
            128
        );
        image::RgbaImage::new(30, 15).save(&path).unwrap();
        assert_eq!(
            cache
                .decode(&path, &info, 8, &CancellationToken::default())
                .unwrap_err()
                .kind,
            ErrorKind::Changed
        );
        let info = cache.inspect(&path).unwrap();
        assert_eq!(
            cache
                .decode(&path, &info, 8, &CancellationToken::default())
                .unwrap()
                .rgba[3],
            0
        );
    }
    #[test]
    fn enforces_quota_leaves_foreign_files_and_falls_back_when_unwritable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        image::RgbaImage::new(20, 10).save(&path).unwrap();
        let root = dir.path().join("cache");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("keep.txt"), "foreign").unwrap();
        let mut cache = CachedThumbnails::new(Some(root.clone()));
        cache.budget = 1;
        let info = cache.inspect(&path).unwrap();
        cache
            .decode(&path, &info, 8, &CancellationToken::default())
            .unwrap();
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        let blocked = dir.path().join("file");
        fs::write(&blocked, "not a directory").unwrap();
        let cache = CachedThumbnails::new(Some(blocked));
        assert!(
            cache
                .decode(&path, &info, 8, &CancellationToken::default())
                .is_ok()
        );
        let cancel = CancellationToken::default();
        cancel.cancel();
        assert_eq!(
            cache.decode(&path, &info, 8, &cancel).unwrap_err().kind,
            ErrorKind::Cancelled
        );
    }
}
