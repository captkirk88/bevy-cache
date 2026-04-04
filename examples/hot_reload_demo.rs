//! Live hot-reload of a runtime-generated cached asset.
//!
//! # What this demo does
//! 1. On **first run** a [`ScoreboardAsset`] is serialised to RON and written
//!    to the OS cache directory via [`CacheManifest::store`].
//! 2. The file is immediately loaded back through the `cache://` asset source,
//!    which is registered with a filesystem watcher when the `hot_reload`
//!    feature is active.
//! 3. A system listens for [`AssetEvent::Modified`] and logs the new content
//!    whenever the backing file is changed on disk — no app restart needed.
//! 4. The cache manifest itself is also watched; any external change is
//!    synced back into the [`CacheManifest`] Bevy resource automatically.
//!
//! # How to run
//! ```text
//! cargo run --example hot_reload_demo --features hot_reload
//! ```
//!
//! The path to the cached file is printed on startup.  Open it in a text
//! editor, change `top_score` or `player_name`, save the file, and watch
//! the log update in real time without restarting the app.
//!
//! On subsequent runs the cached file is preserved (not overwritten) so your
//! edits persist across restarts.

use bevy::{
    app::{AppExit, ScheduleRunnerPlugin},
    asset::{AssetLoader, LoadContext, io::Reader},
    image::ImagePlugin,
    prelude::*,
    reflect::TypePath,
    window::WindowPlugin,
    winit::WinitPlugin,
};
use bevy_cache::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

const APP_NAME: &str = "bevy_cache_hot_reload_demo";
const SCOREBOARD_KEY: &str = "scoreboard";

// ---------------------------------------------------------------------------
// Asset type
// ---------------------------------------------------------------------------

/// A simple scoreboard that can be edited on disk to trigger a hot-reload.
#[derive(Asset, TypePath, Debug, Clone, Serialize, Deserialize)]
struct ScoreboardAsset {
    top_score: u64,
    player_name: String,
    notes: String,
}

// ---------------------------------------------------------------------------
// Asset loader
// ---------------------------------------------------------------------------

#[derive(Default, TypePath)]
struct ScoreboardLoader;

#[non_exhaustive]
#[derive(Debug, Error)]
enum ScoreboardLoaderError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("RON parse error: {0}")]
    Ron(#[from] ron::error::SpannedError),
}

impl AssetLoader for ScoreboardLoader {
    type Asset = ScoreboardAsset;
    type Settings = ();
    type Error = ScoreboardLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(ron::de::from_bytes::<ScoreboardAsset>(&bytes)?)
    }

    fn extensions(&self) -> &[&str] {
        &["scoreboard"]
    }
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct CachedHandles {
    scoreboard: Handle<ScoreboardAsset>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> AppExit {
    App::new()
        .add_plugins(BevyCachePlugin::new(APP_NAME))
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default())
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    ..default()
                })
                .set(AssetPlugin {
                    // Required to activate Bevy's file watcher so that changes
                    // to files under `cache://` are detected.
                    watch_for_changes_override: Some(true),
                    ..default()
                })
                .disable::<WinitPlugin>(),
        )
        // Poll at ~5 Hz – fast enough to feel snappy without burning CPU.
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_millis(200)))
        .init_asset::<ScoreboardAsset>()
        .init_asset_loader::<ScoreboardLoader>()
        .insert_resource(CachedHandles::default())
        .add_systems(PostStartup, seed_and_load_cache)
        .add_systems(Update, (watch_asset_changes, watch_manifest_changes))
        .run()
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// On first run, generates a scoreboard, writes it to the cache, and loads it
/// back via the asset server.  On subsequent runs the cached file is reused
/// (preserving any edits the user made between runs).
fn seed_and_load_cache(
    mut commands: Commands,
    mut cache: Cache,
    asset_server: Res<AssetServer>,
) {
    if cache.is_cached(SCOREBOARD_KEY) {
        info!("Cache hit — loading existing scoreboard (your edits are preserved).");
    } else {
        let board = ScoreboardAsset {
            top_score: 9_000,
            player_name: "Player One".to_owned(),
            notes: "Edit this file while the app is running to see hot-reload!".to_owned(),
        };

        let serialized = ron::ser::to_string_pretty(&board, ron::ser::PrettyConfig::default())
            .expect("failed to serialise scoreboard");

        cache
            .store(
                SCOREBOARD_KEY,
                "scoreboard",
                std::io::Cursor::new(serialized),
                None,
            )
            .expect("failed to cache scoreboard");

        info!("Cache miss — created initial scoreboard.");
    }

    let fs_path = cache.config().file_path("scoreboard.scoreboard");
    info!("Cached file path: {}", fs_path.display());
    info!("Open the file above, edit it, and save to see hot-reload in action.");
    info!("Press Ctrl+C to exit.");

    let handle = cache
        .load_cached::<ScoreboardAsset>(&asset_server, SCOREBOARD_KEY)
        .expect("failed to load cached scoreboard");

    commands.insert_resource(CachedHandles { scoreboard: handle });
}

/// Logs the updated content whenever the scoreboard file is modified on disk.
fn watch_asset_changes(
    cache: Cache,
    mut events: MessageReader<AssetEvent<ScoreboardAsset>>,
    assets: Res<Assets<ScoreboardAsset>>,
    handles: Option<Res<CachedHandles>>,
) {
    let Some(handles) = handles else { return };

    if let Some(board) = cache.get_if_modified(&handles.scoreboard, assets.as_ref(), &mut events) {
        info!("--- Scoreboard hot-reloaded! ---");
        info!("  top_score:   {}", board.top_score);
        info!("  player_name: {}", board.player_name);
        info!("  notes:       {}", board.notes);
    }
}

/// Logs whenever the manifest itself is reloaded from disk (e.g. an external
/// tool adds or removes cache entries while the app is running).
fn watch_manifest_changes(manifest: Res<CacheManifest>) {
    if manifest.is_changed() {
        let count = manifest.entries.len();
        info!(
            "Manifest changed: now has {} cache entr{}.",
            count,
            if count == 1 { "y" } else { "ies" }
        );
    }
}
