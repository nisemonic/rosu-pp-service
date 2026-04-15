use std::num::NonZeroUsize;
use std::sync::{Arc, LazyLock};

use lru::LruCache;
use parking_lot::Mutex;
use rosu_pp::Beatmap;
use tracing::{debug, trace};

const DEFAULT_CACHE_SIZE: usize = 1000;

static CACHE: LazyLock<Mutex<BeatmapCache>> = LazyLock::new(|| {
    let size = std::env::var("PP_SERVICE_CACHE_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CACHE_SIZE);
    Mutex::new(BeatmapCache::new(size))
});

pub struct BeatmapCache {
    inner: LruCache<u32, Arc<Beatmap>>,
    hits: u64,
    misses: u64,
}

impl BeatmapCache {
    fn new(size: usize) -> Self {
        Self {
            inner: LruCache::new(NonZeroUsize::new(size).unwrap_or(NonZeroUsize::MIN)),
            hits: 0,
            misses: 0,
        }
    }
}

/// Returns a cached beatmap. Uses Arc for cheap cloning.
/// Caller should use Arc::unwrap_or_clone() if mutation is needed.
pub fn get(beatmap_id: u32) -> Option<Arc<Beatmap>> {
    let mut cache = CACHE.lock();
    let result = cache.inner.get(&beatmap_id).cloned();
    if result.is_some() {
        cache.hits += 1;
        trace!(beatmap_id, "cache hit");
    } else {
        cache.misses += 1;
        trace!(beatmap_id, "cache miss");
    }
    result
}

pub fn insert(beatmap_id: u32, beatmap: Beatmap) {
    let mut cache = CACHE.lock();
    let was_full = cache.inner.len() >= cache.inner.cap().get();
    cache.inner.put(beatmap_id, Arc::new(beatmap));
    debug!(beatmap_id, evicted = was_full, size = cache.inner.len(), "cache insert");
}

pub fn stats() -> CacheStats {
    let cache = CACHE.lock();
    CacheStats {
        size: cache.inner.len(),
        capacity: cache.inner.cap().get(),
        hits: cache.hits,
        misses: cache.misses,
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    pub size: usize,
    pub capacity: usize,
    pub hits: u64,
    pub misses: u64,
}

pub fn parse_beatmap_id(content: &[u8]) -> Option<u32> {
    let content = std::str::from_utf8(content).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if let Some(id) = line.strip_prefix("BeatmapID:") {
            return id.trim().parse().ok();
        }
        if line.starts_with("[HitObjects]") {
            break;
        }
    }
    None
}
