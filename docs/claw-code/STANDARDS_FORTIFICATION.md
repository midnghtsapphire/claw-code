# Standards Fortification — How the Standards Could Be More Robust

**Date:** 2026-04-05  
**Author:** Copilot Task Agent  
**Purpose:** Concrete proposals to make revvel-standards more rigorous, applicable, and durable — especially for AI-agent-driven and developer-tool contexts like claw-code.

---

## Overview

The revvel-standards (`midnghtsapphire/revvel-standards`) are a solid starting point for consumer-facing SaaS applications. However, several areas lack the depth needed for:

1. **AI-agent orchestrated development** (claw-code's primary context)
2. **Rust/compiled-language projects** (not just TypeScript/Node.js)
3. **Developer tools vs. consumer apps** (the marketing/SEO sections don't apply)
4. **Security and operational rigor** at production scale

Below is a structured list of proposed fortifications organized by category.

---

## Category 1 — Methodology (EXRUP)

### F-01: Add a Verification Gate Between Phases
**Current gap:** The 8-phase lifecycle moves from Development (Phase 3) to Testing (Phase 4) but has no explicit definition of what "done" means before advancing.  
**Proposal:** Add a Phase Exit Criteria table. Each phase should define:
- Mandatory artifacts that must exist
- Automated checks that must pass
- Stakeholder sign-off required (human or AI reviewer)

**Example for Phase 3 → 4:**
- `cargo test --workspace` green (or equivalent)
- No P0 open issues
- PARITY.md updated if feature changes tool surface
- AI reviewer approval on all open PRs

---

### F-02: Define "Agent Handoff Protocol"
**Current gap:** The standard mentions "genius orchestration" but does not specify how agents hand off work — leading to context loss, duplicate effort, and missed failures.  
**Proposal:** Add a mandatory `HANDOFF.md` template that every agent session must produce before closing. Fields: current phase, last verified checkpoint, open blockers, next recommended action, session log URL.

---

### F-03: Add a "Minimum Viable Test Coverage" Threshold
**Current gap:** Testing (Phase 4) is described but no minimum coverage threshold is specified.  
**Proposal:** Specify:
- **Unit coverage:** ≥70% line coverage for business logic modules
- **Integration tests:** At least one end-to-end scenario per major user flow
- **Contract tests:** All public API endpoints and tool specs validated against their schema
- **Parity tests:** For ports/rewrites, a mock-service-based parity harness (as claw-code already has)

---

## Category 2 — Code Review Standard

### F-04: Define Review SLA and Escalation Path
**Current gap:** Venice AI is the mandatory reviewer, but there is no timeout or escalation rule if Venice is unavailable or slow.  
**Proposal:** Add SLA:
- Venice AI must respond within 15 minutes
- If unavailable after 15 min: auto-escalate to Claude Sonnet (first fallback)
- If Claude unavailable after 30 min: human review required; PR is blocked until reviewed
- Log all SLA misses as GitHub issues tagged `review-sla-miss`

---

### F-05: Add a Security Review Checklist to Every PR
**Current gap:** Security gates exist in the CI pipeline but are not explicitly required at the PR review stage.  
**Proposal:** Every PR must include a machine-completed security checklist:
- [ ] No secrets or credentials in diff
- [ ] No new `unsafe` code (or documented justification)
- [ ] Input validation present for all user-supplied data
- [ ] No new external dependencies without advisory DB check
- [ ] All file path operations use workspace boundary checks

---

### F-06: Require Diff Scope Classification in PR Description
**Current gap:** PRs can be large or small without any scope classification, making review quality unpredictable.  
**Proposal:** Every PR description must declare:
- **Scope:** `single-file` | `module` | `crate` | `workspace` | `cross-crate`
- **Risk level:** `trivial` | `low` | `medium` | `high`
- **Revert complexity:** `easy` | `moderate` | `hard`

AI reviewers adjust review depth based on declared scope and risk.

---

## Category 3 — Deployment Standard

### F-07: Add a Rollback Procedure
**Current gap:** The 10-step deploy checklist covers forward deployment but has no rollback procedure. This is critical for production incidents.  
**Proposal:** Add a Step 11: Rollback Procedure:
- Identify last known-good tag/commit
- `git revert HEAD~N` or checkout last good tag
- Re-run steps 4–10 on the rollback commit
- Document the incident in an `INCIDENT_REPORT.md`

---

### F-08: Require Pre-Deploy Smoke Test Suite
**Current gap:** Post-deploy verification is "check the live URL returns HTML." This is too shallow.  
**Proposal:** Define a mandatory smoke test suite that runs against the live environment after every deploy:
- At minimum: 3 critical user flows automated with Playwright
- Health endpoint check (`/healthz` or equivalent returns 200)
- API endpoint spot-check (one read, one write, one auth flow)
- Deploy is considered failed if smoke tests don't pass within 10 minutes of deployment

---

### F-09: Add Environment Parity Requirements
**Current gap:** Dev→Test→Live pipeline exists on paper but the current "live-first exception" makes it permanent default, not a temporary exception.  
**Proposal:** Require evidence of environment parity even during the live-first phase:
- Staging environment must be provisioned (even if infrequently used)
- All environment variables must be documented in `ENV_VARS.md`
- Staging must be refreshed from production data at least monthly (anonymized if PII)

---

## Category 4 — Concurrent Development Standard

### F-10: Add Branch Lifetime Policy
**Current gap:** Feature branches can live indefinitely, causing drift from main and merge conflicts.  
**Proposal:** 
- Feature branches must be merged or closed within **7 days** of creation
- Branches older than 7 days with no activity trigger a stale-branch alert via clawhip
- If a branch is more than 50 commits behind main, it must rebase before a new PR can be opened

---

### F-11: Add a Swarm Collision Prevention Protocol
**Current gap:** When multiple agents work on the same repository, branch collisions and duplicate implementations occur.  
**Proposal:**
- Before a claw agent starts work, it must register its branch and target module in a shared `SWARM_STATE.md` or API endpoint
- If another agent is already registered for the same module, the new agent must wait or take a different module
- On session close, agents must deregister their claim

---

### F-12: Require Commit Message Convention
**Current gap:** Commit messages are not standardized, making changelogs, bisect, and audit trails harder.  
**Proposal:** Enforce Conventional Commits format:
- `feat(scope): description`
- `fix(scope): description`
- `chore(scope): description`
- `docs(scope): description`
- `test(scope): description`
- `refactor(scope): description`
- CI step that validates commit message format on PR

---

## Category 5 — Auto-Documentation Standard

### F-13: Add Architecture Decision Records (ADRs)
**Current gap:** There is no mechanism for recording *why* key decisions were made. When agents or contributors ask "why was X done this way?" the answer is lost in chat history.  
**Proposal:** 
- Add `docs/decisions/` directory
- Every significant architectural choice requires an ADR file: `ADR-NNN-title.md`
- ADR format: Status, Context, Decision, Consequences
- AI agents should generate ADRs as part of any significant design change

---

### F-14: Require SPRINT_STATE.md for All Active Projects
**Current gap:** AD-04 mentions `SPRINT_STATE.md` but it's described as optional/manual. For autonomous agent workflows, this must be mandatory and machine-updated.  
**Proposal:**
- Every project with active development must have `SPRINT_STATE.md`
- It must be automatically updated after every PR merge with: open story count, velocity, blockers, last merged story ID
- Agents read SPRINT_STATE.md at session start to orient themselves

---

### F-15: Define Artifact Versioning and Expiry Policy
**Current gap:** Documentation artifacts can become stale indefinitely. The PARITY.md in claw-code is manually updated and could drift.  
**Proposal:**
- Every documentation artifact must include a `Last Verified:` date and a `Review By:` date (max 30 days out)
- CI check that fails if any document's `Review By:` date has passed
- Expired documents are automatically flagged with a banner: `⚠️ This document has not been reviewed since YYYY-MM-DD. Accuracy not guaranteed.`

---

## Category 6 — Security Standards (Additions)

### F-16: Add Rust-Specific Security Guidance
**Current gap:** Security standards reference npm/pip but not Rust/Cargo ecosystems.  
**Proposal:**
- `cargo audit` run on every CI build to check for CVEs in Cargo dependencies
- `cargo deny` for license and dependency policy enforcement
- Workspace-level `unsafe_code = "forbid"` unless explicitly justified with ADR
- `RUSTSEC` advisory check before adding any new crate dependency

---

### F-17: Add AI-Agent Security Boundary Specification
**Current gap:** As AI agents are granted increasing autonomy, the standards don't define what they are and are not allowed to do.  
**Proposal:** Add an `AGENT_TRUST_BOUNDARY.md` standard:
- Define allowed agent actions by permission level (`read-only` / `workspace-write` / `danger-full-access`)
- Define prohibited agent actions (e.g., never commit to `main` directly, never delete secrets, never change branch protection rules)
- All agent sessions must declare their permission level at startup
- Any action outside declared permission level triggers an immediate alert and session termination

---

## Category 7 — New Standards to Add

### F-18: Performance Budget Standard (for CLIs and APIs)
**Why:** Performance regressions in CLI tools are often invisible until users complain. For agent loops, slow startup kills throughput.  
**Proposal:** Add `PERFORMANCE_BUDGET.md`:
- CLI startup time: <200ms (cold, no API call)
- API response time (non-streaming): <2s for P95
- Tool invocation overhead: <50ms per tool call (excluding external I/O)
- CI benchmark job that fails if budgets are exceeded by >20%

---

### F-19: Observability Standard
**Why:** revvel-standards mention Sentry for errors but do not define a structured observability strategy.  
**Proposal:** Add `OBSERVABILITY_STANDARD.md`:
- Structured logging with JSON format (already partially in claw-code)
- Distributed tracing for multi-agent workflows (e.g., session IDs propagated through all events)
- Metrics: request count, error rate, p50/p95/p99 latency
- Dashboard: one per active project showing current health at a glance
- Alerting: PagerDuty or equivalent for P0 errors; clawhip Discord notifications for P1

---

### F-20: Dependency Update Policy
**Why:** Outdated dependencies are a leading source of security vulnerabilities.  
**Proposal:** Add `DEPENDENCY_UPDATE_POLICY.md`:
- Security patches: applied within 48 hours of CVE publication
- Minor updates: applied within 2 weeks
- Major updates: evaluated in the next sprint; ADR required if breaking changes
- Automated weekly `dependabot` or `renovate` PRs enabled on all repos
- AI reviewer must pass dependency update PRs as long as tests pass

---

## Summary: Priority Fortifications

| # | Fortification | Impact | Effort |
|---|--------------|--------|--------|
| F-01 | Phase exit criteria | High | Low |
| F-02 | Agent handoff protocol | High | Low |
| F-03 | Minimum test coverage threshold | High | Low |
| F-12 | Conventional commits enforcement | High | Low |
| F-17 | AI agent trust boundary spec | Critical | Medium |
| F-16 | Rust-specific security guidance | High | Medium |
| F-13 | Architecture Decision Records | Medium | Low |
| F-18 | Performance budget standard | Medium | Medium |
| F-04 | Review SLA and escalation | Medium | Low |
| F-11 | Swarm collision prevention | High (for claw-code) | High |
