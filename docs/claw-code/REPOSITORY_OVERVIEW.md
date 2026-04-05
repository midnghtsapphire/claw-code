# Claw Code — Repository Overview

**Status:** Active  
**Version:** 0.1.0  
**Primary Language:** Rust (with Python porting workspace)  
**Last Updated:** 2026-04-05

---

## What Is Claw Code?

Claw Code is an **autonomous coding harness** — a CLI and runtime that lets AI agents (claws/lobsters) operate software development workflows without a human sitting at the terminal. The human provides direction; the claws coordinate, build, test, recover, and push.

It is simultaneously:
1. A demonstration of autonomous, agent-driven software development in public.
2. A Rust-native CLI that wraps the Anthropic API with session management, permission enforcement, MCP/plugin lifecycle management, and event-native orchestration.
3. A Python porting workspace that mirrors the original Claude Code tool surface for analysis and compatibility research.

---

## Repository Layout

```
claw-code/
├── rust/                          # Active Rust workspace (canonical CLI + runtime)
│   ├── Cargo.toml                 # Workspace manifest (9 crates)
│   └── crates/
│       ├── api/                   # Anthropic API client types and streaming
│       ├── commands/              # Slash command dispatch (/help, /status, /config…)
│       ├── compat-harness/        # Compatibility shim for parity testing
│       ├── mock-anthropic-service/# Deterministic mock Anthropic API for testing
│       ├── plugins/               # Plugin install/enable/disable/uninstall + hooks
│       ├── runtime/               # Core runtime (session, MCP, permissions, file ops…)
│       ├── rusty-claude-cli/      # CLI binary entrypoint + integration tests
│       ├── telemetry/             # Usage and cost tracking
│       └── tools/                 # Tool dispatch layer (40 exposed tool specs)
├── src/                           # Python porting workspace
│   ├── main.py                    # CLI entrypoint (manifest, summary, bootstrap…)
│   ├── commands.py                # Command port metadata (150+ commands)
│   ├── tools.py                   # Tool port metadata (100+ tools)
│   ├── models.py                  # Dataclasses for subsystems/modules/backlog
│   ├── query_engine.py            # Porting summary renderer
│   ├── port_manifest.py           # Workspace structure summarizer
│   ├── runtime.py                 # PortRuntime — session bootstrap + turn loop
│   ├── execution_registry.py      # Command + tool execution registry
│   └── [30+ subsystem packages]   # Mirrors of original Claude Code subsystems
├── tests/                         # Python verification
│   └── test_porting_workspace.py  # 22 integration tests for Python workspace
├── docs/claw-code/                # Project documentation (this directory)
├── .github/workflows/
│   └── rust-ci.yml               # CI: fmt check + rusty-claude-cli tests
├── CLAUDE.md                      # Instructions for Claude Code when working here
├── PHILOSOPHY.md                  # The "humans direct, claws execute" manifesto
├── ROADMAP.md                     # 5-phase roadmap + immediate P0–P3 backlog
├── PARITY.md                      # 9-lane parity checkpoint with commit hashes
└── USAGE.md                       # Build, auth, CLI, session, and harness workflows
```

---

## Technology Stack

| Layer | Technology |
|-------|-----------|
| CLI binary | Rust (`rusty-claude-cli` crate) |
| Runtime / session | Rust (`runtime` crate) |
| API client | Rust (`api` crate, streaming SSE) |
| Tool dispatch | Rust (`tools` crate, 40 tool specs) |
| Plugins/hooks | Rust (`plugins` crate, shell hook lifecycle) |
| MCP transport | Rust (`runtime::mcp_stdio`, JSON-RPC over stdio) |
| LSP client | Rust (`runtime::lsp_client`, diagnostic/hover/definition) |
| Python porting workspace | Python 3, stdlib only |
| CI | GitHub Actions (`rust-ci.yml`) |
| Auth | Anthropic API key or OAuth (`claw login`) |
| Config | `.claw.json` / `.claw/settings.json` (5-layer merge) |

---

## Key Runtime Modules

### `runtime` crate (core, ~24,600 LOC)

| Module | Purpose |
|--------|---------|
| `bash.rs` | Shell command execution with timeout/sandbox |
| `bash_validation.rs` | 6-submodule validation (read-only, destructive, sed, path, mode, semantics) |
| `config.rs` | 5-layer config merge with hook validation |
| `conversation.rs` | The main API + streaming turn loop |
| `file_ops.rs` | Read/write/edit/glob/grep with size limits and workspace boundary guards |
| `green_contract.rs` | Green-ness levels: Targeted → Package → Workspace → MergeReady |
| `hooks.rs` | Pre/post tool hook execution |
| `lane_events.rs` | Typed lane event schema (started/blocked/failed/finished) |
| `lsp_client.rs` | LSP registry: diagnostics, hover, definition, references, symbols |
| `mcp.rs` + `mcp_stdio.rs` | MCP JSON-RPC lifecycle (spawn, handshake, tool/resource discovery) |
| `mcp_lifecycle_hardened.rs` | Degraded-startup reporting and partial-server failure classification |
| `mcp_tool_bridge.rs` | McpToolRegistry: connection status, auth, tool dispatch |
| `permission_enforcer.rs` | PermissionEnforcer: file-write boundary + bash read-only gating |
| `permissions.rs` | Permission policy types (read-only/workspace-write/danger-full-access) |
| `plugin_lifecycle.rs` | Plugin install/validate/enable/disable/uninstall |
| `policy_engine.rs` | PolicyEngine: rule-based action selection (merge-forward, escalate…) |
| `recovery_recipes.rs` | Auto-recovery recipes for known failure scenarios |
| `sandbox.rs` | Sandbox support detection (unshare capability probe) |
| `session.rs` | Session persistence under `.claw/sessions/` |
| `session_control.rs` | Structured session control API (create/await/send/fetch/restart/terminate) |
| `stale_branch.rs` | Branch freshness detection against main |
| `summary_compression.rs` | Noisy event stream → phase/blocker/checkpoint summary |
| `task_packet.rs` | Typed task packet (objective, scope, branch policy, acceptance tests…) |
| `task_registry.rs` | In-memory task lifecycle registry |
| `team_cron_registry.rs` | TeamRegistry + CronRegistry for multi-agent team/cron management |
| `trust_resolver.rs` | Allowlist-based auto-trust for known repos |
| `worker_boot.rs` | WorkerRegistry + WorkerStatus state machine (spawning → ready → running) |

### `tools` crate (tool dispatch, ~3,800 LOC)

Exposes **40 tool specs** via `mvp_tool_specs()` and routes invocations through `execute_tool()`. Core tools with real execution: `bash`, `read_file`, `write_file`, `edit_file`, `glob_search`, `grep_search`. Registry-backed: `Task*`, `Team*`, `Cron*`, `LSP`, `MCP`.

---

## Current Test Coverage

| Surface | Tests | Status |
|---------|-------|--------|
| `rusty-claude-cli` (integration) | ~18 tests across 3 files | CI-gated |
| `runtime` (integration) | 12 cross-module integration tests | Local only |
| `mock-anthropic-service` | 10 harness scenarios, 19 captured requests | Local only |
| Python workspace | 22 unittest tests | CI-gated (Python path) |

### Gaps
- CI only runs `cargo test -p rusty-claude-cli`, not `cargo test --workspace`
- `render_diff_report` tests are flaky (read real working-tree state)
- No integration tests for `stale_branch` ↔ `recovery_recipes` ↔ `policy_engine` end-to-end
- No tests for `config.rs` precedence edge cases or malformed hook entries
- No contract tests for the 40 tool specs (schema validation)
- Python workspace has no unit tests for individual module classes (only CLI integration tests)

---

## 9-Lane Parity Checkpoint

All 9 feature lanes have merged to `main` as of 2026-04-03:

1. Bash validation (6-submodule validation framework)
2. CI sandbox fix (unshare capability probe)
3. File-tool edge cases (binary detection, size limits, workspace boundary)
4. TaskRegistry (in-memory task lifecycle)
5. Task wiring (TaskRegistry → tool dispatch)
6. Team+Cron (TeamRegistry + CronRegistry)
7. MCP lifecycle (McpToolRegistry bridge)
8. LSP client (LspRegistry dispatch)
9. Permission enforcement (PermissionEnforcer)

---

## Philosophy Summary

> Humans set direction. Claws coordinate, build, test, recover, and push.

The real product is not the code files — it is the **coordination system** that produces them:
- OmX (`oh-my-codex`) — workflow layer, execution modes, planning keywords
- clawhip — event/notification router (git, tmux, GitHub, agent lifecycle)
- OmO (`oh-my-openagent`) — multi-agent coordination, handoffs, disagreement resolution

---

## Build & Verification

```bash
# Build
cd rust && cargo build --workspace

# Format check
cd rust && cargo fmt --all --check

# Lint
cd rust && cargo clippy --workspace --all-targets -- -D warnings

# Test (Rust)
cd rust && cargo test --workspace

# Test (Python)
python3 -m unittest discover -s tests -v
```

---

## Ownership & Disclaimer

- This repository is **not affiliated with, endorsed by, or maintained by Anthropic**.
- Maintained by the UltraWorkers ecosystem (Bellman/Yeachan Heo and contributors).
- Autonomously built by claw workflows using clawhip, oh-my-codex, oh-my-openagent.
