//! Basic real-world cache flow.
//!
//! On first run this example:
//! - generates a runtime asset and stores it in the cache
//! - copies `img.png` into the cache
//! - loads both cached assets through `AssetServerCacheExt`
//!
//! On later runs, `manifest.is_cached(...)` returns `true` and the example
//! skips regeneration / re-copying and loads the cached versions directly.

use bevy::{
    app::{AppExit, ScheduleRunnerPlugin},
    asset::{io::Reader, AssetLoader, LoadContext},
    image::ImagePlugin,
    prelude::*,
    reflect::TypePath,
    window::{WindowPlugin},
    winit::WinitPlugin,
};
use bevy_cache::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

const APP_NAME: &str = "bevy_cache_runtime_roundtrip";
const GENERATED_KEY: &str = "greetings/runtime_greeting";
const IMAGE_KEY: &str = "cached_logo";

#[derive(Asset, TypePath, Debug, Clone, Serialize, Deserialize)]
struct GreetingAsset {
    message: String,
    created_by: String,
}

#[derive(Default, TypePath)]
struct GreetingAssetLoader;

#[non_exhaustive]
#[derive(Debug, Error)]
enum GreetingAssetLoaderError {
    #[error("Could not load asset: {0}")]
    Io(#[from] std::io::Error),
    #[error("Could not parse RON: {0}")]
    Ron(#[from] ron::error::SpannedError),
}

impl AssetLoader for GreetingAssetLoader {
    type Asset = GreetingAsset;
    type Settings = ();
    type Error = GreetingAssetLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(ron::de::from_bytes::<GreetingAsset>(&bytes)?)
    }

    fn extensions(&self) -> &[&str] {
        &["greet"]
    }
}

#[derive(Resource)]
struct CachedAssets {
    greeting: Handle<GreetingAsset>,
    image: Handle<Image>,
}

#[derive(Resource, Default)]
struct PollFrames(u32);

fn main() -> AppExit {
    let mut app = App::new();

    app.add_plugins(BevyCachePlugin::new(APP_NAME));
    app.add_plugins(
        DefaultPlugins
            .set(ImagePlugin::default())
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            })
            .disable::<WinitPlugin>(),
    );
    app.add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0)));
    app.init_asset::<GreetingAsset>()
        .init_asset_loader::<GreetingAssetLoader>()
        .init_resource::<PollFrames>()
        .add_systems(PostStartup, cache_or_load_assets)
        .add_systems(Update, wait_for_cached_assets);

    app.run()
}

fn cache_or_load_assets(
    mut commands: Commands,
    mut cache: Cache,
    asset_server: Res<AssetServer>,
) {
    if cache.is_cached(GENERATED_KEY) {
        info!("Dynamic asset cache hit: loading cached greeting asset");
    } else {
        let runtime_asset = GreetingAsset {
            message: "hello from a runtime-generated cache entry".to_owned(),
            created_by: "runtime_roundtrip example".to_owned(),
        };

        let bytes = ron::ser::to_string_pretty(&runtime_asset, ron::ser::PrettyConfig::default())
            .expect("failed to serialize runtime-generated asset");

        cache
            .store(
                GENERATED_KEY,
                "greet",
                std::io::Cursor::new(bytes),
                Some(Duration::from_secs(3600)),
            )
            .expect("failed to cache generated greeting asset");

        info!("Dynamic asset cache miss: generated and cached greeting asset");
    }

    if cache.is_cached(IMAGE_KEY) {
        info!("Image asset cache hit: loading cached img.png copy");
    } else {
        let image_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("img.png");

        cache
            .store(IMAGE_KEY, "png", std::fs::File::open(&image_path).expect("failed to open img.png"), None)
            .expect("failed to cache img.png");

        info!(
            "Image asset cache miss: copied {} into the cache",
            image_path.display()
        );
    }

    let greeting = cache
        .load_cached::<GreetingAsset>(&asset_server, GENERATED_KEY)
        .expect("failed to create cached greeting handle");
    let image = cache
        .load_cached::<Image>(&asset_server, IMAGE_KEY)
        .expect("failed to create cached image handle");

    commands.insert_resource(CachedAssets { greeting, image });

    info!("Cache path: {}", cache.cache_dir().display());
}

fn wait_for_cached_assets(
    mut frames: ResMut<PollFrames>,
    cached: Res<CachedAssets>,
    greeting_assets: Res<Assets<GreetingAsset>>,
    images: Res<Assets<Image>>,
    mut app_exit: MessageWriter<AppExit>,
) {
    let greeting = greeting_assets.get(&cached.greeting);
    let image = images.get(&cached.image);

    if let (Some(greeting), Some(image)) = (greeting, image) {
        info!("Loaded cached greeting: {}", greeting.message);
        info!("Greeting creator: {}", greeting.created_by);
        info!(
            "Loaded cached image dimensions: {}x{}",
            image.width(),
            image.height()
        );
        app_exit.write(AppExit::Success);
        return;
    }

    frames.0 += 1;
    assert!(frames.0 < 600, "timed out waiting for cached assets to load");
}