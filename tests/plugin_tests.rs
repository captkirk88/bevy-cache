mod helpers;

use bevy_cache::prelude::*;

// =========================================================================
// Full usage scenario: store → query → load path
// =========================================================================

#[test]
fn end_to_end_store_and_asset_path() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let (mut app, config) = helpers::test_app_with_manifest(dir.path());

    // Store a fake cached image via the manifest resource.
    {
        let mut manifest = app
            .world_mut()
            .get_resource_mut::<CacheManifest>()
            .expect("manifest resource");
        manifest
            .store(&config, "quickbar_slot_1", "png", std::io::Cursor::new(b"PNG_DATA"), None)
            .expect("store should succeed");
    }

    // Run an update so save_manifest_on_change fires.
    app.update();

    // Verify the manifest was persisted.
    let loaded =
        CacheManifest::load_from_disk(&config).expect("should load saved manifest from disk");
    assert!(loaded.contains("quickbar_slot_1"));
    assert_eq!(
        loaded.asset_path("quickbar_slot_1"),
        Some("cache://quickbar_slot_1.png".to_owned())
    );
}

// =========================================================================
// Manifest persisted on change across updates
// =========================================================================

#[test]
fn manifest_persists_across_updates() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let (mut app, config) = helpers::test_app_with_manifest(dir.path());

    {
        let mut manifest = app
            .world_mut()
            .get_resource_mut::<CacheManifest>()
            .expect("manifest");
        manifest
            .store(&config, "alpha", "dat", std::io::Cursor::new(b"111"), None)
            .expect("store alpha");
    }
    app.update();

    {
        let mut manifest = app
            .world_mut()
            .get_resource_mut::<CacheManifest>()
            .expect("manifest");
        manifest
            .store(&config, "beta", "dat", std::io::Cursor::new(b"222"), None)
            .expect("store beta");
    }
    app.update();

    let loaded = CacheManifest::load_from_disk(&config).expect("load");
    assert!(loaded.contains("alpha"));
    assert!(loaded.contains("beta"));
}
