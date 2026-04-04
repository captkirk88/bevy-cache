use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufWriter, Read};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::CacheConfig;
use crate::error::CacheError;

/// A single entry in the cache manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Filename on disk including extension (e.g. `"scene_01.png"`).
    pub file_name: String,
    /// Unix timestamp when the entry was first created.
    pub created_at: u64,
    /// Unix timestamp when the entry was last modified.
    pub modified_at: u64,
    /// Size of the cached data in bytes.
    pub size_bytes: u64,
    /// Per-entry maximum age in seconds. When set **and** greater than
    /// [`CacheConfig::max_age`], this value is used instead of the global
    /// default during expiry checks. An entry can extend its lifetime
    /// beyond the global policy but never shorten it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_secs: Option<u64>,
}

/// Manifest tracking all cached assets. Persisted as RON on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Resource, Default)]
pub struct CacheManifest {
    pub entries: HashMap<String, CacheEntry>,
}

impl CacheManifest {
    // ------------------------------------------------------------------
    // Persistence
    // ------------------------------------------------------------------

    /// Load the manifest from the filesystem. Returns `Default` if the file
    /// does not exist.
    pub fn load_from_disk(config: &CacheConfig) -> Result<Self, CacheError> {
        let path = config.manifest_fs_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => {
                let manifest: CacheManifest = ron::from_str(&contents)?;
                Ok(manifest)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(CacheError::Io(e)),
        }
    }

    /// Persist the manifest to disk as pretty-printed RON.
    pub fn save_to_disk(&self, config: &CacheConfig) -> Result<(), CacheError> {
        config.ensure_cache_dir()?;
        let path = config.manifest_fs_path();
        let pretty = ron::ser::PrettyConfig::default();
        let serialized =
            ron::ser::to_string_pretty(self, pretty).map_err(CacheError::RonSerialize)?;
        std::fs::write(path, serialized)?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Entry management
    // ------------------------------------------------------------------

    /// Store the contents of `reader` in the cache under `key` with the given
    /// file `extension` (e.g. `"png"`). The file written to disk will be named
    /// `"{key}.{extension}"`. Overwrites any existing entry for the same key.
    ///
    /// Accepting a [`Read`] instead of a byte slice means the data is streamed
    /// directly to disk without requiring an intermediate in-memory buffer.
    /// Use [`std::io::Cursor`] to wrap an existing slice or `Vec<u8>` when the
    /// data is already in memory.
    ///
    /// `max_age` optionally sets a per-entry lifetime. It only takes effect
    /// when it exceeds [`CacheConfig::max_age`]; an entry cannot shorten
    /// its lifetime below the global policy.
    pub fn store<R: Read>(
        &mut self,
        config: &CacheConfig,
        key: &str,
        extension: &str,
        mut reader: R,
        max_age: Option<Duration>,
    ) -> Result<(), CacheError> {
        config.validate_key(key)?;
        config.ensure_cache_dir()?;

        let file_name = format!("{key}.{extension}");
        let fs_path = config.file_path(&file_name);
        if let Some(parent) = fs_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(&fs_path)?;
        let mut writer = BufWriter::new(file);
        let size_bytes = std::io::copy(&mut reader, &mut writer)?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let created_at = self
            .entries
            .get(key)
            .map_or(now, |existing| existing.created_at);

        self.entries.insert(
            key.to_owned(),
            CacheEntry {
                file_name,
                created_at,
                modified_at: now,
                size_bytes,
                max_age_secs: max_age.map(|d| d.as_secs()),
            },
        );

        Ok(())
    }

    /// Remove a cache entry and delete its file from disk.
    /// Returns `Ok(())` even if the entry did not exist.
    pub fn remove(&mut self, config: &CacheConfig, key: &str) -> Result<(), CacheError> {
        if let Some(entry) = self.entries.remove(key) {
            let fs_path = config.file_path(&entry.file_name);
            match std::fs::remove_file(&fs_path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(CacheError::Io(e)),
            }
        }
        Ok(())
    }

    /// Check whether a key is present in the manifest.
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Get the entry for a key, if it exists.
    pub fn get(&self, key: &str) -> Option<&CacheEntry> {
        self.entries.get(key)
    }

    /// Return the Bevy asset path suitable for `AssetServer::load`.
    /// Uses the `cache://` asset source scheme.
    pub fn asset_path(&self, key: &str) -> Option<String> {
        self.entries
            .get(key)
            .map(|entry| format!("cache://{}", entry.file_name))
    }

    /// Check whether a cached asset exists for `key` — both in the manifest
    /// and on disk. Returns `false` if the manifest entry is stale (file
    /// was deleted externally).
    pub fn is_cached(&self, config: &CacheConfig, key: &str) -> bool {
        match self.entries.get(key) {
            Some(entry) => config.file_path(&entry.file_name).exists(),
            None => false,
        }
    }

    /// Returns the total size of all cached assets in bytes, as recorded in
    /// the manifest. This is a sum of [`CacheEntry::size_bytes`] across all
    /// entries
    pub fn total_size_bytes(&self) -> u64 {
        self.entries.values().map(|e| e.size_bytes).sum()
    }

    /// If a cached file exists for `key`, load it through the [`AssetServer`]
    /// and return the typed handle. Returns [`CacheError::NotFound`] when
    /// there is no manifest entry or the file is missing from disk.
    ///
    /// This lets callers skip regenerating a dynamic asset when a valid
    /// cached copy is already available:
    ///
    /// ```rust,ignore
    /// fn setup(
    ///     manifest: Res<CacheManifest>,
    ///     config: Res<CacheConfig>,
    ///     asset_server: Res<AssetServer>,
    /// ) {
    ///     let handle: Handle<Image> = manifest
    ///         .load_cached(&config, "scene_thumb", &asset_server)
    ///         .unwrap_or_else(|_| generate_and_cache_thumbnail(/* … */));
    /// }
    /// ```
    pub fn load_cached<A: Asset>(
        &self,
        config: &CacheConfig,
        key: &str,
        asset_server: &AssetServer,
    ) -> Result<Handle<A>, CacheError> {
        let entry = self.entries.get(key).ok_or_else(|| {
            CacheError::NotFound(format!("no cache entry for key '{key}'"))
        })?;
        if !config.file_path(&entry.file_name).exists() {
            return Err(CacheError::NotFound(format!(
                "cache file '{}' missing from disk for key '{key}'",
                entry.file_name
            )));
        }
        let path = format!("cache://{}", entry.file_name);
        Ok(asset_server.load(path))
    }

    // ------------------------------------------------------------------
    // Cleanup helpers (used at exit)
    // ------------------------------------------------------------------

    /// Remove expired entries and delete their files. An entry is expired
    /// when its age exceeds the *effective* max-age, which is the greater of
    /// [`CacheConfig::max_age`] and the entry's own
    /// [`CacheEntry::max_age_secs`] (if set).
    /// Returns the number of entries removed.
    pub fn remove_expired(&mut self, config: &CacheConfig) -> usize {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let global_max = config.max_age.as_secs();
        let expired_keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, entry)| {
                let effective_max = match entry.max_age_secs {
                    Some(per_entry) => per_entry.max(global_max),
                    None => global_max,
                };
                now.saturating_sub(entry.modified_at) > effective_max
            })
            .map(|(key, _)| key.clone())
            .collect();

        let count = expired_keys.len();
        for key in &expired_keys {
            if let Some(entry) = self.entries.remove(key) {
                let fs_path = config.file_path(&entry.file_name);
                let _ = std::fs::remove_file(&fs_path);
            }
        }
        count
    }

    /// If the manifest exceeds `max_entries`, evict the oldest entries
    /// (by `modified_at`) until the limit is satisfied. Returns the number
    /// of entries removed.
    pub fn enforce_max_entries(&mut self, config: &CacheConfig) -> usize {
        let max = match config.max_entries {
            Some(m) => m,
            None => return 0,
        };

        if self.entries.len() <= max {
            return 0;
        }

        let to_remove_count = self.entries.len() - max;

        let mut by_age: Vec<(String, u64)> = self
            .entries
            .iter()
            .map(|(k, e)| (k.clone(), e.modified_at))
            .collect();
        by_age.sort_by_key(|(_, ts)| *ts);

        let mut removed = 0;
        for (key, _) in by_age.into_iter().take(to_remove_count) {
            if let Some(entry) = self.entries.remove(&key) {
                let fs_path = config.file_path(&entry.file_name);
                let _ = std::fs::remove_file(&fs_path);
                removed += 1;
            }
        }
        removed
    }
}
