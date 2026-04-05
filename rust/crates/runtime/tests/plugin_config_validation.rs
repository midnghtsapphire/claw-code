//! E6-1: Plugin config validation tests.
//!
//! These tests validate the contract for plugin configuration as defined in
//! `docs/claw-code/PLUGIN_CONFIG_SPEC.md`.  They drive the public API of
//! [`RuntimePluginConfig`] and [`PluginLifecycle::validate_config`] to ensure:
//!
//! 1. **Default state** — a fresh `RuntimePluginConfig` has no enabled entries,
//!    empty directories, and no optional paths.
//! 2. **Enable / disable a plugin** — `set_plugin_state` and `state_for` round-trip
//!    correctly, including the fallback to `default_enabled`.
//! 3. **Multiple plugins** — enabling one plugin does not affect another.
//! 4. **Repeated state updates** — re-calling `set_plugin_state` overwrites the
//!    previous value.
//! 5. **Plugin config loaded from JSON** — the `ConfigLoader` parses the
//!    `plugins` section into the correct `RuntimePluginConfig` fields.
//! 6. **Validation via PluginLifecycle trait** — a mock plugin that accepts the
//!    default config succeeds; one that rejects it returns an `Err` with the
//!    plugin name in the message.
//! 7. **Config with conflicting plugin enable/disable** — project-level disable
//!    wins over a user-level enable when the `ConfigLoader` precedence chain is
//!    applied.
//! 8. **Empty plugins block** — settings file with `"plugins": {}` is valid and
//!    produces defaults.
//! 9. **enabledPlugins key** — the alternative `enabledPlugins` top-level key
//!    is parsed with the same semantics.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use runtime::{
    ConfigLoader, DiscoveryResult, PluginHealthcheck, PluginLifecycle, PluginState,
    ResourceInfo, RuntimePluginConfig, ServerHealth, ServerStatus, ToolInfo,
};

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn temp_dir(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("runtime-plugin-config-{label}-{nanos}"))
}

fn tool(name: &str) -> ToolInfo {
    ToolInfo {
        name: name.to_string(),
        description: Some(format!("{name} tool")),
        input_schema: None,
    }
}

fn resource(name: &str, uri: &str) -> ResourceInfo {
    ResourceInfo {
        uri: uri.to_string(),
        name: name.to_string(),
        description: Some(format!("{name} resource")),
        mime_type: Some("application/json".to_string()),
    }
}

fn healthy_server(name: &str, capabilities: &[&str]) -> ServerHealth {
    ServerHealth {
        server_name: name.to_string(),
        status: ServerStatus::Healthy,
        capabilities: capabilities.iter().map(|c| c.to_string()).collect(),
        last_error: None,
    }
}

// ── Mock plugin implementations ───────────────────────────────────────────────

#[derive(Debug, Clone)]
struct AlwaysValidPlugin {
    name: String,
    servers: Vec<ServerHealth>,
    tools: Vec<ToolInfo>,
}

impl AlwaysValidPlugin {
    fn new(name: &str, servers: Vec<ServerHealth>, tools: Vec<ToolInfo>) -> Self {
        Self {
            name: name.to_string(),
            servers,
            tools,
        }
    }
}

impl PluginLifecycle for AlwaysValidPlugin {
    fn validate_config(&self, _config: &RuntimePluginConfig) -> Result<(), String> {
        Ok(())
    }

    fn healthcheck(&self) -> PluginHealthcheck {
        PluginHealthcheck::new(&self.name, self.servers.clone())
    }

    fn discover(&self) -> DiscoveryResult {
        DiscoveryResult {
            tools: self.tools.clone(),
            resources: Vec::new(),
            partial: false,
        }
    }

    fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct RequiresInstallRootPlugin {
    name: String,
}

impl RequiresInstallRootPlugin {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl PluginLifecycle for RequiresInstallRootPlugin {
    fn validate_config(&self, config: &RuntimePluginConfig) -> Result<(), String> {
        if config.install_root().is_none() {
            return Err(format!(
                "plugin `{}` requires `install_root` to be configured",
                self.name
            ));
        }
        Ok(())
    }

    fn healthcheck(&self) -> PluginHealthcheck {
        PluginHealthcheck::new(&self.name, Vec::new())
    }

    fn discover(&self) -> DiscoveryResult {
        DiscoveryResult {
            tools: Vec::new(),
            resources: Vec::new(),
            partial: false,
        }
    }

    fn shutdown(&mut self) -> Result<(), String> {
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 1. Default state
// ──────────────────────────────────────────────────────────────────────────────

/// A freshly constructed `RuntimePluginConfig` has all-empty fields.
#[test]
fn default_plugin_config_has_no_plugins_or_paths() {
    let config = RuntimePluginConfig::default();

    assert!(
        config.enabled_plugins().is_empty(),
        "default config should have no enabled plugins"
    );
    assert!(
        config.external_directories().is_empty(),
        "default config should have no external directories"
    );
    assert!(config.install_root().is_none(), "install_root should be None");
    assert!(config.registry_path().is_none(), "registry_path should be None");
    assert!(config.bundled_root().is_none(), "bundled_root should be None");
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. Enable / disable a plugin with default fallback
// ──────────────────────────────────────────────────────────────────────────────

/// `set_plugin_state` + `state_for` round-trips correctly; the `default_enabled`
/// fallback applies when the plugin has never been configured.
#[test]
fn plugin_state_round_trips_and_falls_back_to_default() {
    let mut config = RuntimePluginConfig::default();

    // unknown plugin uses the default
    assert!(
        config.state_for("unknown-plugin", true),
        "unknown plugin with default_enabled=true should be enabled"
    );
    assert!(
        !config.state_for("unknown-plugin", false),
        "unknown plugin with default_enabled=false should be disabled"
    );

    // explicitly enable a plugin
    config.set_plugin_state("my-plugin".to_string(), true);
    assert!(config.state_for("my-plugin", false)); // override beats default

    // explicitly disable it
    config.set_plugin_state("my-plugin".to_string(), false);
    assert!(!config.state_for("my-plugin", true)); // override beats default
}

// ──────────────────────────────────────────────────────────────────────────────
// 3. Multiple plugins are independent
// ──────────────────────────────────────────────────────────────────────────────

/// Enabling plugin A does not affect plugin B.
#[test]
fn enabling_one_plugin_does_not_affect_another() {
    let mut config = RuntimePluginConfig::default();
    config.set_plugin_state("plugin-a".to_string(), true);

    assert!(config.state_for("plugin-a", false));
    assert!(
        !config.state_for("plugin-b", false),
        "plugin-b should still use default_enabled=false"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 4. Repeated state updates overwrite
// ──────────────────────────────────────────────────────────────────────────────

/// The last call to `set_plugin_state` wins.
#[test]
fn repeated_set_plugin_state_overwrites_previous_value() {
    let mut config = RuntimePluginConfig::default();

    config.set_plugin_state("plugin-x".to_string(), true);
    config.set_plugin_state("plugin-x".to_string(), false);
    config.set_plugin_state("plugin-x".to_string(), true);

    assert!(
        config.state_for("plugin-x", false),
        "last set_plugin_state (true) should win"
    );
    assert_eq!(config.enabled_plugins().len(), 1);
}

// ──────────────────────────────────────────────────────────────────────────────
// 5. Plugin config loaded from JSON
// ──────────────────────────────────────────────────────────────────────────────

/// The `ConfigLoader` correctly parses the `plugins` section from a settings
/// file into `RuntimePluginConfig`.
#[test]
fn config_loader_parses_plugins_section_from_settings_json() {
    let root = temp_dir("plugins-section");
    let cwd = root.join("project");
    let home = root.join("home").join(".claw");
    fs::create_dir_all(&home).expect("home config dir");
    fs::create_dir_all(&cwd).expect("project dir");

    fs::write(
        home.join("settings.json"),
        r#"{
          "plugins": {
            "enabled": {
              "tool-guard@builtin": true,
              "mcp-bridge@builtin": false
            }
          }
        }"#,
    )
    .expect("write settings");

    let config = ConfigLoader::new(&cwd, &home)
        .load()
        .expect("load config");

    let plugins = config.plugins();
    assert_eq!(plugins.state_for("tool-guard@builtin", false), true);
    assert_eq!(plugins.state_for("mcp-bridge@builtin", true), false);
    assert_eq!(plugins.enabled_plugins().len(), 2);

    fs::remove_dir_all(root).expect("cleanup");
}

// ──────────────────────────────────────────────────────────────────────────────
// 6. Validation via PluginLifecycle trait
// ──────────────────────────────────────────────────────────────────────────────

/// A plugin that accepts any config returns `Ok(())`; one that requires
/// `install_root` returns `Err` when the config omits it.
#[test]
fn plugin_validate_config_ok_when_requirements_met() {
    let plugin = AlwaysValidPlugin::new(
        "my-plugin",
        vec![healthy_server("alpha", &["search"])],
        vec![tool("search")],
    );
    let config = RuntimePluginConfig::default();

    assert_eq!(plugin.validate_config(&config), Ok(()));
}

#[test]
fn plugin_validate_config_err_when_install_root_missing() {
    let plugin = RequiresInstallRootPlugin::new("my-install-plugin");
    let config = RuntimePluginConfig::default(); // install_root is None

    let result = plugin.validate_config(&config);
    assert!(result.is_err(), "validation should fail without install_root");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("my-install-plugin"),
        "error message should name the plugin, got: {msg}"
    );
    assert!(
        msg.contains("install_root"),
        "error message should mention install_root, got: {msg}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 7. Project-level disable overrides user-level enable (config precedence)
// ──────────────────────────────────────────────────────────────────────────────

/// When user settings enable a plugin and project settings disable it,
/// the project-level value takes precedence.
#[test]
fn project_level_plugin_disable_overrides_user_level_enable() {
    let root = temp_dir("plugin-precedence");
    let cwd = root.join("project");
    let home = root.join("home").join(".claw");
    fs::create_dir_all(cwd.join(".claw")).expect("project config dir");
    fs::create_dir_all(&home).expect("home config dir");

    // user enables the plugin
    fs::write(
        home.join("settings.json"),
        r#"{"plugins": {"enabled": {"my-plugin": true}}}"#,
    )
    .expect("write user settings");

    // project disables it
    fs::write(
        cwd.join(".claw").join("settings.json"),
        r#"{"plugins": {"enabled": {"my-plugin": false}}}"#,
    )
    .expect("write project settings");

    let config = ConfigLoader::new(&cwd, &home)
        .load()
        .expect("load config");

    assert_eq!(
        config.plugins().state_for("my-plugin", true),
        false,
        "project-level disable should override user-level enable"
    );

    fs::remove_dir_all(root).expect("cleanup");
}

// ──────────────────────────────────────────────────────────────────────────────
// 8. Empty plugins block is valid
// ──────────────────────────────────────────────────────────────────────────────

/// A settings file with `"plugins": {}` should parse successfully and leave all
/// plugin config at defaults.
#[test]
fn empty_plugins_block_is_valid_and_produces_defaults() {
    let root = temp_dir("empty-plugins");
    let cwd = root.join("project");
    let home = root.join("home").join(".claw");
    fs::create_dir_all(&home).expect("home config dir");
    fs::create_dir_all(&cwd).expect("project dir");

    fs::write(home.join("settings.json"), r#"{"plugins": {}}"#)
        .expect("write settings");

    let config = ConfigLoader::new(&cwd, &home)
        .load()
        .expect("load config");

    assert!(
        config.plugins().enabled_plugins().is_empty(),
        "empty plugins block should yield no enabled entries"
    );

    fs::remove_dir_all(root).expect("cleanup");
}

// ──────────────────────────────────────────────────────────────────────────────
// 9. enabledPlugins top-level key
// ──────────────────────────────────────────────────────────────────────────────

/// The alternative `enabledPlugins` top-level key is parsed with the same
/// semantics as `plugins.enabled`.
#[test]
fn enabled_plugins_top_level_key_is_parsed() {
    let root = temp_dir("enabledPlugins-key");
    let cwd = root.join("project");
    let home = root.join("home").join(".claw");
    fs::create_dir_all(&home).expect("home config dir");
    fs::create_dir_all(&cwd).expect("project dir");

    fs::write(
        home.join("settings.json"),
        r#"{"enabledPlugins": {"side-plugin": true}}"#,
    )
    .expect("write settings");

    let config = ConfigLoader::new(&cwd, &home)
        .load()
        .expect("load config");

    assert_eq!(
        config.plugins().state_for("side-plugin", false),
        true,
        "top-level enabledPlugins key should be parsed"
    );

    fs::remove_dir_all(root).expect("cleanup");
}

// ──────────────────────────────────────────────────────────────────────────────
// 10. Healthy plugin produces correct PluginHealthcheck state
// ──────────────────────────────────────────────────────────────────────────────

/// A plugin with all healthy servers produces a `PluginState::Healthy`
/// healthcheck and a complete, non-partial `DiscoveryResult`.
#[test]
fn healthy_plugin_produces_correct_healthcheck_and_discovery() {
    // given
    let mut plugin = AlwaysValidPlugin::new(
        "healthy-plugin",
        vec![
            healthy_server("alpha", &["search", "read"]),
            healthy_server("beta", &["write"]),
        ],
        vec![tool("search"), tool("read"), tool("write")],
    );
    plugin.tools.push(tool("extra"));
    let mut plugin = AlwaysValidPlugin::new(
        "healthy-plugin",
        vec![
            healthy_server("alpha", &["search", "read"]),
            healthy_server("beta", &["write"]),
        ],
        vec![tool("search"), tool("read"), tool("write")],
    );
    let config = RuntimePluginConfig::default();

    // when
    let validation = plugin.validate_config(&config);
    let healthcheck = plugin.healthcheck();
    let discovery = plugin.discover();

    // then
    assert_eq!(validation, Ok(()));
    assert_eq!(healthcheck.state, PluginState::Healthy);
    assert_eq!(healthcheck.plugin_name, "healthy-plugin");
    assert_eq!(discovery.tools.len(), 3);
    assert!(!discovery.partial);
    assert!(healthcheck.degraded_mode(&discovery).is_none());
}
