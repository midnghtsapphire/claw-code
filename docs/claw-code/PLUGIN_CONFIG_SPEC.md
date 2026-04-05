# Plugin Configuration Validation Spec

**ID:** E6-1  
**Epic:** Epic 6 — MCP & Plugin Lifecycle Maturity  
**Status:** Implemented  
**Validation:** `rust/crates/runtime/tests/plugin_config_validation.rs`

---

## Overview

This document defines the contract for plugin configuration in claw-code.  It is the authoritative reference for all configuration fields consumed by the plugin subsystem, the validation semantics exposed by the `PluginLifecycle` trait, and the precedence rules applied by the `ConfigLoader`.

---

## Configuration Model

Plugin configuration is represented by the `RuntimePluginConfig` struct in `runtime/src/config.rs`.

### Fields

| Field | JSON Key | Type | Default | Description |
|-------|----------|------|---------|-------------|
| `enabled_plugins` | `plugins.enabled` or `enabledPlugins` | `BTreeMap<String, bool>` | empty | Per-plugin enable/disable flags. Plugin IDs are strings in the form `<name>@<namespace>` (e.g. `tool-guard@builtin`). |
| `external_directories` | `plugins.externalDirectories` | `Vec<String>` | empty | Filesystem paths to scan for external plugin installations. |
| `install_root` | `plugins.installRoot` | `Option<String>` | `None` | Root directory where plugins are installed. |
| `registry_path` | `plugins.registryPath` | `Option<String>` | `None` | Path to the plugin registry index file. |
| `bundled_root` | `plugins.bundledRoot` | `Option<String>` | `None` | Root directory for bundled (shipped with claw) plugins. |

### JSON shape

```json
{
  "plugins": {
    "enabled": {
      "tool-guard@builtin": true,
      "mcp-bridge@builtin": false
    },
    "externalDirectories": ["/opt/claw-plugins"],
    "installRoot": "~/.claw/plugins",
    "registryPath": "~/.claw/plugins/registry.json",
    "bundledRoot": "/usr/local/share/claw/plugins"
  }
}
```

The alternative top-level key `enabledPlugins` is also accepted for backward compatibility:

```json
{
  "enabledPlugins": {
    "side-plugin": true
  }
}
```

---

## API Contract

### `RuntimePluginConfig::default()`

Returns a configuration with:
- `enabled_plugins` — empty map
- `external_directories` — empty slice
- `install_root` — `None`
- `registry_path` — `None`
- `bundled_root` — `None`

### `state_for(plugin_id, default_enabled) -> bool`

Returns the effective enabled state of a plugin:
- If the plugin has been explicitly set via `set_plugin_state`, returns that value.
- Otherwise, returns `default_enabled`.

### `set_plugin_state(plugin_id, enabled)`

Writes an enable/disable entry for the given plugin ID.  Calling this multiple times for the same ID overwrites the previous value; the last write wins.

---

## Precedence Rules

Plugin enable/disable state follows the `ConfigLoader` precedence chain:

```
settings.local.json  >  .claw/settings.json (project)  >  ~/.claw/settings.json (user)
```

When the same `plugin_id` appears in both user-level and project-level settings, the project-level value is used.  A local settings file (`settings.local.json`) overrides both.

**Example:**

| File | `my-plugin` |
|------|------------|
| `~/.claw/settings.json` | `true` |
| `.claw/settings.json` | `false` |
| **Resolved** | **`false`** (project wins) |

---

## PluginLifecycle Validation Contract

Every plugin that implements the `PluginLifecycle` trait must implement `validate_config`:

```rust
fn validate_config(&self, config: &RuntimePluginConfig) -> Result<(), String>;
```

**Contract:**
- If the plugin's requirements are met by the supplied `RuntimePluginConfig`, return `Ok(())`.
- If a required field is absent or invalid, return `Err(message)` where `message`:
  - **Must** contain the plugin's identifier (name or ID) so the operator knows which plugin rejected the config.
  - **Should** name the specific missing or invalid field.
  - **Must not** panic.

**Example error message:**
```
plugin `my-install-plugin` requires `install_root` to be configured
```

---

## Validation Test Coverage

The validation contract is exercised in `rust/crates/runtime/tests/plugin_config_validation.rs`:

| Test | What it validates |
|------|------------------|
| `default_plugin_config_has_no_plugins_or_paths` | Default state has empty fields |
| `plugin_state_round_trips_and_falls_back_to_default` | `set_plugin_state` + `state_for` semantics |
| `enabling_one_plugin_does_not_affect_another` | Plugin isolation |
| `repeated_set_plugin_state_overwrites_previous_value` | Last-write-wins |
| `config_loader_parses_plugins_section_from_settings_json` | JSON → struct parsing |
| `plugin_validate_config_ok_when_requirements_met` | Validation success path |
| `plugin_validate_config_err_when_install_root_missing` | Validation failure path with named plugin |
| `project_level_plugin_disable_overrides_user_level_enable` | Precedence chain |
| `empty_plugins_block_is_valid_and_produces_defaults` | Empty config block is valid |
| `enabled_plugins_top_level_key_is_parsed` | Alternative `enabledPlugins` key |
| `healthy_plugin_produces_correct_healthcheck_and_discovery` | End-to-end health + discovery |

---

## Error Handling

| Scenario | Expected Behavior |
|----------|------------------|
| `plugins` key present but empty | Parsed successfully; all fields at default |
| `enabled` value is not a bool | `ConfigError::Parse` with field path in message |
| `enabledPlugins` value is not a map | `ConfigError::Parse` |
| Plugin ID contains `@` | Accepted as-is; no namespace resolution at load time |

---

## Related Modules

- `runtime/src/config.rs` — `RuntimePluginConfig`, `ConfigLoader`
- `runtime/src/plugin_lifecycle.rs` — `PluginLifecycle` trait, `PluginHealthcheck`, `PluginState`
- `runtime/src/mcp_lifecycle_hardened.rs` — `McpLifecycleValidator`, `McpDegradedReport`
- `rust/crates/runtime/tests/plugin_config_validation.rs` — validation tests (this spec)
- `rust/crates/runtime/tests/mcp_degraded_startup.rs` — E4-5 degraded-startup tests
- `rust/crates/runtime/tests/mcp_lifecycle_e2e.rs` — E6-2 end-to-end lifecycle tests
