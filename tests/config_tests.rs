mod helpers;

use bevy_cache::prelude::*;
use std::path::PathBuf;

// =========================================================================
// CacheConfig
// =========================================================================

#[test]
fn config_default_creates_valid_dir() {
    let config = CacheConfig::default();
    assert!(
        !config.cache_dir.as_os_str().is_empty(),
        "default cache_dir must not be empty"
    );
    assert_eq!(config.max_age, std::time::Duration::from_secs(604_800));
    assert!(config.max_entries.is_none());
}

#[test]
fn config_new_joins_app_name() {
    let config = CacheConfig::new("my_test_app");
    let dir_str = config
        .cache_dir
        .to_str()
        .expect("cache_dir should be valid UTF-8");
    assert!(
        dir_str.contains("my_test_app"),
        "cache_dir should contain the app name, got: {dir_str}"
    );
}

#[test]
fn config_file_path_joins_filename() {
    let config = CacheConfig {
        app_name: "test_app".to_owned(),
        cache_dir: PathBuf::from("/tmp/test_cache"),
        max_age: std::time::Duration::from_secs(60),
        max_entries: None,
    };
    let path = config.file_path("scene.png");
    assert_eq!(path, PathBuf::from("/tmp/test_cache/scene.png"));
}

#[test]
fn config_manifest_fs_path() {
    let config = CacheConfig {
        app_name: "test_app".to_owned(),
        cache_dir: PathBuf::from("/tmp/test_cache"),
        max_age: std::time::Duration::from_secs(60),
        max_entries: None,
    };
    assert_eq!(
        config.manifest_fs_path(),
        PathBuf::from("/tmp/test_cache/test_app.cache_manifest")
    );
}

// =========================================================================
// Key validation
// =========================================================================

#[test]
fn validate_key_accepts_simple_names() {
    let config = CacheConfig::new("test_app");
    config.validate_key("hello").expect("simple key should be valid");
    config.validate_key("scene_01").expect("underscore key should be valid");
    config.validate_key("a-b-c").expect("dash key should be valid");
    config.validate_key("123").expect("numeric key should be valid");
}

#[test]
fn validate_key_accepts_forward_slash_subpaths() {
    let config = CacheConfig::new("test_app");
    config
        .validate_key("icons/ui/scene_01")
        .expect("forward-slash subpaths should be valid");
}

#[test]
fn validate_key_rejects_empty() {
    let config = CacheConfig::new("test_app");
    assert!(matches!(
        config.validate_key(""),
        Err(CacheError::InvalidKey(_))
    ));
}

#[test]
fn validate_key_rejects_backslashes() {
    let config = CacheConfig::new("test_app");
    assert!(matches!(
        config.validate_key("a\\b"),
        Err(CacheError::InvalidKey(_))
    ));
}

#[test]
fn validate_key_rejects_empty_path_segments() {
    let config = CacheConfig::new("test_app");
    assert!(matches!(
        config.validate_key("a//b"),
        Err(CacheError::InvalidKey(_))
    ));
    assert!(matches!(
        config.validate_key("/a"),
        Err(CacheError::InvalidKey(_))
    ));
    assert!(matches!(
        config.validate_key("a/"),
        Err(CacheError::InvalidKey(_))
    ));
}

#[test]
fn validate_key_rejects_dot_dot() {
    let config = CacheConfig::new("test_app");
    assert!(matches!(
        config.validate_key(".."),
        Err(CacheError::InvalidKey(_))
    ));
    assert!(matches!(
        config.validate_key("a/../b"),
        Err(CacheError::InvalidKey(_))
    ));
}

#[test]
fn validate_key_rejects_null_byte() {
    let config = CacheConfig::new("test_app");
    assert!(matches!(
        config.validate_key("a\0b"),
        Err(CacheError::InvalidKey(_))
    ));
}

#[test]
fn validate_key_rejects_manifest_ron() {
    let config = CacheConfig::new("test_app");
    assert!(matches!(
        config.validate_key("test_app.cache_manifest"),
        Err(CacheError::InvalidKey(_))
    ));
}
