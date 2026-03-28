use bevy::app::AppExit;
use bevy::prelude::*;

use crate::config::CacheConfig;
use crate::manifest::CacheManifest;

/// Startup system: loads the manifest from disk (or creates a default one).
pub fn load_manifest(mut commands: Commands, config: Res<CacheConfig>) {
    let manifest = match CacheManifest::load_from_disk(&config) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("Failed to load cache manifest, starting fresh: {e}");
            CacheManifest::default()
        }
    };
    commands.insert_resource(manifest);
}

/// Runs when [`AppExit`] is received. Removes only entries whose expiry period
/// has passed, enforces the max-entries limit, and persists the manifest.
pub fn cleanup_on_exit(
    mut exit_messages: MessageReader<AppExit>,
    config: Res<CacheConfig>,
    mut manifest: ResMut<CacheManifest>,
) {
    if exit_messages.read().next().is_none() {
        return;
    }

    let expired = manifest.remove_expired(&config);
    let evicted = manifest.enforce_max_entries(&config);

    if expired + evicted > 0 {
        tracing::debug!("Exit cache cleanup: removed {expired} expired, {evicted} over limit");
    }

    if let Err(e) = manifest.save_to_disk(&config) {
        tracing::error!("Failed to save cache manifest on exit: {e}");
    }
}

/// Persist the manifest to disk whenever it has changed (e.g. after a `store`
/// call during the frame).
#[cfg(not(feature = "hot_reload"))]
pub fn save_manifest_on_change(config: Res<CacheConfig>, manifest: Res<CacheManifest>) {
    if !manifest.is_changed() {
        return;
    }

    if let Err(e) = manifest.save_to_disk(config.as_ref()) {
        tracing::error!("Failed to save cache manifest: {e}");
    }
}
