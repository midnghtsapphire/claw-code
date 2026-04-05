# Contributing to claw-code

Thank you for your interest in contributing! This document explains how to set up your local environment, the branch and PR conventions, the quality gate you need to pass, and how to run the mock-Anthropic parity harness before submitting.

---

## Table of contents

1. [Prerequisites](#prerequisites)
2. [Cloning and building](#cloning-and-building)
3. [Branch naming](#branch-naming)
4. [Making changes](#making-changes)
5. [Quality gate — fmt + clippy + test](#quality-gate)
6. [Running the mock-Anthropic parity harness](#running-the-parity-harness)
7. [Pull request process](#pull-request-process)
8. [Code of conduct](#code-of-conduct)

---

## Prerequisites

| Tool | Minimum version | Install |
|------|----------------|---------|
| Rust toolchain | stable (1.75+) | `rustup toolchain install stable` |
| `rustfmt` | bundled with stable | `rustup component add rustfmt` |
| `clippy` | bundled with stable | `rustup component add clippy` |
| `cargo` | bundled with Rust | — |

Optional but recommended:

```bash
# Speed up incremental builds
cargo install sccache
export RUSTC_WRAPPER=sccache
```

---

## Cloning and building

```bash
git clone https://github.com/midnghtsapphire/claw-code.git
cd claw-code/rust

# Build everything
cargo build --workspace

# Build just the CLI binary
cargo build -p rusty-claude-cli
```

The compiled binary is at `rust/target/debug/claw`.

---

## Branch naming

| Prefix | Use for |
|--------|---------|
| `feat/` | New features or capabilities |
| `fix/` | Bug fixes |
| `copilot/` | Copilot / automation-driven branches |
| `chore/` | Housekeeping — CI, deps, docs |
| `test/` | Pure test additions or improvements |

Examples:

```
feat/mcp-json-output
fix/doctor-exit-code
copilot/sprint-3-config-precedence
```

Keep branch names lowercase, hyphen-separated, and concise (≤ 50 characters).

---

## Making changes

1. Create your branch from `main`:
   ```bash
   git switch -c feat/my-change
   ```

2. Make your changes. Keep commits small and atomic — one logical change per commit.

3. Add or update tests for every behaviour change (see [Quality gate](#quality-gate)).

4. Update documentation if the user-facing interface changes (CLI flags, output format, config schema).

---

## Quality gate

All pull requests must pass the three-step gate that CI enforces:

```bash
cd rust

# 1. Formatting — zero diff
cargo fmt --all --check

# 2. Lint — zero errors, warnings promoted to errors
cargo clippy --workspace --all-targets -- -D warnings

# 3. Tests — all pass, skip hook-runner CI restriction
cargo test --workspace -- --skip collects_and_runs_hooks_from_enabled_plugins
```

Run these locally before pushing — CI will reject your PR if any step fails.

> **Tip:** `cargo fmt --all` (without `--check`) auto-fixes formatting in place. Run it before committing.

### What CI checks

| Job | Command |
|-----|---------|
| `cargo fmt` | `cargo fmt --all --check` |
| `cargo test -p rusty-claude-cli` | full crate test suite |
| `cargo test --workspace` | all crates (with hook-runner skip) |

---

## Running the parity harness

The mock-Anthropic parity harness (`tests/mock_parity_harness.rs`) runs the CLI binary against a local HTTP server that mimics the Anthropic API. It validates that claw's conversation loop, tool calls, and session handling stay compatible with expected behaviour.

### Starting the mock server standalone

```bash
# Build the mock server
cargo build -p mock-anthropic-service

# Run it (prints MOCK_ANTHROPIC_BASE_URL to stdout)
./target/debug/mock-anthropic-service
# Output: MOCK_ANTHROPIC_BASE_URL=http://127.0.0.1:<port>
```

You can then point any claw command at it:

```bash
export ANTHROPIC_BASE_URL=http://127.0.0.1:<port>
export ANTHROPIC_API_KEY=mock-key
./target/debug/claw "hello"
```

### Running the full harness

```bash
cd rust
cargo test -p rusty-claude-cli --test mock_parity_harness
```

The harness starts its own mock server internally — you don't need to launch it manually. Each scenario in `tests/mock_parity_harness.rs` sets `ANTHROPIC_BASE_URL` automatically.

### Adding new parity scenarios

1. Add a YAML scenario file under `rust/crates/rusty-claude-cli/tests/` (follow the existing `.yaml` files for structure).
2. The harness discovers and runs it automatically.
3. Verify locally with the command above before opening a PR.

---

## Pull request process

1. Push your branch and open a PR against `main`.
2. Fill in the PR description — explain *what* changed and *why*.
3. Ensure all CI jobs are green (fmt, clippy, tests).
4. Request a review from a maintainer.
5. Squash or rebase your branch before merging if the maintainer requests it.

PRs that change the JSON output format of any `--output-format json` command **must** update the corresponding contract test in `rust/crates/rusty-claude-cli/tests/json_contract.rs`.

PRs that add or modify a built-in tool spec **must** verify the schema validation test in `rust/crates/tools/tests/tool_spec_schema.rs` still passes.

---

## Code of conduct

Be respectful, constructive, and kind. We're all here to build something good.
