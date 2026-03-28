mod asset_server_ext;
mod config;
mod error;
mod manifest;
mod save_queue;
mod systems;

#[cfg(feature = "hot_reload")]
pub mod hot_reload;

pub use asset_server_ext::AssetServerCacheExt;
pub use config::CacheConfig;
pub use error::CacheError;
pub use manifest::{CacheEntry, CacheManifest};
pub use save_queue::CacheQueue;

#[cfg(not(feature = "hot_reload"))]
use bevy::asset::io::file::FileAssetReader;
use bevy::asset::io::AssetSourceBuilder;
use bevy::prelude::*;

pub mod prelude {
    pub use crate::AssetServerCacheExt;
    pub use crate::{BevyCachePlugin, CacheError, CacheEntry, CacheConfig, CacheManifest, CacheQueue};
}

/// Plugin that registers the file cache system.
///
/// Registers a `"cache"` asset source that maps to the OS cache directory.
/// Any Bevy asset type can be loaded from the cache using the `cache://` scheme;
/// Bevy's built-in loaders handle the file based on its real extension.
///
/// **Must be added before `DefaultPlugins`** so that the asset source is
/// available when `AssetPlugin` initialises.
///
/// # Example
/// ```rust,ignore
/// use bevy::prelude::*;
/// use bevy_cache::BevyCachePlugin;
///
/// App::new()
///     .add_plugins(BevyCachePlugin::new("my_game"))
///     .add_plugins(DefaultPlugins)
///     .run();
/// ```
///
/// ## Hot-reloading
///
/// Enable the `hot_reload` Cargo feature and configure Bevy's file watcher to
/// automatically reload cached assets and the manifest when files change:
///
/// ```rust,ignore
/// App::new()
///     .add_plugins(BevyCachePlugin::new("my_game"))
///     .add_plugins(DefaultPlugins.set(AssetPlugin {
///         watch_for_changes_override: Some(true),
///         ..default()
///     }))
///     .run();
/// ```
///
/// With `hot_reload` enabled the manifest is saved as `manifest.cache_manifest`
/// instead of `manifest.ron`.
///
/// After adding the plugin, use [`CacheManifest`] to store and query cached
/// assets, and [`CacheQueue`] to enqueue asset handles for deferred caching:
///
/// ```rust,ignore
/// fn cache_screenshot(
///     mut manifest: ResMut<CacheManifest>,
///     config: Res<CacheConfig>,
/// ) {
///     let png_data: Vec<u8> = render_my_screenshot();
///     manifest.store(&config, "scene_01", "png", std::io::Cursor::new(png_data), None)
///         .expect("cache write failed");
/// }
///
/// fn cache_asset_by_handle(
///     mut pending: ResMut<CacheQueue>,
///     assets: Res<Assets<MyAsset>>,
///     handle: Res<MyAssetHandle>,
/// ) {
///     if let Some(asset) = assets.get(&handle.0) {
///         // Reflect-based: serialized to RON via ReflectSerializer
///         pending.enqueue_reflect(
///             Box::new(asset.clone()),
///             "my_asset_key",
///             "ron",
///             None,
///         );
///     }
/// }
///
/// fn load_cached(
///     manifest: Res<CacheManifest>,
///     asset_server: Res<AssetServer>,
/// ) {
///     if let Some(path) = manifest.asset_path("scene_01") {
///         // Bevy detects ".png" and uses ImageLoader automatically.
///         let handle: Handle<Image> = asset_server.load(path);
///     }
/// }
/// ```
#[derive(Default)]
pub struct BevyCachePlugin {
    pub config: CacheConfig,
}

impl BevyCachePlugin {
    pub fn new(app_name: &str) -> Self {
        Self {
            config: CacheConfig::new(app_name),
        }
    }
}

impl Plugin for BevyCachePlugin {
    fn build(&self, app: &mut App) {
        let cache_dir = self.config.cache_dir.clone();

        // Under `hot_reload`, use `platform_default` so Bevy's file watcher
        // is wired up for the cache source automatically.
        #[cfg(feature = "hot_reload")]
        {
            let cache_dir_str = cache_dir.to_string_lossy().into_owned();
            app.register_asset_source(
                "cache",
                AssetSourceBuilder::platform_default(&cache_dir_str, None),
            );
        }
        #[cfg(not(feature = "hot_reload"))]
        app.register_asset_source(
            "cache",
            AssetSourceBuilder::new(
                move || Box::new(FileAssetReader::new(cache_dir.clone())),
            ),
        );

        app.insert_resource(self.config.clone())
            .init_resource::<save_queue::CacheQueue>()
            .add_systems(Startup, systems::load_manifest)
            .add_systems(Last, systems::cleanup_on_exit);

        #[cfg(not(feature = "hot_reload"))]
        app.add_systems(PostUpdate, (
            save_queue::process_pending_saves,
            systems::save_manifest_on_change,
        ).chain());

        #[cfg(feature = "hot_reload")]
        app
            .init_resource::<hot_reload::ManifestHotReload>()
            .add_systems(
                Startup,
                hot_reload::startup_watch_manifest.after(systems::load_manifest),
            )
            .add_systems(PostUpdate, (
                save_queue::process_pending_saves,
                hot_reload::sync_manifest_from_asset,
                hot_reload::save_manifest_skip_reload,
            ).chain());
    }

    /// Called after all plugins have had `build` run — by this point
    /// `AssetPlugin` (from `DefaultPlugins`) has initialised `AssetServer`,
    /// so it is safe to call `init_asset` / `register_asset_loader`.
    #[cfg(feature = "hot_reload")]
    fn finish(&self, app: &mut App) {
        app.init_asset::<hot_reload::CacheManifestAsset>()
            .register_asset_loader(hot_reload::CacheManifestLoader::default());
    }
}
