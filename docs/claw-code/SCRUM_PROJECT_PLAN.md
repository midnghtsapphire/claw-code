# Scrum Project Plan — Claw Code

**Generated:** 2026-04-05  
**Standards Source:** revvel-standards EXRUP methodology + Claw Code ROADMAP.md  
**Framework:** Scrum (2-week sprints)  
**Team Structure:** Human Product Owner + Claw Agents (Architect, Executor, Reviewer, QA)

---

## Product Vision

> Build the most **clawable** coding harness: deterministic to start, machine-readable in state and failure, recoverable without a human watching the terminal, and event-first throughout.

---

## Product Backlog

### Epic 1 — CI & Build Reliability (P0)
| ID | User Story | Acceptance Criteria | Points |
|----|-----------|---------------------|--------|
| E1-1 | As a claw agent, I need `cargo test --workspace` to run in CI so I know all crates are green | CI job added; all workspace tests pass | 3 |
| E1-2 | As a developer, I need `render_diff_report` tests to be deterministic so CI doesn't flake | Tests isolated to tmpdir; no live working-tree reads | 2 |
| E1-3 | As a release manager, I need a tagged release workflow so I can publish binary artifacts | GitHub Actions workflow produces signed binaries for Linux/macOS/Windows on tag | 5 |
| E1-4 | As a user, I need warning-free first-run output so I understand the tool surface immediately | `cargo run -- --help` produces zero warnings | 2 |
| E1-5 | As a user, I need README to reflect actual repo state (Rust-active, not Python-first) | README top-level narrative updated; no contradiction | 1 |

### Epic 2 — JSON Output Contract (P0)
| ID | User Story | Acceptance Criteria | Points |
|----|-----------|---------------------|--------|
| E2-1 | As a claw script, I need `claw status --output-format json` to return valid JSON | Machine-parseable JSON for status command | 3 |
| E2-2 | As a claw script, I need `claw sandbox --output-format json` to return valid JSON | Machine-parseable JSON | 2 |
| E2-3 | As a claw script, I need `claw skills --output-format json` to return valid JSON inventory | JSON with skill name, path, config namespace | 3 |
| E2-4 | As a claw script, I need `claw mcp --output-format json` to return valid JSON inventory | JSON with server name, status, tools/resources | 3 |
| E2-5 | As a developer, I need `--output-format` contract documented and tested for every command | Contract test for all 40+ commands | 5 |

### Epic 3 — Onboarding & Doctor UX (P0)
| ID | User Story | Acceptance Criteria | Points |
|----|-----------|---------------------|--------|
| E3-1 | As a new user, I need `claw doctor` callable from shell without entering REPL | `claw doctor` works from shell; returns structured health report | 3 |
| E3-2 | As a new user, I need doctor results to be JSON-serializable for automated setup scripts | `claw doctor --output-format json` returns valid JSON | 2 |
| E3-3 | As a user, I need legacy config namespaces (.codex, .claude) unified in `skills` output | Skills output shows canonical `.claw` namespace; legacy namespaces noted as migrated | 3 |

### Epic 4 — Test Coverage Expansion (P1)
| ID | User Story | Acceptance Criteria | Points |
|----|-----------|---------------------|--------|
| E4-1 | As a maintainer, I need integration tests for stale_branch → recovery_recipes → policy_engine | Tests covering all 3-module chain; happy + failure paths | 5 |
| E4-2 | As a maintainer, I need config precedence edge-case tests | Tests for malformed hooks, missing keys, conflicting values | 3 |
| E4-3 | As a maintainer, I need schema validation tests for all 40 tool specs | Each spec validated: name, description, input schema fields | 3 |
| E4-4 | As a maintainer, I need Python unit tests for individual src/ module classes | Tests for `config.rs`-equivalent config classes, tool dispatch, session store | 5 |
| E4-5 | As a CI engineer, I need integration test for MCP degraded-startup path | Mock MCP server that fails; verify structured failure report | 5 |

### Epic 5 — Documentation (P1 per Revvel AD-01)
| ID | User Story | Acceptance Criteria | Points |
|----|-----------|---------------------|--------|
| E5-1 | As a contributor, I need a CHANGELOG.md so I can track what changed and when | CHANGELOG.md present; GitHub Action auto-updates it on push to main | 2 |
| E5-2 | As a user, I need a container workflow documented | `docs/claw-code/CONTAINER_WORKFLOW.md` with Docker/Podman commands for build, test, run | 2 |
| E5-3 | As a user, I need branding consistency across all docs | CI lint step catches stale org names (ultraworkers vs midnghtsapphire) in badges/URLs | 2 |
| E5-4 | As a new contributor, I need a CONTRIBUTING.md explaining branch/PR/review process | CONTRIBUTING.md with lane, branch, PR, and hook conventions | 2 |

### Epic 6 — MCP & Plugin Lifecycle Maturity (P1)
| ID | User Story | Acceptance Criteria | Points |
|----|-----------|---------------------|--------|
| E6-1 | As a plugin author, I need a config validation contract spec | Spec document + validation unit tests | 3 |
| E6-2 | As a claw agent, I need end-to-end MCP lifecycle tests beyond registry bridge | Tests cover: config load → spawn → handshake → tool discovery → invocation → shutdown | 8 |
| E6-3 | As a user, I need session compaction tested against known token thresholds | Compaction tests with mock token counts; boundary conditions | 5 |

### Epic 7 — Swarm Efficiency (P3)
| ID | User Story | Acceptance Criteria | Points |
|----|-----------|---------------------|--------|
| E7-1 | As a swarm operator, I need branch-lock detection before parallel workers collide | `branch_lock.rs` module; collision detection emits structured event before spawn | 8 |
| E7-2 | As a swarm operator, I need commit provenance tracking per worktree | Push events include branch, worktree, superseded-by, lineage | 5 |

### Epic 8 — Swarm State Persistence (P2)
| ID | User Story | Acceptance Criteria | Points |
|----|-----------|---------------------|--------|
| E8-1 | As a swarm operator, I need branch-lock and provenance state persisted across restarts | `swarm_state.rs`: `SwarmStateStore` saves/loads `BranchLockRegistry` + `CommitProvenanceRegistry` to `.claw/swarm-state.json`; `BranchLockRegistry::snapshot()`/`restore_from_snapshot()` | 5 |
| E8-2 | As a swarm operator, I need `git push` output auto-populated into provenance | `parse_git_push_output` + `record_push_from_git_output` in `commit_provenance.rs`; forced-push detects superseded SHA | 3 |
| E8-3 | As a QA engineer, I need an E2E swarm simulation test | Two workers compete for one branch: collision detected, provenance recorded, state round-trip verified | 5 |
| E8-4 | As a maintainer, I need runtime lib clippy warnings resolved | Zero warnings in `cargo clippy -p runtime --lib`; `#[must_use]`, format inlining, unused-import removal, structural lint suppression | 3 |

---

## Sprint Plan

### Sprint 0 — Foundation (Weeks 1–2)
**Theme:** Fix what blocks everything else  
**Goal:** CI reliable, first-run UX clean, README accurate

| Story | Assignee | Points |
|-------|----------|--------|
| E1-2 Isolate render_diff_report tests | Executor | 2 |
| E1-4 Warning-free first-run output | Executor | 2 |
| E1-5 README narrative fix | Executor | 1 |
| E5-1 CHANGELOG.md + GitHub Action | Executor | 2 |
| E5-4 CONTRIBUTING.md | Executor | 2 |
| **Sprint Total** | | **9** |

**Definition of Done:** CI passes on main; README accurately describes Rust-primary repo; no warning spam on first run; CHANGELOG exists and is auto-updated.

---

### Sprint 1 — CI Scope + JSON Contract (Weeks 3–4)
**Theme:** Make machine-readable outputs actually machine-readable  
**Goal:** `cargo test --workspace` in CI; JSON output reliable

| Story | Assignee | Points |
|-------|----------|--------|
| E1-1 `cargo test --workspace` CI job | Executor + QA | 3 |
| E2-1 `claw status --output-format json` | Executor | 3 |
| E2-2 `claw sandbox --output-format json` | Executor | 2 |
| E3-1 `claw doctor` from shell | Executor | 3 |
| **Sprint Total** | | **11** |

**Definition of Done:** CI runs full workspace tests; `status` and `sandbox` return parseable JSON; `claw doctor` works from shell.

---

### Sprint 2 — JSON Contract Completion + Doctor UX (Weeks 5–6)
**Theme:** Complete the JSON output contract across all major commands

| Story | Assignee | Points |
|-------|----------|--------|
| E2-3 `skills --output-format json` | Executor | 3 |
| E2-4 `mcp --output-format json` | Executor | 3 |
| E2-5 Contract tests for all commands | QA | 5 |
| E3-2 `claw doctor --output-format json` | Executor | 2 |
| E3-3 Unify legacy config namespaces | Executor | 3 |
| **Sprint Total** | | **16** |

---

### Sprint 3 — Test Coverage Expansion (Weeks 7–8)
**Theme:** Raise test confidence across the runtime crate

| Story | Assignee | Points |
|-------|----------|--------|
| E4-1 stale_branch→recovery→policy integration tests | QA | 5 |
| E4-2 Config precedence edge-case tests | QA | 3 |
| E4-3 Tool spec schema validation tests | QA | 3 |
| E5-3 Branding consistency CI lint | Executor | 2 |
| **Sprint Total** | | **13** |

---

### Sprint 4 — Python Tests + Release Workflow (Weeks 9–10)
**Theme:** Validate Python workspace depth; unblock releases

| Story | Assignee | Points |
|-------|----------|--------|
| E1-3 Release binary workflow | Executor | 5 |
| E4-4 Python unit tests for src/ classes | QA | 5 |
| E5-2 Container workflow docs | Executor | 2 |
| **Sprint Total** | | **12** |

---

### Sprint 5 — MCP Lifecycle Maturity (Weeks 11–12)
**Theme:** Complete MCP lifecycle from config to shutdown

| Story | Assignee | Points |
|-------|----------|--------|
| E6-1 Plugin config validation spec | Architect + Executor | 3 |
| E4-5 MCP degraded-startup integration tests | QA + Executor | 5 |
| E6-2 End-to-end MCP lifecycle tests | QA | 8 |
| **Sprint Total** | | **16** |

---

### Sprint 6 — Session Compaction + Swarm Prep (Weeks 13–14)
**Theme:** Harden remaining session and swarm features

| Story | Assignee | Points |
|-------|----------|--------|
| E6-3 Session compaction tests | QA | 5 |
| E7-1 Branch-lock detection | Architect + Executor | 8 |
| **Sprint Total** | | **13** |

---

### Sprint 7 — Swarm Efficiency (Weeks 15–16)
**Theme:** Parallel worker safety

| Story | Assignee | Points |
|-------|----------|--------|
| E7-2 Commit provenance tracking | Executor | 5 |
| Retrospective hardening + docs review | All | 3 |
| **Sprint Total** | | **8** |

---

### Sprint 8 — Swarm State Persistence & Lint Hardening (Weeks 17–18)
**Theme:** Reliability, restartability, code quality

| Story | Assignee | Points |
|-------|----------|--------|
| E8-1 Swarm state persistence (`swarm_state.rs`) | Executor | 5 |
| E8-2 Git push output parsing for provenance | Executor | 3 |
| E8-3 E2E swarm simulation test (`swarm_e2e.rs`) | QA | 5 |
| E8-4 Runtime lib clippy hardening | All | 3 |
| **Sprint Total** | | **16** |

---

## Velocity Assumptions
- 10–16 story points per 2-week sprint for a claw agent team
- Human PO spends ~2 hrs/sprint on direction + review
- Claws handle execution, testing, documentation, and PR creation autonomously

## Definition of Done (Global)
1. Feature is implemented and passes `cargo test --workspace` (or Python `unittest discover`)
2. New code passes `cargo clippy --workspace --all-targets -- -D warnings`
3. Formatted with `cargo fmt`
4. CHANGELOG.md updated
5. PR reviewed (by AI reviewer per revvel CR-01 or human)
6. No force-push; feature branch merged to main via PR

## Scrum Ceremonies

| Ceremony | Cadence | Duration | Owner |
|----------|---------|----------|-------|
| Sprint Planning | Start of sprint | 1 hr (async) | Human PO |
| Daily Claw Standup | Daily | 5 min (clawhip event summary) | clawhip |
| Sprint Review | End of sprint | 30 min | Human PO |
| Sprint Retrospective | End of sprint | 30 min | Human PO + Architect |
| Backlog Refinement | Mid-sprint | 30 min | Human PO |
