# Changelog

All notable changes to claw-code will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- `rust/crates/runtime/tests/mcp_degraded_startup.rs` — 7 integration tests for MCP degraded-startup path: partial/all-server failure, timeout recoverability, JSON round-trip, deduplication (E4-5)
- `rust/crates/runtime/tests/mcp_lifecycle_e2e.rs` — 8 end-to-end MCP lifecycle tests: full happy path, tool-only discovery, repeated invocation cycle, non-recoverable failure forcing shutdown, recoverable timeout, invalid start phase, plugin-healthcheck wiring, timestamp monotonicity (E6-2)
- `rust/crates/runtime/tests/plugin_config_validation.rs` — 11 plugin config validation tests covering default state, enable/disable API, JSON parsing, `PluginLifecycle::validate_config` trait contract, config precedence chain, and PluginHealthcheck wiring (E6-1)
- `docs/claw-code/PLUGIN_CONFIG_SPEC.md` — authoritative plugin configuration contract spec: field reference, JSON shape, precedence rules, `PluginLifecycle` validation contract, test coverage table (E6-1)
- `tests/test_dispatch.py` — 74 unit tests for tool dispatch, command dispatch, `ToolPermissionContext` filtering, and `ExecutionRegistry` wiring (E4-4)
- `docs/claw-code/CONTAINER_WORKFLOW.md` — Docker/Podman commands for build, test, and run including cross-compilation, volume mounting, and troubleshooting (E5-2)
- `docs/claw-code/` directory with comprehensive project documentation:
  - `REPOSITORY_OVERVIEW.md` — full repository explanation, layout, tech stack, and module inventory
  - `STANDARDS_REVIEW.md` — side-by-side audit of revvel-standards vs. claw-code
  - `SCRUM_PROJECT_PLAN.md` — 7-sprint scrum plan using EXRUP methodology
  - `CHANGE_PROPOSAL.md` — prioritized list of changes and additions before any code is touched
  - `STANDARDS_FORTIFICATION.md` — 20 proposals to make revvel-standards more robust
  - `BLUE_OCEAN_OPPORTUNITIES.md` — 6 blue ocean opportunity analyses
  - `TEST_COVERAGE_PLAN.md` — coverage gap analysis and improvement roadmap
- `tests/test_models.py` — 20 unit tests for `src/models.py` dataclasses (PortingModule, Subsystem, UsageSummary, PermissionDenial, PortingBacklog)
- `tests/test_session_store.py` — 14 unit tests for `src/session_store.py` (save/load roundtrip, edge cases, large token counts, nested directory creation)
- `tests/test_tool_coverage.py` — 23 unit tests for `src/tools.py`, `src/commands.py`, and `src/execution_registry.py` (tool surface structure, command registry, known-tool presence)

### Changed
- Total Python test count increased from 22 to 79 (all passing)

---

## [0.1.0] — 2026-04-03

### Added
- 9-lane Rust workspace reaching parity checkpoint (bash validation, CI fix, file-tool edge cases, TaskRegistry, task wiring, Team+Cron, MCP lifecycle, LSP client, Permission enforcement)
- 40 exposed tool specs in `tools/src/lib.rs`
- Worker state machine lifecycle (`worker_boot.rs`)
- Typed lane event schema (`lane_events.rs`)
- Failure taxonomy and recovery recipes (`recovery_recipes.rs`)
- Stale-branch detection (`stale_branch.rs`)
- Green-ness contract levels (`green_contract.rs`)
- Typed task packet format (`task_packet.rs`)
- Policy engine for autonomous coding rules (`policy_engine.rs`)
- MCP degraded-startup reporting (`mcp_lifecycle_hardened.rs`)
- Permission enforcement layer (`permission_enforcer.rs`)
- Python porting workspace with 22 integration tests
- Mock Anthropic service for deterministic parity testing
- 12 cross-module integration tests in `runtime/tests/`

### Infrastructure
- GitHub Actions CI: `cargo fmt --all --check` + `cargo test -p rusty-claude-cli`
- 9-lane parity documented in `PARITY.md`
- ROADMAP.md with 5-phase roadmap and P0–P3 backlog
