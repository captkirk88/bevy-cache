use bevy::prelude::*;

use crate::config::CacheConfig;
use crate::error::CacheError;
use crate::manifest::CacheManifest;

/// Extension trait that adds cache-aware loading to [`AssetServer`].
///
/// # Example
/// ```rust,ignore
/// use bevy_cache::AssetServerCacheExt;
///
/// fn setup(
///     asset_server: Res<AssetServer>,
///     manifest: Res<CacheManifest>,
///     config: Res<CacheConfig>,
/// ) {
///     let handle: Handle<Image> = asset_server
///         .load_cached(&manifest, &config, "scene_thumb")
///         .unwrap_or_else(|_| regenerate_thumbnail(&asset_server));
/// }
/// ```
pub trait AssetServerCacheExt {
    /// Load a cached asset by key.
    ///
    /// Returns `Ok(Handle<A>)` when the cache file is present, or
    /// [`CacheError::NotFound`] when there is no manifest entry or the
    /// file was deleted from disk.
    fn load_cached<A: Asset>(
        &self,
        manifest: &CacheManifest,
        config: &CacheConfig,
        key: &str,
    ) -> Result<Handle<A>, CacheError>;
}

impl AssetServerCacheExt for AssetServer {
    fn load_cached<A: Asset>(
        &self,
        manifest: &CacheManifest,
        config: &CacheConfig,
        key: &str,
    ) -> Result<Handle<A>, CacheError> {
        manifest.load_cached(config, key, self)
    }
}
