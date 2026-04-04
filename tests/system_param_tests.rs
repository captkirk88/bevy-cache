mod helpers;

use bevy::prelude::*;
use bevy_cache::prelude::*;

#[derive(Resource)]
struct CacheProbe {
    app_name: String,
    contains: bool,
    path: Option<String>,
    size_bytes: u64,
    file_exists: bool,
    load_exists: bool,
    load_cached_ok: bool,
    entry_file_name: Option<String>,
    manifest_len: usize,
}

fn exercise_cache_system(
    mut commands: Commands,
    mut cache: Cache,
    asset_server: Res<AssetServer>,
) {
    cache
        .store(
            "nested/ui/icon",
            "png",
            std::io::Cursor::new(b"\x89PNG fake"),
            None,
        )
        .expect("cache store should succeed");

    let entry = cache.get("nested/ui/icon");
    let path = cache.asset_path("nested/ui/icon");
    let unchecked_handle = cache.load::<Image>(&asset_server, "nested/ui/icon");
    let checked_handle = cache.load_cached::<Image>(&asset_server, "nested/ui/icon");
    let file_exists = cache.config().file_path("nested/ui/icon.png").exists();

    commands.insert_resource(CacheProbe {
        app_name: cache.config().app_name.clone(),
        contains: cache.contains("nested/ui/icon"),
        path,
        size_bytes: cache.total_size_bytes(),
        file_exists,
        load_exists: unchecked_handle.is_some(),
        load_cached_ok: checked_handle.is_ok(),
        entry_file_name: entry.map(|entry| entry.file_name.clone()),
        manifest_len: cache.manifest().entries.len(),
    });
}

#[test]
fn cache_system_param_wraps_manifest_and_config() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let (mut app, _) = helpers::test_app_with_manifest(dir.path());

    app.add_systems(Update, exercise_cache_system);
    app.update();

    let probe = app
        .world()
        .get_resource::<CacheProbe>()
        .expect("CacheProbe should be inserted by the test system");

    assert_eq!(probe.app_name, "test_app");
    assert!(probe.contains, "cache should report the stored key");
    assert_eq!(
        probe.path.as_deref(),
        Some("cache://nested/ui/icon.png")
    );
    assert_eq!(probe.size_bytes, 9);
    assert!(probe.file_exists, "stored cache file should exist on disk");
    assert!(probe.load_exists, "asset path loading should return a handle");
    assert!(probe.load_cached_ok, "checked cache loading should succeed");
    assert_eq!(
        probe.entry_file_name.as_deref(),
        Some("nested/ui/icon.png")
    );
    assert_eq!(probe.manifest_len, 1);
}

#[cfg(feature = "hot_reload")]
#[derive(Resource)]
struct TestImageHandle(Handle<Image>);

#[cfg(feature = "hot_reload")]
#[derive(Resource)]
struct HotReloadProbe(bool);

#[cfg(feature = "hot_reload")]
fn exercise_hot_reload_helper(
    mut commands: Commands,
    cache: Cache,
    mut events: MessageReader<AssetEvent<Image>>,
    assets: Res<Assets<Image>>,
    handle: Res<TestImageHandle>,
) {
    let found = cache
        .get_if_modified(&handle.0, assets.as_ref(), &mut events)
        .is_some();
    commands.insert_resource(HotReloadProbe(found));
}

#[cfg(feature = "hot_reload")]
#[test]
fn cache_hot_reload_helper_filters_modified_events() {
    let dir = tempfile::tempdir().expect("failed to create tempdir");
    let (mut app, _) = helpers::test_app_with_manifest(dir.path());

    let handle = {
        let mut assets = app.world_mut().resource_mut::<Assets<Image>>();
        assets.add(Image::default())
    };

    app.insert_resource(TestImageHandle(handle.clone()));
    app.add_systems(Update, exercise_hot_reload_helper);

    app.world_mut()
        .resource_mut::<Messages<AssetEvent<Image>>>()
        .write(AssetEvent::Modified { id: handle.id() });

    app.update();

    let probe = app
        .world()
        .get_resource::<HotReloadProbe>()
        .expect("HotReloadProbe should be inserted by the test system");
    assert!(probe.0, "modified event should resolve to the matching asset");
}