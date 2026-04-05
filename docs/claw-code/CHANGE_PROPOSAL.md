# Change Proposal — What to Change and Add

**Date:** 2026-04-05  
**Author:** Copilot Task Agent  
**Purpose:** Concrete, prioritized list of proposed changes and additions — with rationale — before any code is touched.

---

## Principle

> Before making any change: understand what exists, document what is wrong or missing, then make surgical edits. This proposal is the pre-change record.

---

## Part 1 — What to CHANGE (Fixes to existing behavior)

### C-01: Expand CI to `cargo test --workspace`

**Current:** `.github/workflows/rust-ci.yml` runs `cargo test -p rusty-claude-cli` only.  
**Problem:** 8 other crates (runtime, tools, api, commands, plugins, telemetry, compat-harness, mock-anthropic-service) are never CI-verified. Bugs can slip in silently.  
**Change:** Add a `test-workspace` job to `rust-ci.yml` that runs `cargo test --workspace`.  
**Risk:** Low. Tests already pass locally.

---

### C-02: Isolate `render_diff_report` tests from live working tree

**Current:** These tests read from the actual working tree, causing CI failures during parallel lane operations.  
**Problem:** Flaky CI is worse than no CI — it trains developers to ignore red builds.  
**Change:** Wrap tests in a `tempdir` fixture; replace live working-tree reads with synthetic fixture data.  
**Risk:** Low. Test-only change.

---

### C-03: Fix `--output-format json` to return valid JSON

**Current:** Several commands (`status`, `sandbox`, `skills`, `mcp`, `agents`) accept `--output-format json` but return prose.  
**Problem:** Breaks shell automation and agent-friendly health polling; documented contract is a lie.  
**Change:** Add `OutputFormat` branch in each command renderer; serialize state structs to `serde_json::Value` when JSON requested.  
**Risk:** Medium. Touches multiple command renderers.

---

### C-04: Promote `claw doctor` to top-level shell command

**Current:** `claw doctor` requires entering a REPL session first; calling it from shell errors.  
**Problem:** Healthcheck is the most-needed first-touch command for new users. Burying it behind REPL is backwards.  
**Change:** Add `doctor` as a recognized top-level subcommand in `args.rs`/`app.rs`, callable without an active session.  
**Risk:** Low. Additive routing change.

---

### C-05: Fix warning spam on first-run help

**Current:** `cargo run -p rusty-claude-cli -- --help` prints dozens of compile warnings before help text.  
**Problem:** Terrible first-touch UX; buries the product surface behind noise.  
**Change:** Audit and fix or suppress legitimate warnings with `#[allow(...)]` where appropriate, or fix underlying code issues causing warnings.  
**Risk:** Low (lint fixes). Medium if warnings indicate real bugs.

---

### C-06: Reconcile README product narrative

**Current:** Top-level README says "the active workspace is now Rust" but later sections describe the repo as Python-first with Python quickstart as primary.  
**Problem:** Confuses new users and contributors about what is canonical.  
**Change:** Reorder README; lead with Rust workspace, relegate Python porting workspace to a "Background / History" section with accurate framing.  
**Risk:** Very low. Documentation only.

---

### C-07: Unify legacy config namespaces in `skills` output

**Current:** `skills` shows mixed `.codex` and `.claude` project roots from historical layering.  
**Problem:** Leaks implementation history into the product surface; unclear which namespace is authoritative.  
**Change:** Normalize `skills` output to show canonical `.claw` namespace; annotate legacy paths as "migrated from `.claude`" if present.  
**Risk:** Low. Display-layer change; no persistent config mutation.

---

## Part 2 — What to ADD (New capabilities and artifacts)

### A-01: CHANGELOG.md with auto-update GitHub Action

**Why:** Required by revvel AD-01. Also: agents need a machine-readable record of what changed and when to make informed decisions during handoffs.  
**What:** Create `CHANGELOG.md` following Keep a Changelog format. Add a GitHub Action that appends entries from PR titles on merge to main.  
**Format:** `## [Unreleased]` → `## [YYYY-MM-DD]` → Added/Changed/Fixed/Removed.

---

### A-02: Release binary workflow

**Why:** The CLI binary is the product. Without a tagged release pipeline, users must `cargo build` from source — raising the barrier to adoption significantly.  
**What:** Add `.github/workflows/release.yml` that triggers on version tags (`v*.*.*`), builds binaries for `x86_64-unknown-linux-musl`, `x86_64-apple-darwin`, `aarch64-apple-darwin`, and `x86_64-pc-windows-gnu`, and publishes them as GitHub Release assets.

---

### A-03: `CONTRIBUTING.md`

**Why:** New contributors (human and claw) need to know the branch convention, how to submit PRs, what the review process is, and how to run the test suite.  
**What:** Document: branch naming (`gaebal/feature-name`, `omx-issue-N`), commit message convention, required CI checks, how to run `cargo test --workspace`, how to use the mock parity harness.

---

### A-04: Cross-module integration tests for `stale_branch` → `recovery_recipes` → `policy_engine`

**Why:** These three modules form a critical chain. The existing `integration_tests.rs` covers `stale_branch` + `policy_engine` but not the recovery recipe bridge.  
**What:** Add tests in `rust/crates/runtime/tests/` that simulate: stale branch detected → recovery recipe selected → policy action emitted → outcome verified.

---

### A-05: Tool spec schema validation tests

**Why:** 40 tool specs are exposed but none are validated beyond compile-time type checks. Invalid schemas would break agents silently at runtime.  
**What:** Add a test that iterates `mvp_tool_specs()` and validates each entry: non-empty name, non-empty description, valid JSON schema in `input_schema`, `required_permission` field present.

---

### A-06: Config precedence edge-case tests

**Why:** The 5-layer config merge is a core reliability feature. Edge cases (malformed entries, conflicting values, missing files) should be explicitly tested.  
**What:** Add tests in `rust/crates/runtime/` covering: missing user config, malformed hook entry (already partially covered per PARITY.md), conflicting permission modes across layers, empty local settings file.

---

### A-07: Python unit tests for individual `src/` module classes

**Why:** The current 22 Python tests are all CLI integration tests. Internal module logic (config merging, session storage, tool dispatch filtering) is untested at unit level.  
**What:** Add `tests/test_models.py`, `tests/test_session_store.py`, `tests/test_tool_filtering.py` covering the key non-trivial Python classes.

---

### A-08: Container workflow documentation

**Why:** The runtime detects Docker/Podman/container state, but no docs show how to use it. CI also needs container-compatible test runs.  
**What:** Add `docs/claw-code/CONTAINER_WORKFLOW.md` with canonical commands for: building the Docker image, running `cargo test --workspace` in a container, mounting a repo for bind-mount usage.

---

### A-09: Branding consistency CI lint step

**Why:** After repo migration, old org names survive in badge URLs (`ultraworkers/claw-code` still appears in README star-history links and badge references).  
**What:** Add a simple GitHub Actions step or `make check-branding` target that greps for known stale org/repo names and fails if found.

---

### A-10: MCP degraded-startup integration test

**Why:** The `mcp_lifecycle_hardened.rs` module handles partial server failures, but there is no test that actually spawns a mock broken MCP server and verifies the structured error report.  
**What:** Add an integration test using a mock MCP server that fails during handshake; assert that `failed_servers` and `recovery_recommendations` are populated correctly in the tool output.

---

## Part 3 — What NOT to Change (Explicit non-changes)

1. **Do not remove the Python workspace.** It serves a distinct purpose (parity analysis) and has working tests. Keep it.
2. **Do not change the crate boundary design.** The separation between `api`, `runtime`, `tools`, `plugins`, and `commands` is sound.
3. **Do not break the existing mock parity harness.** It is a key verification layer for behavioral correctness.
4. **Do not enable `unsafe_code`.** The workspace-level `forbid` is an important safety guarantee.
5. **Do not change session storage format** without a migration path and PARITY.md update.
6. **Do not add external runtime dependencies** (non-stdlib crates) without checking the advisory database and documenting the rationale.

---

## Summary Priority Order

| Priority | Action |
|----------|--------|
| P0 (Sprint 0) | C-05 (warning spam), C-06 (README), A-01 (CHANGELOG), A-03 (CONTRIBUTING) |
| P0 (Sprint 1) | C-01 (CI scope), C-03 (JSON output), C-04 (doctor), C-02 (flaky tests) |
| P1 (Sprint 2-3) | C-07 (namespaces), A-04 (integration tests), A-05 (tool spec tests), A-06 (config tests) |
| P1 (Sprint 4) | A-02 (release workflow), A-07 (Python tests), A-08 (container docs) |
| P1 (Sprint 5) | A-10 (MCP degraded test), A-09 (branding lint) |
