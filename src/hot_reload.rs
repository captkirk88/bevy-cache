//! Hot-reload support for the cache manifest and cached assets.
//!
//! Enabled via the `hot_reload` Cargo feature. When active:
//!
//! - The `cache://` asset source is registered with a filesystem watcher so
//!   any cached asset already held as a [`Handle`] is automatically reloaded
//!   by Bevy when its backing file changes on disk.
//! - The manifest is loaded through Bevy's [`AssetServer`] and
//!   [`CacheManifest`] is re-synced whenever `manifest.cache_manifest` is
//!   modified on disk.
//!
//! Bevy's own file-watching must be enabled by the application:
//!
//! ```rust,ignore
//! # use bevy::prelude::*;
//! # use bevy_cache::BevyCachePlugin;
//! App::new()
//!     .add_plugins(BevyCachePlugin::new("my_game"))
//!     .add_plugins(DefaultPlugins.set(AssetPlugin {
//!         watch_for_changes_override: Some(true),
//!         ..default()
//!     }))
//!     .run();
//! ```

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::prelude::*;

use crate::config::CacheConfig;
use crate::manifest::CacheManifest;

/// Bevy [`Asset`] wrapper used to watch the cache manifest file for changes.
///
/// There is no need to interact with this type directly; it is managed
/// internally by [`BevyCachePlugin`](crate::BevyCachePlugin) when the
/// `hot_reload` feature is enabled.
#[derive(Asset, TypePath, Clone)]
pub struct CacheManifestAsset(pub CacheManifest);

/// Resource that holds the [`Handle`] to the manifest asset and tracks
/// whether the next save should be suppressed after a hot-reload to prevent
/// a write → watch → reload loop.
#[derive(Resource, Default)]
pub struct ManifestHotReload {
    /// Handle to the loaded manifest asset.
    pub handle: Option<Handle<CacheManifestAsset>>,
    /// Set to `true` by [`sync_manifest_from_asset`] so that the following
    /// [`save_manifest_skip_reload`] call does not re-write the file.
    pub(crate) skip_next_save: bool,
}

/// RON [`AssetLoader`] for `.cache_manifest` files.
#[derive(Default, TypePath)]
pub struct CacheManifestLoader;

impl AssetLoader for CacheManifestLoader {
    type Asset = CacheManifestAsset;
    type Settings = ();
    type Error = Box<dyn std::error::Error + Send + Sync>;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let manifest: CacheManifest = ron::from_str(std::str::from_utf8(&bytes)?)?;
        Ok(CacheManifestAsset(manifest))
    }

    fn extensions(&self) -> &[&str] {
        &["cache_manifest"]
    }
}

/// Startup system: ensures the manifest file exists on disk, then loads it
/// via [`AssetServer`] so Bevy's file watcher can track changes to it.
///
/// Runs **after** `load_manifest` so the [`CacheManifest`] resource is
/// already populated by the time this system runs.
pub fn startup_watch_manifest(
    mut state: ResMut<ManifestHotReload>,
    asset_server: Res<AssetServer>,
    config: Res<CacheConfig>,
) {
    let path = config.manifest_fs_path();
    if !path.exists() {
        let _ = config.ensure_cache_dir();
        let pretty = ron::ser::PrettyConfig::default();
        let empty = ron::ser::to_string_pretty(&CacheManifest::default(), pretty)
            .expect("serialize empty manifest");
        let _ = std::fs::write(&path, empty);
    }
    let handle = asset_server
        .load::<CacheManifestAsset>(format!("cache://{}", config.manifest_file_name()));
    state.handle = Some(handle);
}

/// Re-syncs [`CacheManifest`] from the asset whenever the manifest file is
/// **modified** on disk.
///
/// The initial `LoadedWithDependencies` event is intentionally ignored to
/// avoid overwriting in-memory changes that happened after the synchronous
/// startup load but before the async asset finished loading.
///
/// When the on-disk content actually differs, the resource is updated and the
/// next save is suppressed to prevent a write → watch → reload loop.
pub fn sync_manifest_from_asset(
    mut manifest: ResMut<CacheManifest>,
    mut state: ResMut<ManifestHotReload>,
    assets: Res<Assets<CacheManifestAsset>>,
    mut events: MessageReader<AssetEvent<CacheManifestAsset>>,
) {
    let Some(handle) = state.handle.clone() else {
        return;
    };

    for event in events.read() {
        // Only react to file modifications — not the initial load.
        let AssetEvent::Modified { id } = event else {
            continue;
        };
        if *id != handle.id() {
            continue;
        }
        if let Some(asset) = assets.get(&handle) {
            if *manifest != asset.0 {
                tracing::info!("Cache manifest hot-reloaded from disk.");
                state.skip_next_save = true;
                *manifest = asset.0.clone();
            }
        }
    }
}

/// Drop-in replacement for `save_manifest_on_change` that skips one save
/// cycle after a hot-reload to prevent a write → watch → reload loop.
pub fn save_manifest_skip_reload(
    config: Res<CacheConfig>,
    manifest: Res<CacheManifest>,
    mut state: ResMut<ManifestHotReload>,
) {
    if state.skip_next_save {
        state.skip_next_save = false;
        return;
    }
    if !manifest.is_changed() {
        return;
    }
    if let Err(e) = manifest.save_to_disk(config.as_ref()) {
        tracing::error!("Failed to save cache manifest: {e}");
    }
}
