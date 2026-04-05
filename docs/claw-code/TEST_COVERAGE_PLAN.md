# Test Coverage Plan

**Date:** 2026-04-05  
**Author:** Copilot Task Agent  
**Purpose:** Identify current coverage gaps, define a coverage improvement plan, and document new tests added.

---

## Current State

### Rust Test Surface

| File | Tests | Coverage Status |
|------|-------|----------------|
| `rusty-claude-cli/tests/cli_flags_and_config_defaults.rs` | ~8 tests | CLI flags, model, permission mode |
| `rusty-claude-cli/tests/resume_slash_commands.rs` | ~6 tests | Session resume, slash commands |
| `rusty-claude-cli/tests/mock_parity_harness.rs` | 10 scenarios | Behavioral parity: streaming, file, bash, plugin |
| `runtime/tests/integration_tests.rs` | 12 tests | Cross-module: stale_branch↔policy, green_contract↔policy |
| **Total Rust** | **~36 tests** | CI: `rusty-claude-cli` only; `runtime` tests local-only |

### Python Test Surface

| File | Tests | Coverage Status |
|------|-------|----------------|
| `tests/test_porting_workspace.py` | 22 tests | CLI integration only; no unit tests for internal classes |

### Coverage Gaps

| Gap | Risk | Priority |
|----|------|----------|
| `cargo test --workspace` not in CI | High: runtime/tools bugs ship silently | P0 |
| `render_diff_report` tests flaky (live working-tree reads) | Medium: CI unreliability | P0 |
| No tests for `stale_branch` → `recovery_recipes` → `policy_engine` full chain | Medium | P1 |
| No tool spec schema validation tests | Medium | P1 |
| No config precedence edge-case tests (malformed hooks, conflicting layers) | Medium | P1 |
| No Python unit tests for `models.py`, `session_store.py`, `tool_filtering` logic | Low | P1 |
| No integration test for MCP degraded-startup path | Medium | P1 |
| No session compaction boundary tests | Low | P2 |
| No tests for `trust_resolver` allowlist logic | Low | P2 |
| No contract tests for `--output-format json` across commands | Medium | P0 |

---

## New Tests Added

The following tests have been added in `tests/test_models.py` and `tests/test_session_store.py` to improve Python workspace coverage, and `tests/test_tool_coverage.py` for tool surface validation.

### `tests/test_models.py`
Unit tests for `src/models.py` dataclasses and `src/port_manifest.py`.

### `tests/test_session_store.py`
Unit tests for `src/session_store.py` — session persistence, loading, and listing.

### `tests/test_tool_coverage.py`
Unit tests verifying the tool and command surfaces are non-trivial and correctly structured.

---

## Rust Test Coverage Roadmap

### Sprint 3: Integration test — stale_branch → recovery_recipes → policy_engine

Add to `rust/crates/runtime/tests/integration_tests.rs`:

```rust
// Scenario: stale branch detected → recovery recipe selected → policy escalation
#[test]
fn stale_branch_triggers_recovery_recipe_then_policy_escalation() { ... }

// Scenario: compile failure → recovery recipe lookup → auto-recovery action
#[test]  
fn compile_failure_maps_to_recovery_recipe_and_policy_action() { ... }
```

### Sprint 3: Tool spec schema validation

Add to `rust/crates/tools/src/lib.rs` or a new `rust/crates/tools/tests/`:

```rust
#[test]
fn all_tool_specs_have_non_empty_name_and_description() {
    for spec in mvp_tool_specs() {
        assert!(!spec.name.is_empty());
        assert!(!spec.description.is_empty());
    }
}

#[test]
fn all_tool_specs_have_valid_input_schema() {
    for spec in mvp_tool_specs() {
        // input_schema must be a JSON object
        assert!(spec.input_schema.is_object());
    }
}
```

### Sprint 3: Config edge-case tests

Add to `rust/crates/runtime/tests/`:

```rust
#[test]
fn config_loads_correctly_when_user_config_absent() { ... }

#[test]
fn malformed_hook_entry_fails_with_source_context() { ... }

#[test]
fn local_settings_override_project_settings() { ... }
```

### Sprint 5: MCP degraded-startup integration test

Using `mock-anthropic-service` pattern for MCP:

```rust
#[test]
fn mcp_manager_reports_structured_failure_when_server_handshake_fails() {
    // Spawn a mock MCP server that closes connection immediately
    // Assert McpManager produces a structured failed_servers report
    // Assert recovery_recommendations is non-empty
}
```

---

## Python Coverage Roadmap

### Sprint 4: Unit tests for `src/models.py`

```python
class TestSubsystemModel(unittest.TestCase):
    def test_subsystem_has_required_fields(self): ...
    def test_module_count_is_positive(self): ...
    def test_backlog_state_defaults(self): ...
```

### Sprint 4: Unit tests for session store

```python
class TestSessionStore(unittest.TestCase):
    def test_save_and_load_roundtrip(self): ...
    def test_list_sessions_returns_sorted_by_date(self): ...
    def test_missing_session_raises_not_found(self): ...
```

### Sprint 4: Unit tests for tool filtering

```python
class TestToolFiltering(unittest.TestCase):
    def test_deny_prefix_excludes_matching_tools(self): ...
    def test_no_mcp_flag_excludes_mcp_tools(self): ...
    def test_simple_mode_returns_subset(self): ...
```

---

## Coverage Targets

| Surface | Current (est.) | Target (Sprint 7) |
|---------|---------------|-------------------|
| Rust runtime (line) | ~35% | ≥60% |
| Rust tools (line) | ~20% | ≥55% |
| Python src/ (unit) | ~5% | ≥50% |
| Python CLI (integration) | ~80% | ≥85% |
| Tool spec schemas (contract) | 0% | 100% |
| Config precedence (scenario) | ~30% | ≥80% |

---

## CI Coverage Gate (Proposed)

Add to `.github/workflows/rust-ci.yml`:

```yaml
coverage:
  name: cargo test --workspace (coverage)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
      with:
        components: llvm-tools-preview
    - name: Install cargo-llvm-cov
      run: cargo install cargo-llvm-cov
    - name: Run coverage
      run: cargo llvm-cov --workspace --lcov --output-path lcov.info
    - name: Upload to Codecov
      uses: codecov/codecov-action@v4
      with:
        files: lcov.info
```

This will track coverage over time without blocking builds (until targets are established and agreed).
