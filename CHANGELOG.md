# Changelog

All notable changes to claw-code will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- `rust/crates/runtime/src/swarm_state.rs` — `SwarmStateStore`: atomic save/load of combined `BranchLockRegistry` + `CommitProvenanceRegistry` state to `.claw/swarm-state.json`; `BranchLockRegistry::snapshot()`/`restore_from_snapshot()` (E8-1)
- `rust/crates/runtime/src/commit_provenance.rs` — `parse_git_push_output`, `record_push_from_git_output`, `GitPushRefUpdate`: parse `git push` stderr ref-update lines and auto-populate provenance registry; forced-push detects superseded SHA (E8-2)
- `rust/crates/runtime/tests/swarm_e2e.rs` — 12 end-to-end swarm simulation tests: two workers competing for one branch (collision + provenance), three-worker race, independent branches, save/restore round-trips, lineage preservation, forced-push superseded-by, git push output integration (E8-3)

### Fixed
- `rust/crates/runtime/src/*` — resolved all clippy warnings in runtime lib: removed unused imports (`session_control.rs`), inlined format strings (`mcp_tool_bridge.rs`, `permission_enforcer.rs`, `recovery_recipes.rs`), added `#[must_use]` to 25 public API methods across `branch_lock.rs`, `lsp_client.rs`, `mcp_tool_bridge.rs`, `permission_enforcer.rs`, `stale_branch.rs`, `task_registry.rs`, `team_cron_registry.rs`, suppressed structural lints with `#[allow]` where a fix would require significant refactoring, fixed `clone_from` in `commit_provenance.rs`, `is_none_or` in `task_registry.rs`, `map_or` in `mcp_lifecycle_hardened.rs`, unnested or-pattern in `mcp_tool_bridge.rs`, `?`-operator in `worker_boot.rs`, redundant closure in `worker_boot.rs` (E8-4)

- `rust/crates/runtime/src/commit_provenance.rs` — `CommitProvenanceRegistry`, `CommitProvenanceRecord`, `PushEvent`, `CommitLineage`: push events include branch, worktree, superseded-by, and oldest-first commit lineage; serde-stable, thread-safe (E7-2)
- `rust/crates/runtime/tests/commit_provenance.rs` — 16 integration tests: push event field coverage, superseded-by tracking, lineage accumulation, worktree/branch filtering, latest-per-branch map, monotonic seq numbers, JSON round-trip, Display formatting, empty registry, event count, parallel worktrees on same branch, superseded-by omitted from JSON when absent, standalone record serialization (E7-2)
- `docs/claw-code/RETROSPECTIVE.md` — Sprint 7 close-out retrospective: velocity summary, acceptance review, what-went-well, improvement areas, docs audit, Definition-of-Done audit, Epic 8 action items (retrospective hardening)
- `rust/crates/runtime/src/branch_lock.rs` — `BranchLockRegistry`, `BranchCollisionEvent`, `BranchAcquireOutcome`, `BranchLockEntry`: thread-safe branch-lock detection for parallel swarm workers; collision emits structured event before spawn (E7-1)
- `rust/crates/runtime/tests/branch_lock_detection.rs` — 12 integration tests: first-acquire success, collision event fields, synchronous detection before spawn, release semantics, wrong-worker release rejection, independent branch isolation, N-worker race (only first wins), re-acquire after release, JSON round-trip, lock_count tracking, all_locks snapshot, human-readable collision message (E7-1)
- `rust/crates/runtime/tests/session_compaction.rs` — 13 session compaction tests against known token thresholds: below/at/above threshold boundary conditions, token estimate decreases, tail preservation, double-compaction summary merging, empty session, single-message no-op, tool-use counting, System-role injection, custom configs, linear scaling (E6-3)
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
