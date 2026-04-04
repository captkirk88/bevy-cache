use bevy::prelude::*;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

/// Configuration for the file cache system.
///
/// The cache directory defaults to the operating system's cache folder
/// (e.g. `%LOCALAPPDATA%` on Windows, `~/Library/Caches` on macOS,
/// `$XDG_CACHE_HOME` on Linux) joined with the application name.
/// On platforms where no cache directory can be determined (including
/// some Android configurations), [`std::env::temp_dir`] is used as
/// a fallback.
#[derive(Debug, Clone, Resource)]
pub struct CacheConfig {
    /// Application name used to derive the manifest filename
    /// (`"{app_name}.cache_manifest"`).
    pub app_name: String,

    /// Filesystem directory for cache files and the manifest.
    pub cache_dir: PathBuf,

    /// Maximum age of an entry before it becomes eligible for cleanup
    /// at application exit.
    pub max_age: Duration,

    /// Maximum number of cache entries. `None` means unlimited.
    /// Enforced at application exit — new entries are never rejected.
    pub max_entries: Option<usize>,
}

impl CacheConfig {
    /// Create a new config using the OS cache directory for the given app name.
    pub fn new(app_name: &str) -> Self {
        Self {
            app_name: app_name.to_owned(),
            cache_dir: resolve_cache_dir(app_name),
            max_age: Duration::from_secs(604_800), // 7 days
            max_entries: None,
        }
}

    /// Returns the manifest filename, e.g. `"my_game.cache_manifest"`.
    pub fn manifest_file_name(&self) -> String {
        format!("{}.cache_manifest", self.app_name)
    }

    /// Filesystem path for a cached file.
    pub fn file_path(&self, file_name: &str) -> PathBuf {
        self.cache_dir.join(file_name)
    }

    /// Filesystem path for the manifest, e.g.
    /// `<cache_dir>/my_game.cache_manifest`.
    pub fn manifest_fs_path(&self) -> PathBuf {
        self.cache_dir.join(self.manifest_file_name())
    }

    /// Ensure the cache directory exists on disk.
    pub fn ensure_cache_dir(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.cache_dir)
    }

    /// Validate that a cache key is a safe relative path within the cache
    /// root. Forward-slash subpaths are allowed, but traversal, absolute
    /// paths, backslashes, empty segments, and manifest filename collisions
    /// are rejected.
    pub fn validate_key(&self, key: &str) -> Result<(), crate::CacheError> {
        if key.is_empty()
            || key.contains('\0')
            || key.contains('\\')
            || key.starts_with('/')
            || key.ends_with('/')
            || key.split('/').any(str::is_empty)
            || key == self.manifest_file_name()
        {
            return Err(crate::CacheError::InvalidKey(key.to_owned()));
        }

        for component in Path::new(key).components() {
            match component {
                Component::Normal(_) => {}
                Component::CurDir
                | Component::ParentDir
                | Component::RootDir
                | Component::Prefix(_) => {
                    return Err(crate::CacheError::InvalidKey(key.to_owned()));
                }
            }
        }

        Ok(())
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self::new("bevy_cache")
    }
}

/// Resolve the platform-appropriate cache directory. Falls back to the OS temp
/// dir when no user-specific cache folder is available (e.g. on Android).
fn resolve_cache_dir(app_name: &str) -> PathBuf {
    sysdirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(app_name)
}
