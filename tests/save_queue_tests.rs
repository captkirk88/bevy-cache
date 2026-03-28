mod helpers;

use bevy::prelude::*;
use bevy_cache::prelude::*;

// =========================================================================
// Test asset types
// =========================================================================

/// A simple reflected + serializable asset for testing `enqueue_reflect`.
#[derive(Asset, Reflect, Clone, Debug, serde::Serialize, serde::Deserialize)]
#[reflect(Serialize, Deserialize)]
struct TestData {
    value: u32,
    label: String,
}

// =========================================================================
// enqueue_bytes round-trip
// =========================================================================

#[test]
fn enqueue_bytes_writes_to_cache() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let (mut app, config) = helpers::test_app_with_manifest(dir.path());

    {
        let mut pending = app
            .world_mut()
            .get_resource_mut::<CacheQueue>()
            .expect("CacheQueue should exist");
        pending.enqueue("blob_key", "bin", std::io::Cursor::new(b"hello cache"), None);
    }

    app.update();

    let manifest = app
        .world()
        .get_resource::<CacheManifest>()
        .expect("CacheManifest should exist");
    assert!(
        manifest.contains("blob_key"),
        "manifest should contain the enqueued key"
    );
    let entry = manifest.get("blob_key").expect("entry should exist");
    assert_eq!(entry.file_name, "blob_key.bin");
    assert_eq!(entry.size_bytes, 11); // b"hello cache".len()

    let fs_path = config.file_path("blob_key.bin");
    let on_disk = std::fs::read(&fs_path).expect("cached file should exist");
    assert_eq!(on_disk, b"hello cache");
}

// =========================================================================
// enqueue_reflect round-trip
// =========================================================================

#[test]
fn enqueue_reflect_serializes_to_ron() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let (mut app, config) = helpers::test_app_with_manifest(dir.path());

    // Register the reflected type.
    app.register_type::<TestData>();

    let data = TestData {
        value: 42,
        label: "test".to_owned(),
    };

    {
        let mut pending = app
            .world_mut()
            .get_resource_mut::<CacheQueue>()
            .expect("CacheQueue should exist");
        pending.enqueue_reflect(Box::new(data), "test_data", "ron", None);
    }

    app.update();

    let manifest = app
        .world()
        .get_resource::<CacheManifest>()
        .expect("CacheManifest should exist");
    assert!(
        manifest.contains("test_data"),
        "manifest should contain the enqueued key"
    );
    let entry = manifest.get("test_data").expect("entry should exist");
    assert_eq!(entry.file_name, "test_data.ron");

    let fs_path = config.file_path("test_data.ron");
    let on_disk = std::fs::read_to_string(&fs_path).expect("cached file should exist");
    // The RON output should contain our values (ReflectSerializer wraps in type map).
    assert!(on_disk.contains("42"), "RON should contain the value 42");
    assert!(on_disk.contains("test"), "RON should contain the label");
}

// =========================================================================
// Asset path is correct after enqueue + process
// =========================================================================

#[test]
fn asset_path_after_enqueue() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let (mut app, _config) = helpers::test_app_with_manifest(dir.path());

    {
        let mut pending = app
            .world_mut()
            .get_resource_mut::<CacheQueue>()
            .expect("pending");
        pending.enqueue("img_key", "png", std::io::Cursor::new(b"fake png data"), None);
    }

    app.update();

    let manifest = app
        .world()
        .get_resource::<CacheManifest>()
        .expect("manifest");
    assert_eq!(
        manifest.asset_path("img_key"),
        Some("cache://img_key.png".to_owned())
    );
}

// =========================================================================
// Pending queue is drained after processing
// =========================================================================

#[test]
fn queue_is_empty_after_successful_process() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let (mut app, _config) = helpers::test_app_with_manifest(dir.path());

    {
        let mut pending = app
            .world_mut()
            .get_resource_mut::<CacheQueue>()
            .expect("pending");
        pending.enqueue("drain_test", "bin", std::io::Cursor::new(b"x"), None);
    }

    app.update();

    let pending = app
        .world()
        .get_resource::<CacheQueue>()
        .expect("pending");
    assert!(
        pending.is_empty(),
        "queue should be empty after all saves processed"
    );
}
