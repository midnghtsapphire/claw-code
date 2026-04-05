//! E4-2: Config precedence edge-case tests.
//!
//! Covers three important edge cases that the in-crate unit tests don't
//! fully exercise as explicit regression targets:
//!
//! 1. **Malformed JSON** — a settings file with invalid JSON should return a
//!    `ConfigError::Parse` (not panic), and the error message should identify
//!    the offending file.
//!
//! 2. **Missing / empty required keys** — a config that omits optional keys
//!    should load successfully and return `None` / empty defaults for every
//!    field, not a hard error.
//!
//! 3. **permissionMode precedence: project wins over user** — when both a
//!    user-level settings file and a project-level settings file specify
//!    `permissionMode`, the project-level value should take precedence.
//!    A local settings file beats both.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use runtime::{ConfigLoader, ResolvedPermissionMode};

fn temp_dir(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("runtime-config-precedence-{label}-{nanos}"))
}

// ──────────────────────────────────────────────
// 1. Malformed JSON
// ──────────────────────────────────────────────

/// A settings.json with invalid JSON should return `ConfigError::Parse`.
/// It must NOT panic, and the error message must include the file path so the
/// user knows which file to fix.
#[test]
fn malformed_json_in_user_settings_returns_parse_error_not_panic() {
    let root = temp_dir("malformed-user");
    let cwd = root.join("project");
    let home = root.join("home").join(".claw");
    fs::create_dir_all(&home).expect("home config dir");
    fs::create_dir_all(&cwd).expect("project dir");

    let settings_path = home.join("settings.json");
    fs::write(&settings_path, r#"{ "model": "haiku", INVALID }"#)
        .expect("write malformed settings");

    let error = ConfigLoader::new(&cwd, &home)
        .load()
        .expect_err("malformed JSON should produce an error");

    let message = error.to_string();
    assert!(
        message.contains(settings_path.to_str().expect("path is utf-8")),
        "error message should contain the offending file path, got: {message}"
    );

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

/// A project settings.json with invalid JSON should return `ConfigError::Parse`
/// and identify the project config path in the error.
#[test]
fn malformed_json_in_project_settings_returns_parse_error_not_panic() {
    let root = temp_dir("malformed-project");
    let cwd = root.join("project");
    let home = root.join("home").join(".claw");
    fs::create_dir_all(cwd.join(".claw")).expect("project config dir");
    fs::create_dir_all(&home).expect("home config dir");

    let settings_path = cwd.join(".claw").join("settings.json");
    fs::write(&settings_path, r#"{"model": }"#).expect("write malformed project settings");

    let error = ConfigLoader::new(&cwd, &home)
        .load()
        .expect_err("malformed JSON should produce an error");

    let message = error.to_string();
    assert!(
        message.contains(settings_path.to_str().expect("path is utf-8")),
        "error message should contain the offending file path, got: {message}"
    );

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

/// The legacy `.claw.json` compat file silently skips malformed JSON instead of
/// erroring out, to stay backward-compatible with users who may have stale files.
#[test]
fn malformed_legacy_claw_json_is_skipped_not_an_error() {
    let root = temp_dir("malformed-legacy");
    let cwd = root.join("project");
    let home = root.join("home").join(".claw");
    fs::create_dir_all(&home).expect("home config dir");
    fs::create_dir_all(&cwd).expect("project dir");

    // Write malformed JSON into the legacy compat file location
    fs::write(
        home.parent().expect("home parent").join(".claw.json"),
        r#"not valid json at all !!!"#,
    )
    .expect("write malformed legacy config");

    // Should load successfully (legacy file is skipped, not an error)
    let loaded = ConfigLoader::new(&cwd, &home)
        .load()
        .expect("malformed legacy .claw.json should be silently skipped");

    assert_eq!(
        loaded.loaded_entries().len(),
        0,
        "no entries should be loaded when only the malformed legacy file exists"
    );

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

// ──────────────────────────────────────────────
// 2. Missing / empty required keys → defaults
// ──────────────────────────────────────────────

/// A config file that only sets unrelated keys should not produce an error, and
/// optional fields like `model` and `permissionMode` should return `None`.
#[test]
fn config_with_missing_optional_keys_uses_defaults() {
    let root = temp_dir("missing-keys");
    let cwd = root.join("project");
    let home = root.join("home").join(".claw");
    fs::create_dir_all(cwd.join(".claw")).expect("project config dir");
    fs::create_dir_all(&home).expect("home config dir");

    // Only set an env key — no model, no permissionMode, no hooks, no mcp
    fs::write(
        cwd.join(".claw").join("settings.json"),
        r#"{"env":{"MY_VAR":"1"}}"#,
    )
    .expect("write minimal settings");

    let loaded = ConfigLoader::new(&cwd, &home)
        .load()
        .expect("minimal config should load");

    assert_eq!(loaded.model(), None, "model should default to None");
    assert_eq!(
        loaded.permission_mode(),
        None,
        "permissionMode should default to None"
    );
    assert!(
        loaded.hooks().pre_tool_use().is_empty(),
        "pre_tool_use hooks should default to empty"
    );
    assert!(
        loaded.mcp().servers().is_empty(),
        "MCP servers should default to empty"
    );

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

/// An entirely empty config directory (no files at all) should load
/// successfully with all-default values and zero loaded entries.
#[test]
fn empty_config_directory_loads_with_all_defaults() {
    let root = temp_dir("empty-config");
    let cwd = root.join("project");
    let home = root.join("home").join(".claw");
    fs::create_dir_all(&home).expect("home config dir");
    fs::create_dir_all(&cwd).expect("project dir");
    // No settings files written

    let loaded = ConfigLoader::new(&cwd, &home)
        .load()
        .expect("empty config should load");

    assert_eq!(loaded.loaded_entries().len(), 0);
    assert_eq!(loaded.model(), None);
    assert_eq!(loaded.permission_mode(), None);
    assert!(loaded.hooks().pre_tool_use().is_empty());
    assert!(loaded.mcp().servers().is_empty());

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

// ──────────────────────────────────────────────
// 3. permissionMode precedence
// ──────────────────────────────────────────────

/// When the user settings file sets `permissionMode` and the project settings
/// file sets a *different* `permissionMode`, the project value must win
/// (project overrides user because it is merged later).
#[test]
fn project_permission_mode_overrides_user_permission_mode() {
    let root = temp_dir("project-wins");
    let cwd = root.join("project");
    let home = root.join("home").join(".claw");
    fs::create_dir_all(cwd.join(".claw")).expect("project config dir");
    fs::create_dir_all(&home).expect("home config dir");

    // User sets read-only (plan)
    fs::write(
        home.join("settings.json"),
        r#"{"permissionMode":"plan"}"#,
    )
    .expect("write user settings");

    // Project sets workspace write (acceptEdits)
    fs::write(
        cwd.join(".claw").join("settings.json"),
        r#"{"permissionMode":"acceptEdits"}"#,
    )
    .expect("write project settings");

    let loaded = ConfigLoader::new(&cwd, &home)
        .load()
        .expect("config should load");

    assert_eq!(
        loaded.permission_mode(),
        Some(ResolvedPermissionMode::WorkspaceWrite),
        "project permissionMode (acceptEdits) should override user permissionMode (plan)"
    );

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

/// When only the user settings file sets `permissionMode` and the project
/// settings file does NOT set it, the user value should be used (not dropped).
#[test]
fn user_permission_mode_is_used_when_project_does_not_override() {
    let root = temp_dir("user-fallback");
    let cwd = root.join("project");
    let home = root.join("home").join(".claw");
    fs::create_dir_all(cwd.join(".claw")).expect("project config dir");
    fs::create_dir_all(&home).expect("home config dir");

    // User sets read-only (plan)
    fs::write(
        home.join("settings.json"),
        r#"{"permissionMode":"plan"}"#,
    )
    .expect("write user settings");

    // Project sets no permissionMode
    fs::write(
        cwd.join(".claw").join("settings.json"),
        r#"{"env":{"X":"1"}}"#,
    )
    .expect("write project settings without permissionMode");

    let loaded = ConfigLoader::new(&cwd, &home)
        .load()
        .expect("config should load");

    assert_eq!(
        loaded.permission_mode(),
        Some(ResolvedPermissionMode::ReadOnly),
        "user permissionMode should be used when project does not override it"
    );

    fs::remove_dir_all(root).expect("cleanup temp dir");
}

/// The local settings file (settings.local.json) must take precedence over
/// both user and project-level `permissionMode`.
#[test]
fn local_settings_permission_mode_beats_user_and_project() {
    let root = temp_dir("local-wins");
    let cwd = root.join("project");
    let home = root.join("home").join(".claw");
    fs::create_dir_all(cwd.join(".claw")).expect("project config dir");
    fs::create_dir_all(&home).expect("home config dir");

    // User: read-only
    fs::write(
        home.join("settings.json"),
        r#"{"permissionMode":"plan"}"#,
    )
    .expect("write user settings");

    // Project: workspace write
    fs::write(
        cwd.join(".claw").join("settings.json"),
        r#"{"permissionMode":"acceptEdits"}"#,
    )
    .expect("write project settings");

    // Local: full access (dontAsk)
    fs::write(
        cwd.join(".claw").join("settings.local.json"),
        r#"{"permissionMode":"dontAsk"}"#,
    )
    .expect("write local settings");

    let loaded = ConfigLoader::new(&cwd, &home)
        .load()
        .expect("config should load");

    assert_eq!(
        loaded.permission_mode(),
        Some(ResolvedPermissionMode::DangerFullAccess),
        "local settings.local.json should beat user and project permissionMode"
    );

    fs::remove_dir_all(root).expect("cleanup temp dir");
}
