mod helpers;

use bevy_cache::prelude::*;
use std::path::PathBuf;

fn temp_config(dir: &tempfile::TempDir) -> CacheConfig {
    CacheConfig {
        app_name: "test_app".to_owned(),
        cache_dir: dir.path().to_path_buf(),
        max_age: std::time::Duration::from_secs(604_800),
        max_entries: None,
    }
}

// =========================================================================
// Store / get / contains / remove
// =========================================================================

#[test]
fn store_and_retrieve_entry() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = temp_config(&dir);
    let mut manifest = CacheManifest::default();

    let data = b"fake png bytes";
    manifest
        .store(&config, "shot_1", "png", std::io::Cursor::new(data), None)
        .expect("store should succeed");

    assert!(manifest.contains("shot_1"));
    let entry = manifest.get("shot_1").expect("entry should exist");
    assert_eq!(entry.file_name, "shot_1.png");
    assert_eq!(entry.size_bytes, data.len() as u64);

    // File should exist on disk
    let fs_path = config.file_path("shot_1.png");
    assert!(fs_path.exists(), "cached file should exist on disk");
    let on_disk = std::fs::read(&fs_path).expect("should read cached file");
    assert_eq!(on_disk, data);
}

#[test]
fn store_overwrites_existing_entry() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = temp_config(&dir);
    let mut manifest = CacheManifest::default();

    manifest
        .store(&config, "key", "bin", std::io::Cursor::new(b"first"), None)
        .expect("first store");
    let created = manifest.get("key").expect("entry").created_at;

    // Small sleep to ensure different timestamp is possible (not required —
    // created_at should be preserved regardless).
    manifest
        .store(&config, "key", "bin", std::io::Cursor::new(b"second"), None)
        .expect("second store");

    let entry = manifest.get("key").expect("entry should still exist");
    assert_eq!(entry.created_at, created, "created_at must be preserved");
    assert_eq!(entry.size_bytes, 6);
}

#[test]
fn remove_deletes_file_and_entry() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = temp_config(&dir);
    let mut manifest = CacheManifest::default();

    manifest
        .store(&config, "rm_me", "dat", std::io::Cursor::new(b"data"), None)
        .expect("store");
    let fs_path = config.file_path("rm_me.dat");
    assert!(fs_path.exists());

    manifest.remove(&config, "rm_me").expect("remove should succeed");
    assert!(!manifest.contains("rm_me"));
    assert!(!fs_path.exists(), "file should be deleted from disk");
}

#[test]
fn remove_nonexistent_is_ok() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = temp_config(&dir);
    let mut manifest = CacheManifest::default();
    manifest
        .remove(&config, "nope")
        .expect("removing absent key should succeed");
}

// =========================================================================
// asset_path
// =========================================================================

#[test]
fn asset_path_returns_cache_scheme() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = temp_config(&dir);
    let mut manifest = CacheManifest::default();

    manifest
        .store(&config, "img", "png", std::io::Cursor::new(b"px"), None)
        .expect("store");

    let path = manifest.asset_path("img").expect("should have path");
    assert_eq!(path, "cache://img.png");
}

#[test]
fn asset_path_returns_none_for_missing() {
    let manifest = CacheManifest::default();
    assert!(manifest.asset_path("missing").is_none());
}

// =========================================================================
// Persistence round-trip
// =========================================================================

#[test]
fn manifest_save_and_load_roundtrip() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = temp_config(&dir);
    let mut manifest = CacheManifest::default();

    manifest
        .store(&config, "a", "png", std::io::Cursor::new(b"aaa"), None)
        .expect("store a");
    manifest
        .store(&config, "b", "jpg", std::io::Cursor::new(b"bbbbb"), None)
        .expect("store b");

    manifest.save_to_disk(&config).expect("save");

    let loaded = CacheManifest::load_from_disk(&config).expect("load");
    assert_eq!(loaded.entries.len(), 2);
    assert!(loaded.contains("a"));
    assert!(loaded.contains("b"));
}

#[test]
fn load_from_nonexistent_returns_default() {
    let config = CacheConfig {
        app_name: "test_app".to_owned(),
        cache_dir: PathBuf::from("/tmp/nonexistent_bevy_cache_test_dir_12345"),
        max_age: std::time::Duration::from_secs(60),
        max_entries: None,
    };
    let manifest = CacheManifest::load_from_disk(&config).expect("should return default");
    assert!(manifest.entries.is_empty());
}

// =========================================================================
// Expiry cleanup
// =========================================================================

#[test]
fn remove_expired_deletes_old_entries() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = CacheConfig {
        app_name: "test_app".to_owned(),
        cache_dir: dir.path().to_path_buf(),
        max_age: std::time::Duration::ZERO, // everything is immediately expired
        max_entries: None,
    };
    let mut manifest = CacheManifest::default();

    manifest
        .store(&config, "old", "bin", std::io::Cursor::new(b"x"), None)
        .expect("store");
    manifest
        .entries
        .get_mut("old")
        .expect("entry should exist")
        .modified_at = 0;
    assert!(manifest.contains("old"));

    let removed = manifest.remove_expired(&config);
    assert_eq!(removed, 1);
    assert!(!manifest.contains("old"));
}

#[test]
fn remove_expired_keeps_fresh_entries() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = CacheConfig {
        app_name: "test_app".to_owned(),
        cache_dir: dir.path().to_path_buf(),
        max_age: std::time::Duration::from_secs(999_999), // far future
        max_entries: None,
    };
    let mut manifest = CacheManifest::default();

    manifest
        .store(&config, "fresh", "bin", std::io::Cursor::new(b"x"), None)
        .expect("store");

    let removed = manifest.remove_expired(&config);
    assert_eq!(removed, 0);
    assert!(manifest.contains("fresh"));
}

#[test]
fn per_entry_max_age_extends_lifetime() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = CacheConfig {
        app_name: "test_app".to_owned(),
        cache_dir: dir.path().to_path_buf(),
        max_age: std::time::Duration::ZERO, // global: expire immediately
        max_entries: None,
    };
    let mut manifest = CacheManifest::default();

    // Store with a per-entry max_age that exceeds the global policy.
    manifest
        .store(
            &config,
            "long_lived",
            "bin",
            std::io::Cursor::new(b"x"),
            Some(std::time::Duration::from_secs(999_999)),
        )
        .expect("store");

    // Even though global max_age is ZERO, the entry's own max_age wins.
    let removed = manifest.remove_expired(&config);
    assert_eq!(removed, 0, "per-entry max_age should keep the entry alive");
    assert!(manifest.contains("long_lived"));
}

#[test]
fn per_entry_max_age_cannot_shorten_below_global() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = CacheConfig {
        app_name: "test_app".to_owned(),
        cache_dir: dir.path().to_path_buf(),
        max_age: std::time::Duration::from_secs(999_999), // global: far future
        max_entries: None,
    };
    let mut manifest = CacheManifest::default();

    // Store with a per-entry max_age shorter than global. Global wins.
    manifest
        .store(
            &config,
            "short_request",
            "bin",
            std::io::Cursor::new(b"x"),
            Some(std::time::Duration::ZERO),
        )
        .expect("store");

    let removed = manifest.remove_expired(&config);
    assert_eq!(
        removed, 0,
        "entry cannot expire sooner than global max_age"
    );
    assert!(manifest.contains("short_request"));
}

// =========================================================================
// Max entries enforcement
// =========================================================================

#[test]
fn enforce_max_entries_evicts_oldest() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = CacheConfig {
        app_name: "test_app".to_owned(),
        cache_dir: dir.path().to_path_buf(),
        max_age: std::time::Duration::from_secs(999_999),
        max_entries: Some(2),
    };
    let mut manifest = CacheManifest::default();

    // Insert three entries with increasing modified_at via manual insertion
    // so we can control timestamps.
    for (i, key) in ["oldest", "middle", "newest"].iter().enumerate() {
        manifest.entries.insert(
            key.to_string(),
            bevy_cache::CacheEntry {
                file_name: format!("{key}.bin"),
                created_at: i as u64,
                modified_at: i as u64,
                size_bytes: 1,
                max_age_secs: None,
            },
        );
        // Write a dummy file so removal doesn't error
        let p = config.file_path(&format!("{key}.bin"));
        std::fs::create_dir_all(dir.path()).expect("mkdir");
        std::fs::write(&p, b"x").expect("write");
    }

    let evicted = manifest.enforce_max_entries(&config);
    assert_eq!(evicted, 1, "should evict one entry to meet limit of 2");
    assert!(!manifest.contains("oldest"), "oldest should be evicted");
    assert!(manifest.contains("middle"));
    assert!(manifest.contains("newest"));
}

// =========================================================================
// is_cached
// =========================================================================

#[test]
fn is_cached_true_when_entry_and_file_exist() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = temp_config(&dir);
    let mut manifest = CacheManifest::default();

    manifest
        .store(&config, "img", "png", std::io::Cursor::new(b"px"), None)
        .expect("store");

    assert!(manifest.is_cached(&config, "img"));
}

#[test]
fn is_cached_false_when_no_entry() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = temp_config(&dir);
    let manifest = CacheManifest::default();

    assert!(!manifest.is_cached(&config, "missing"));
}

#[test]
fn is_cached_false_when_file_deleted_externally() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = temp_config(&dir);
    let mut manifest = CacheManifest::default();

    manifest
        .store(&config, "gone", "bin", std::io::Cursor::new(b"data"), None)
        .expect("store");

    // Delete the file behind the manifest's back
    let fs_path = config.file_path("gone.bin");
    std::fs::remove_file(&fs_path).expect("delete file");

    assert!(!manifest.is_cached(&config, "gone"));
}

// =========================================================================
// load_cached
// =========================================================================

#[test]
fn load_cached_returns_not_found_for_missing_key() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = temp_config(&dir);
    let manifest = CacheManifest::default();

    let mut app = helpers::test_app(dir.path());
    app.update();

    let asset_server = app.world().resource::<bevy::prelude::AssetServer>();
    let result = manifest.load_cached::<bevy::prelude::Image>(&config, "nope", asset_server);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, bevy_cache::CacheError::NotFound(_)),
        "expected NotFound, got {err:?}"
    );
}

#[test]
fn load_cached_returns_not_found_when_file_missing() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = temp_config(&dir);
    let mut manifest = CacheManifest::default();

    manifest
        .store(&config, "vanished", "png", std::io::Cursor::new(b"px"), None)
        .expect("store");

    // Delete the file
    std::fs::remove_file(config.file_path("vanished.png")).expect("delete");

    let mut app = helpers::test_app(dir.path());
    app.update();

    let asset_server = app.world().resource::<bevy::prelude::AssetServer>();
    let result =
        manifest.load_cached::<bevy::prelude::Image>(&config, "vanished", asset_server);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        bevy_cache::CacheError::NotFound(_)
    ));
}

#[test]
fn load_cached_returns_handle_when_file_exists() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = temp_config(&dir);
    let mut manifest = CacheManifest::default();

    manifest
        .store(&config, "tex", "png", std::io::Cursor::new(b"\x89PNG fake"), None)
        .expect("store");

    let mut app = helpers::test_app(dir.path());
    app.update();

    let asset_server = app.world().resource::<bevy::prelude::AssetServer>();
    let result = manifest.load_cached::<bevy::prelude::Image>(&config, "tex", asset_server);
    assert!(result.is_ok(), "expected Ok handle, got {result:?}");
}

// =========================================================================
// Max entries – continued
// =========================================================================

#[test]
fn enforce_max_entries_noop_when_unlimited() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = CacheConfig {
        app_name: "test_app".to_owned(),
        cache_dir: dir.path().to_path_buf(),
        max_age: std::time::Duration::from_secs(999_999),
        max_entries: None,
    };
    let mut manifest = CacheManifest::default();
    manifest
        .store(&config, "a", "bin", std::io::Cursor::new(b"x"), None)
        .expect("store");
    manifest
        .store(&config, "b", "bin", std::io::Cursor::new(b"y"), None)
        .expect("store");

    let evicted = manifest.enforce_max_entries(&config);
    assert_eq!(evicted, 0);
}

// =========================================================================
// Total size
// =========================================================================

#[test]
fn total_size_bytes_sums_entries() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let config = temp_config(&dir);
    let mut manifest = CacheManifest::default();

    assert_eq!(manifest.total_size_bytes(), 0);

    manifest
        .store(&config, "a", "bin", std::io::Cursor::new(b"hello"), None)
        .expect("store a");
    manifest
        .store(&config, "b", "bin", std::io::Cursor::new(b"world!"), None)
        .expect("store b");

    // "hello" = 5 bytes, "world!" = 6 bytes
    assert_eq!(manifest.total_size_bytes(), 11);

    manifest.remove(&config, "a").expect("remove a");
    assert_eq!(manifest.total_size_bytes(), 6);
}
