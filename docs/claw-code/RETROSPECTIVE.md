# Sprint Retrospective — Swarm Safety & Compaction Hardening

*Sprint 7 close-out · Weeks 15–16*

---

## Velocity Summary

| Sprint | Stories | Points | Delivered |
|--------|---------|--------|-----------|
| Sprint 0 | Foundation fixes | 13 | ✅ |
| Sprint 1 | Session & recovery | 18 | ✅ |
| Sprint 2 | Policy & cross-module wiring | 16 | ✅ |
| Sprint 3 | Tool/command dispatch coverage | 21 | ✅ |
| Sprint 4 | Container workflow + dispatch tests | 13 | ✅ |
| Sprint 5 | MCP degraded-startup, lifecycle, plugin config | 16 | ✅ |
| Sprint 6 | Compaction tests + branch-lock detection | 13 | ✅ |
| Sprint 7 | Commit provenance tracking + retrospective | 8 | ✅ |
| **Total** | | **118** | |

---

## Sprint 7 Acceptance Review

### E7-2 — Commit provenance tracking (`commit_provenance.rs`)

**Acceptance criteria met:**

| Criterion | Status |
|-----------|--------|
| Push events include `branch` | ✅ `CommitProvenanceRecord.branch` |
| Push events include `worktree` | ✅ `CommitProvenanceRecord.worktree` |
| Push events include `superseded_by` | ✅ optional field, omitted from JSON when `None` |
| Push events include `lineage` | ✅ `CommitLineage` ordered oldest-first, cumulative |
| Registry is thread-safe | ✅ `Arc<Mutex<…>>` |
| Types are serde-serializable | ✅ full JSON round-trip verified |
| 16 integration tests passing | ✅ `cargo test -p runtime --test commit_provenance` |

**Artifacts:**
- `rust/crates/runtime/src/commit_provenance.rs` — `CommitProvenanceRegistry`, `CommitProvenanceRecord`, `PushEvent`, `CommitLineage`
- `rust/crates/runtime/tests/commit_provenance.rs` — 16 integration tests
- Re-exported from `runtime::lib.rs` at crate root

---

## What Went Well

1. **Incremental module pattern held up** — Each sprint added a focused module
   (`branch_lock.rs`, `commit_provenance.rs`) with a co-located test file, keeping
   the working tree small and reviewable per PR.

2. **No regressions across 8 sprints** — `cargo test --workspace` has remained
   green at every commit.  The `Arc<Mutex<…>>` registry pattern, established in
   `WorkerRegistry` (Sprint 3) and reused in `BranchLockRegistry` (Sprint 6) and
   `CommitProvenanceRegistry` (Sprint 7), proved reliable and easy to test
   without mocking.

3. **Test coverage by boundary condition, not just happy path** — The
   session-compaction suite (E6-3) deliberately exercises below/at/above
   threshold, empty session, single-message, double-compaction, and tool-use
   counting.  The branch-lock suite (E7-1) races N workers against one branch.
   This pattern caught an edge case in `should_compact` where a session with
   fewer messages than `preserve_recent_messages` must never compact even when
   `max_estimated_tokens` is 1.

4. **Structured events are the integration seam** — `LaneEvent`, `WorkerEvent`,
   `BranchCollisionEvent`, and `PushEvent` all share the same design: serde
   types with wire-stable names, `Display` for human logs, and `skip_serializing_if`
   to keep JSON clean.  This consistency made adding `PushEvent` in Sprint 7
   straightforward.

---

## What Could Be Improved

1. **No real git subprocess integration** — `commit_provenance.rs` and
   `branch_lock.rs` are purely in-memory.  A follow-on epic should wire
   `CommitProvenanceRegistry::record_push` to actual `git push` output parsing
   (e.g. reading the ref-update line from stdout) so provenance is populated
   automatically rather than by manual caller bookkeeping.

2. **No persistence between runs** — the registries reset on process restart.
   For long-running swarm operators, provenance and lock state should be written
   to a file (e.g. `.claw/swarm-state.json`) and restored on startup.  This is
   a natural follow-on for Epic 8.

3. **Compaction threshold is estimated, not exact** — `estimate_session_tokens`
   uses a `chars / 4` heuristic.  As the LLM context window grows, a calibrated
   model-specific tokenizer (e.g. tiktoken or a Rust port) would give more
   accurate results and reduce unnecessary compaction or missed compaction.

4. **`cargo clippy` warnings in pre-existing code** — seven pre-existing clippy
   warnings exist in the runtime lib (e.g. `is_symlink_escape` unused).  These
   were out of scope for each sprint but accumulating them is technical debt.  A
   dedicated "lint hardening" sprint task should resolve them before the next
   major release.

---

## Docs Review

All documentation updated or created across the project:

| Document | Status | Sprint |
|----------|--------|--------|
| `CHANGELOG.md` | Up-to-date through Sprint 7 | All |
| `docs/claw-code/SCRUM_PROJECT_PLAN.md` | All epics and sprints defined | Sprint 0 |
| `docs/claw-code/REPOSITORY_OVERVIEW.md` | Architecture overview | Sprint 0 |
| `docs/claw-code/CONTAINER_WORKFLOW.md` | Container build/run workflow | Sprint 4 |
| `docs/claw-code/PLUGIN_CONFIG_SPEC.md` | Plugin config contract | Sprint 5 (E6-1) |
| `docs/claw-code/STANDARDS_REVIEW.md` | Code standards review | Sprint 0 |
| `docs/claw-code/STANDARDS_FORTIFICATION.md` | Lint/format enforcement plan | Sprint 0 |
| `docs/claw-code/TEST_COVERAGE_PLAN.md` | Test coverage strategy | Sprint 2 |
| `docs/claw-code/RETROSPECTIVE.md` | This document | Sprint 7 |

---

## Definition of Done — Audit

All work through Sprint 7 satisfies the global Definition of Done:

- [x] Features implemented and pass `cargo test --workspace`
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes for new code
  (pre-existing warnings in unrelated files are tracked as technical debt)
- [x] `cargo fmt` applied
- [x] `CHANGELOG.md` updated at each sprint
- [x] PR reviewed (AI code review via parallel_validation)
- [x] No force-push; incremental commits on feature branch

---

## Action Items for Epic 8

| Action | Owner | Priority |
|--------|-------|----------|
| Wire `CommitProvenanceRegistry` to real `git push` output | Executor | P1 |
| Persist swarm state (locks + provenance) to `.claw/swarm-state.json` | Executor | P1 |
| Resolve pre-existing `cargo clippy` warnings in runtime lib | All | P2 |
| Replace token estimator heuristic with calibrated tokenizer | Architect | P2 |
| Add `BranchLockRegistry` snapshot restore from persisted state | Executor | P2 |
| E2E swarm simulation test: two workers, one branch, verify collision + provenance | QA | P1 |
