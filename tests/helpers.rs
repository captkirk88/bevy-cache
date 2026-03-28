use bevy::prelude::*;
use bevy_cache::{BevyCachePlugin, CacheConfig};
use std::path::Path;

/// Create a [`BevyCachePlugin`] backed by the given temp directory.
/// The returned [`App`] has `MinimalPlugins` + `AssetPlugin` + `BevyCachePlugin`
/// and is ready for tests. Call `app.update()` at least once to run `Startup`
/// systems (which loads the manifest).
#[allow(dead_code)]
pub fn test_app(cache_dir: &Path) -> App {
    let config = CacheConfig {
        app_name: "test_app".to_owned(),
        cache_dir: cache_dir.to_path_buf(),
        max_age: std::time::Duration::from_secs(604_800),
        max_entries: None,
    };

    let plugin = BevyCachePlugin { config };

    let mut app = App::new();
    app.add_plugins(plugin);
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    app.init_asset::<Image>();
    app
}

/// Convenience: build a test app and run one update so `Startup` systems
/// execute, then return (app, config).
#[allow(dead_code)]
pub fn test_app_with_manifest(cache_dir: &Path) -> (App, CacheConfig) {
    let mut app = test_app(cache_dir);
    app.update();

    let config = app
        .world()
        .get_resource::<CacheConfig>()
        .expect("CacheConfig resource should exist after plugin init")
        .clone();

    (app, config)
}
