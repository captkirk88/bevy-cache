use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::{io::Read, path::Path};
use std::time::Duration;

use crate::{CacheConfig, CacheEntry, CacheError, CacheManifest};

/// Combined system parameter for the cache manifest and config resources.
///
/// This removes the need to request both `ResMut<CacheManifest>` and
/// `Res<CacheConfig>` in every system that interacts with the cache.
///
/// ```rust,ignore
/// use bevy::prelude::*;
/// use bevy_cache::prelude::*;
///
/// fn cache_screenshot(
///     mut cache: Cache,
///     asset_server: Res<AssetServer>,
/// ) {
///     cache
///         .store(
///             "screenshots/title",
///             "png",
///             std::io::Cursor::new(vec![1, 2, 3]),
///             None,
///         )
///         .expect("cache write failed");
///
///     let _handle: Handle<Image> = cache
///         .load_cached(&asset_server, "screenshots/title")
///         .expect("cache load failed");
/// }
/// ```
#[derive(SystemParam)]
pub struct Cache<'w> {
    manifest: ResMut<'w, CacheManifest>,
    config: Res<'w, CacheConfig>,
}

impl<'w> Cache<'w> {

    pub fn cache_dir(&self) -> &Path {
        self.config.cache_dir.as_path()
    }
    
    /// Returns the cache configuration resource.
    pub fn config(&self) -> &CacheConfig {
        self.config.as_ref()
    }

    /// Returns the cache manifest resource.
    pub fn manifest(&self) -> &CacheManifest {
        self.manifest.as_ref()
    }

    /// Returns the cache manifest resource mutably.
    pub fn manifest_mut(&mut self) -> &mut CacheManifest {
        self.manifest.as_mut()
    }

    /// Store data in the cache under `key` using the given file extension.
    pub fn store<R: Read>(
        &mut self,
        key: &str,
        extension: &str,
        reader: R,
        max_age: Option<Duration>,
    ) -> Result<(), CacheError> {
        self.manifest.store(self.config.as_ref(), key, extension, reader, max_age)
    }

    /// Remove a cache entry and its backing file from disk.
    pub fn remove(&mut self, key: &str) -> Result<(), CacheError> {
        self.manifest.remove(self.config.as_ref(), key)
    }

    /// Check whether a key exists in the manifest.
    pub fn contains(&self, key: &str) -> bool {
        self.manifest.contains(key)
    }

    /// Get a manifest entry by key.
    pub fn get(&self, key: &str) -> Option<&CacheEntry> {
        self.manifest.get(key)
    }

    /// Return the `cache://` asset path for `key`, if present.
    pub fn asset_path(&self, key: &str) -> Option<String> {
        self.manifest.asset_path(key)
    }

    /// Check whether the cached file for `key` still exists on disk.
    pub fn is_cached(&self, key: &str) -> bool {
        self.manifest.is_cached(self.config.as_ref(), key)
    }

    /// Returns the total manifest-reported size of cached data in bytes.
    pub fn total_size_bytes(&self) -> u64 {
        self.manifest.total_size_bytes()
    }

    /// Load a cached asset path through the provided [`AssetServer`].
    ///
    /// This returns `None` when the key does not exist in the manifest.
    pub fn load<A: Asset>(&self, asset_server: &AssetServer, key: &str) -> Option<Handle<A>> {
        self.asset_path(key).map(|path| asset_server.load(path))
    }

    /// Load a cached asset through the provided [`AssetServer`], returning an
    /// error when the manifest entry is missing or stale.
    pub fn load_cached<A: Asset>(
        &self,
        asset_server: &AssetServer,
        key: &str,
    ) -> Result<Handle<A>, CacheError> {
        self.manifest.load_cached(self.config.as_ref(), key, asset_server)
    }

    /// Returns the modified asset for `handle` when a matching
    /// [`AssetEvent::Modified`] is present in `events`.
    ///
    /// This is only available with the `hot_reload` feature enabled.
    #[cfg(feature = "hot_reload")]
    pub fn get_if_modified<'a, A: Asset>(
        &self,
        handle: &Handle<A>,
        assets: &'a Assets<A>,
        messages: &'a mut MessageReader<AssetEvent<A>>,
    ) -> Option<&'a A> {
        for event in messages.read() {
            if let AssetEvent::Modified { id } = event {
                if *id == handle.id() {
                    return assets.get(handle);
                }
            }
        }

        None
    }
}