//! Caching layer for Figma API responses
//!
//! Provides in-memory caching with TTL using moka, plus optional
//! disk-based persistence for reuse between CLI invocations.

use moka::sync::Cache;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Cache key types for different API resources
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum CacheKey {
    /// Full file metadata by file key
    File(String),
    /// Light file metadata by file key
    FileMeta(String),
    /// Specific nodes by file_key and node_ids hash
    Nodes(String, String),
    /// Image export URLs by file_key and params hash
    Images(String, String),
    /// Version history by file key
    Versions(String),
    /// Projects in a team
    TeamProjects(String),
    /// Files in a project
    ProjectFiles(String),
    /// Team library components
    TeamComponents(String),
    /// Team library styles
    TeamStyles(String),
    /// Component detail by key
    Component(String),
}

impl CacheKey {
    /// Convert cache key to string for storage
    pub fn as_string(&self) -> String {
        match self {
            CacheKey::File(key) => format!("file:{}", key),
            CacheKey::FileMeta(key) => format!("file_meta:{}", key),
            CacheKey::Nodes(file, nodes) => format!("nodes:{}:{}", file, nodes),
            CacheKey::Images(file, params) => format!("images:{}:{}", file, params),
            CacheKey::Versions(key) => format!("versions:{}", key),
            CacheKey::TeamProjects(team) => format!("team_projects:{}", team),
            CacheKey::ProjectFiles(project) => format!("project_files:{}", project),
            CacheKey::TeamComponents(team) => format!("team_components:{}", team),
            CacheKey::TeamStyles(team) => format!("team_styles:{}", team),
            CacheKey::Component(key) => format!("component:{}", key),
        }
    }

    /// Create hash of node IDs for cache key
    pub fn hash_node_ids(ids: &[String]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        ids.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Create hash of export parameters
    pub fn hash_export_params(ids: &[String], format: &str, scale: f32) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        ids.hash(&mut hasher);
        format.hash(&mut hasher);
        scale.to_bits().hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

/// Cached entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEntry {
    /// JSON serialized data
    pub data: String,
    /// Unix timestamp when fetched
    pub fetched_at: i64,
    /// TTL in seconds
    pub ttl_seconds: u64,
}

impl CachedEntry {
    /// Check if entry is still valid
    pub fn is_valid(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        let age = now - self.fetched_at;
        age < self.ttl_seconds as i64
    }
}

/// TTL configurations for different resource types
pub struct CacheTTL;

impl CacheTTL {
    /// Full file metadata - 5 minutes
    pub const FILE_METADATA: Duration = Duration::from_secs(300);
    /// Light file metadata - 10 minutes
    pub const FILE_META_LIGHT: Duration = Duration::from_secs(600);
    /// Image export URLs - 30 minutes (Figma S3 URLs expire after some time)
    pub const IMAGE_URLS: Duration = Duration::from_secs(1800);
    /// Version history - 1 minute (changes frequently)
    pub const VERSIONS: Duration = Duration::from_secs(60);
    /// Team data - 1 hour
    pub const TEAM_DATA: Duration = Duration::from_secs(3600);
    /// Component info - 30 minutes
    pub const COMPONENTS: Duration = Duration::from_secs(1800);
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of entries in memory cache
    pub memory_entries: u64,
    /// Weighted size of memory cache
    pub memory_weighted_size: u64,
    /// Whether disk caching is enabled
    pub disk_enabled: bool,
    /// Number of entries on disk
    pub disk_entries: usize,
    /// Disk cache path (if enabled)
    pub disk_path: Option<PathBuf>,
}

/// In-memory + optional disk cache for Figma API
pub struct FigmaCache {
    /// In-memory cache (moka)
    memory: Cache<String, CachedEntry>,
    /// Disk cache directory (optional)
    disk_path: Option<PathBuf>,
    /// Whether disk caching is enabled
    disk_enabled: bool,
}

impl FigmaCache {
    /// Create new cache with optional disk persistence
    ///
    /// # Arguments
    /// * `disk_path` - Optional path for disk-based cache. If Some, enables disk persistence.
    pub fn new(disk_path: Option<PathBuf>) -> Self {
        let memory = Cache::builder()
            .max_capacity(1000)
            .time_to_live(Duration::from_secs(3600)) // Global 1 hour max TTL
            .build();

        let disk_enabled = disk_path.is_some();
        if let Some(ref path) = disk_path {
            if let Err(e) = fs::create_dir_all(path) {
                eprintln!("Warning: Could not create cache directory: {}", e);
            }
        }

        Self {
            memory,
            disk_path,
            disk_enabled,
        }
    }

    /// Create a cache-only (no disk) instance
    pub fn memory_only() -> Self {
        Self::new(None)
    }

    /// Get the default cache directory path
    pub fn default_cache_dir() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "fgm").map(|dirs| dirs.cache_dir().to_path_buf())
    }

    /// Get value from cache (memory first, then disk)
    pub fn get<T: for<'de> Deserialize<'de>>(&self, key: &CacheKey) -> Option<T> {
        let key_str = key.as_string();

        // Check memory cache first
        if let Some(entry) = self.memory.get(&key_str) {
            if entry.is_valid() {
                return serde_json::from_str(&entry.data).ok();
            } else {
                // Expired in memory, remove it
                self.memory.invalidate(&key_str);
            }
        }

        // Check disk cache if enabled
        if self.disk_enabled {
            if let Some(entry) = self.read_disk(&key_str) {
                if entry.is_valid() {
                    // Promote to memory cache
                    self.memory.insert(key_str, entry.clone());
                    return serde_json::from_str(&entry.data).ok();
                } else {
                    // Expired on disk, remove it
                    self.delete_disk(&key_str);
                }
            }
        }

        None
    }

    /// Store value in cache
    pub fn set<T: Serialize>(&self, key: &CacheKey, value: &T, ttl: Duration) {
        let key_str = key.as_string();
        let data = match serde_json::to_string(value) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Warning: Could not serialize value for cache: {}", e);
                return;
            }
        };

        let entry = CachedEntry {
            data,
            fetched_at: chrono::Utc::now().timestamp(),
            ttl_seconds: ttl.as_secs(),
        };

        // Store in memory
        self.memory.insert(key_str.clone(), entry.clone());

        // Store on disk if enabled
        if self.disk_enabled {
            self.write_disk(&key_str, &entry);
        }
    }

    /// Invalidate specific key
    pub fn invalidate(&self, key: &CacheKey) {
        let key_str = key.as_string();
        self.memory.invalidate(&key_str);

        if self.disk_enabled {
            self.delete_disk(&key_str);
        }
    }

    /// Invalidate all entries for a file (by file key prefix)
    pub fn invalidate_file(&self, file_key: &str) {
        // Memory cache - invalidate all (moka doesn't support prefix invalidation efficiently)
        self.memory.invalidate_all();

        // Disk cache - delete files matching the file_key
        if self.disk_enabled {
            if let Some(ref path) = self.disk_path {
                if let Ok(entries) = fs::read_dir(path) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.contains(file_key) {
                            let _ = fs::remove_file(entry.path());
                        }
                    }
                }
            }
        }
    }

    /// Clear all caches
    pub fn clear(&self) {
        self.memory.invalidate_all();

        if let Some(ref path) = self.disk_path {
            if let Err(e) = fs::remove_dir_all(path) {
                eprintln!("Warning: Could not clear disk cache: {}", e);
            }
            if let Err(e) = fs::create_dir_all(path) {
                eprintln!("Warning: Could not recreate cache directory: {}", e);
            }
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            memory_entries: self.memory.entry_count(),
            memory_weighted_size: self.memory.weighted_size(),
            disk_enabled: self.disk_enabled,
            disk_entries: self.count_disk_entries(),
            disk_path: self.disk_path.clone(),
        }
    }

    /// Check if cache has a valid entry for key
    pub fn contains(&self, key: &CacheKey) -> bool {
        let key_str = key.as_string();

        // Check memory
        if let Some(entry) = self.memory.get(&key_str) {
            if entry.is_valid() {
                return true;
            }
        }

        // Check disk
        if self.disk_enabled {
            if let Some(entry) = self.read_disk(&key_str) {
                return entry.is_valid();
            }
        }

        false
    }

    // Private helper methods

    fn disk_key_path(&self, key: &str) -> Option<PathBuf> {
        self.disk_path.as_ref().map(|p| {
            // Sanitize key for filesystem - replace problematic characters
            let safe_key = key
                .replace(':', "_")
                .replace('/', "_")
                .replace('\\', "_")
                .replace(' ', "_");
            p.join(format!("{}.json", safe_key))
        })
    }

    fn read_disk(&self, key: &str) -> Option<CachedEntry> {
        let path = self.disk_key_path(key)?;
        let content = fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn write_disk(&self, key: &str, entry: &CachedEntry) {
        if let Some(path) = self.disk_key_path(key) {
            if let Ok(json) = serde_json::to_string(entry) {
                if let Err(e) = fs::write(&path, json) {
                    eprintln!("Warning: Could not write to disk cache: {}", e);
                }
            }
        }
    }

    fn delete_disk(&self, key: &str) {
        if let Some(path) = self.disk_key_path(key) {
            let _ = fs::remove_file(path);
        }
    }

    fn count_disk_entries(&self) -> usize {
        self.disk_path
            .as_ref()
            .map_or(0, |path| fs::read_dir(path).map_or(0, |entries| entries.count()))
    }
}

/// Create a shared cache instance with disk persistence enabled by default
pub fn create_shared_cache() -> Arc<FigmaCache> {
    let disk_path = FigmaCache::default_cache_dir();
    Arc::new(FigmaCache::new(disk_path))
}

/// Create a memory-only cache (no disk persistence)
pub fn create_memory_cache() -> Arc<FigmaCache> {
    Arc::new(FigmaCache::memory_only())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_string() {
        let key = CacheKey::File("abc123".to_string());
        assert_eq!(key.as_string(), "file:abc123");

        let key = CacheKey::Images("abc".to_string(), "hash".to_string());
        assert_eq!(key.as_string(), "images:abc:hash");
    }

    #[test]
    fn test_memory_cache() {
        let cache = FigmaCache::memory_only();

        let key = CacheKey::File("test123".to_string());
        cache.set(&key, &"test_value".to_string(), Duration::from_secs(60));

        let result: Option<String> = cache.get(&key);
        assert_eq!(result, Some("test_value".to_string()));
    }

    #[test]
    fn test_cache_miss() {
        let cache = FigmaCache::memory_only();
        let key = CacheKey::File("nonexistent".to_string());
        let result: Option<String> = cache.get(&key);
        assert!(result.is_none());
    }

    #[test]
    fn test_cache_invalidate() {
        let cache = FigmaCache::memory_only();
        let key = CacheKey::File("test".to_string());

        cache.set(&key, &"value".to_string(), Duration::from_secs(60));
        assert!(cache.contains(&key));

        cache.invalidate(&key);
        assert!(!cache.contains(&key));
    }

    #[test]
    fn test_hash_node_ids() {
        let ids1 = vec!["1:1".to_string(), "1:2".to_string()];
        let ids2 = vec!["1:1".to_string(), "1:2".to_string()];
        let ids3 = vec!["1:2".to_string(), "1:1".to_string()];

        let hash1 = CacheKey::hash_node_ids(&ids1);
        let hash2 = CacheKey::hash_node_ids(&ids2);
        let hash3 = CacheKey::hash_node_ids(&ids3);

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3); // Different order = different hash
    }
}
