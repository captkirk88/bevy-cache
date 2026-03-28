use bevy::prelude::*;
use bevy::reflect::serde::ReflectSerializer;

use std::time::Duration;

use crate::config::CacheConfig;
use crate::error::CacheError;
use crate::manifest::CacheManifest;

/// A pending save entry — either reflected data awaiting serialization,
/// or a reader ready to write.
pub(crate) enum PendingPayload {
    /// A reflected value to be serialized via [`ReflectSerializer`] → RON.
    Reflect {
        value: Box<dyn Reflect>,
        extension: String,
    },
    /// Pre-serialized data provided as a [`Read`]er.
    Bytes {
        reader: Box<dyn std::io::Read + Send + Sync + 'static>,
        extension: String,
    },
}

pub(crate) struct PendingSaveEntry {
    pub key: String,
    pub max_age: Option<Duration>,
    pub payload: PendingPayload,
}

/// Resource holding queued asset-save requests.
///
/// Assets can be enqueued in two ways:
///
/// 1. [`enqueue_reflect`](Self::enqueue_reflect) — pass any value that
///    implements [`Reflect`]. The value will be serialized to RON via
///    Bevy's [`ReflectSerializer`] using the [`AppTypeRegistry`].
///    The type **must** be registered in the type registry (via
///    `app.register_type::<T>()`) and should have `ReflectSerialize`
///    type data (derived automatically for types that implement both
///    `Reflect` and `serde::Serialize`).
///
/// 2. [`enqueue`](Self::enqueue) — pass any [`Read`]er containing
///    pre-serialized data and a file extension. No reflection or registry
///    required. Use [`std::io::Cursor`] in tests or for in-memory data.
///
/// Enqueued entries are processed by [`process_pending_saves`] which runs
/// each frame in `PostUpdate`.
#[derive(Resource, Default)]
pub struct CacheQueue {
    pub(crate) queue: Vec<PendingSaveEntry>,
}

impl CacheQueue {
    /// Returns `true` when there are no pending save requests.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Returns the number of pending save requests.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Enqueue a reflected value for caching.
    ///
    /// The value will be serialized to RON using Bevy's
    /// [`ReflectSerializer`] during [`process_pending_saves`].
    /// The type must be registered in the [`AppTypeRegistry`].
    ///
    /// # Example
    /// ```rust,ignore
    /// #[derive(Asset, TypePath, Reflect, serde::Serialize)]
    /// #[reflect(Serialize)]
    /// struct LevelData { tiles: Vec<u32> }
    ///
    /// fn cache_level(
    ///     mut pending: ResMut<CacheQueue>,
    ///     assets: Res<Assets<LevelData>>,
    ///     handle: Res<MyLevelHandle>,
    /// ) {
    ///     if let Some(level) = assets.get(&handle.0) {
    ///         pending.enqueue_reflect(
    ///             Box::new(level.clone()),
    ///             "level_01",
    ///             "ron",
    ///             None,
    ///         );
    ///     }
    /// }
    /// ```
    pub fn enqueue_reflect(
        &mut self,
        value: Box<dyn Reflect>,
        key: impl Into<String>,
        extension: impl Into<String>,
        max_age: Option<Duration>,
    ) {
        self.queue.push(PendingSaveEntry {
            key: key.into(),
            max_age,
            payload: PendingPayload::Reflect {
                value,
                extension: extension.into(),
            },
        });
    }

    /// Enqueue a [`Read`]er for caching.
    ///
    /// The caller is responsible for serialization; the data is streamed
    /// as-is to `{key}.{extension}` in the cache directory without buffering
    /// the entire payload in memory.
    ///
    /// Use [`std::io::Cursor`] to wrap in-memory buffers:
    ///
    /// # Example
    /// ```rust,ignore
    /// fn cache_screenshot(mut pending: ResMut<CacheQueue>) {
    ///     let png_bytes: Vec<u8> = capture_screenshot();
    ///     pending.enqueue("scene_01", "png", std::io::Cursor::new(png_bytes), None);
    /// }
    /// ```
    pub fn enqueue<R: std::io::Read + Send + Sync + 'static>(
        &mut self,
        key: impl Into<String>,
        extension: impl Into<String>,
        reader: R,
        max_age: Option<Duration>,
    ) {
        self.queue.push(PendingSaveEntry {
            key: key.into(),
            max_age,
            payload: PendingPayload::Bytes {
                reader: Box::new(reader),
                extension: extension.into(),
            },
        });
    }
}

/// Serialize a reflected value to RON bytes using the type registry.
fn serialize_reflect(
    value: &dyn Reflect,
    registry: &AppTypeRegistry,
) -> Result<Vec<u8>, CacheError> {
    let registry = registry.read();
    let serializer = ReflectSerializer::new(value.as_partial_reflect(), &registry);
    let pretty = ron::ser::PrettyConfig::default();
    let serialized =
        ron::ser::to_string_pretty(&serializer, pretty).map_err(CacheError::RonSerialize)?;
    Ok(serialized.into_bytes())
}

/// System that processes the pending save queue each frame.
/// Reflected values are serialized via [`ReflectSerializer`], then all
/// entries are written to the cache directory through [`CacheManifest::store`].
pub fn process_pending_saves(world: &mut World) {
    let queue = {
        let mut pending = world
            .get_resource_mut::<CacheQueue>()
            .expect("CacheQueue resource should exist");
        std::mem::take(&mut pending.queue)
    };

    if queue.is_empty() {
        return;
    }

    let config = world
        .get_resource::<CacheConfig>()
        .expect("CacheConfig resource should exist")
        .clone();

    let registry = world
        .get_resource::<AppTypeRegistry>()
        .expect("AppTypeRegistry resource should exist")
        .clone();

    for entry in queue {
        let PendingSaveEntry { key, max_age, payload } = entry;
        let result = match payload {
            PendingPayload::Reflect { value, extension } => {
                serialize_reflect(value.as_ref(), &registry)
                    .and_then(|bytes| {
                        let mut manifest = world
                            .get_resource_mut::<CacheManifest>()
                            .expect("CacheManifest resource should exist");
                        manifest.store(
                            &config,
                            &key,
                            &extension,
                            std::io::Cursor::new(bytes),
                            max_age,
                        )
                    })
            }
            PendingPayload::Bytes { mut reader, extension } => {
                let mut manifest = world
                    .get_resource_mut::<CacheManifest>()
                    .expect("CacheManifest resource should exist");
                manifest.store(
                    &config,
                    &key,
                    &extension,
                    &mut *reader,
                    max_age,
                )
            }
        };

        match result {
            Ok(()) => {
                tracing::debug!("Cached asset '{key}'");
            }
            Err(e) => {
                tracing::error!("Failed to cache asset '{key}': {e}");
            }
        }
    }
}
