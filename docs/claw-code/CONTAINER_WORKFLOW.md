# Container Workflow — claw-code

This guide covers building, testing, and running the claw-code CLI binary inside a Docker or Podman container. Use it to get a reproducible environment without installing a Rust toolchain locally, or to replicate the exact CI build on your machine.

---

## Table of contents

1. [Prerequisites](#prerequisites)
2. [Quick start](#quick-start)
3. [Build the image](#build-the-image)
4. [Run tests inside the container](#run-tests-inside-the-container)
5. [Run the CLI binary](#run-the-cli-binary)
6. [Cross-compile release builds](#cross-compile-release-builds)
7. [Using Podman instead of Docker](#using-podman-instead-of-docker)
8. [Useful aliases](#useful-aliases)
9. [Troubleshooting](#troubleshooting)

---

## Prerequisites

Install **either**:

- [Docker Engine](https://docs.docker.com/engine/install/) ≥ 24  
- [Podman](https://podman.io/getting-started/installation) ≥ 4 (drop-in replacement — all commands are identical after you alias `docker=podman`)

No Rust toolchain required on the host.

---

## Quick start

```bash
# Build the dev image and run workspace tests in one step
docker build -t claw-dev .
docker run --rm claw-dev cargo test --workspace -- --skip collects_and_runs_hooks_from_enabled_plugins
```

If you don't have a `Dockerfile` in the repo root yet, use the one-liner recipe below to get a working image:

```bash
docker run --rm \
  -v "$(pwd)/rust":/workspace \
  -w /workspace \
  rust:1.78-slim \
  cargo test --workspace -- --skip collects_and_runs_hooks_from_enabled_plugins
```

---

## Build the image

Create a minimal `Dockerfile` in the repo root if one does not already exist:

```dockerfile
FROM rust:1.78-slim AS builder

WORKDIR /build

# Cache dependency downloads before copying sources
COPY rust/Cargo.toml rust/Cargo.lock ./
COPY rust/crates ./crates
RUN cargo fetch

# Build everything
RUN cargo build --workspace --release

# ── Runtime image ──────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/claw /usr/local/bin/claw

ENTRYPOINT ["claw"]
```

Build it:

```bash
# Docker
docker build -t claw-code:latest .

# Podman
podman build -t claw-code:latest .
```

Tag a specific version:

```bash
docker build -t claw-code:0.1.0 .
```

---

## Run tests inside the container

### Workspace tests (Rust)

```bash
# Mount the repo into a stock Rust image and run all workspace tests
docker run --rm \
  -v "$(pwd)/rust":/workspace \
  -w /workspace \
  rust:1.78-slim \
  cargo test --workspace -- --skip collects_and_runs_hooks_from_enabled_plugins
```

### Single-crate tests

```bash
docker run --rm \
  -v "$(pwd)/rust":/workspace \
  -w /workspace \
  rust:1.78-slim \
  cargo test -p rusty-claude-cli
```

### Python unit tests

```bash
docker run --rm \
  -v "$(pwd)":/workspace \
  -w /workspace \
  python:3.11-slim \
  python3 -m unittest discover -s tests -p "test_*.py"
```

### Format check + clippy (CI parity)

```bash
docker run --rm \
  -v "$(pwd)/rust":/workspace \
  -w /workspace \
  rust:1.78-slim \
  sh -c "cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings"
```

---

## Run the CLI binary

Mount a project directory and pass it as the working directory:

```bash
# Interactive — open a shell inside the container
docker run --rm -it \
  -v "$(pwd)":/project \
  -w /project \
  -e ANTHROPIC_API_KEY="${ANTHROPIC_API_KEY}" \
  claw-code:latest \
  --help

# Non-interactive — run a single command
docker run --rm \
  -v "$(pwd)":/project \
  -w /project \
  -e ANTHROPIC_API_KEY="${ANTHROPIC_API_KEY}" \
  claw-code:latest \
  status --output-format json
```

### Environment variables

| Variable | Purpose |
|----------|---------|
| `ANTHROPIC_API_KEY` | Required for live API calls |
| `ANTHROPIC_BASE_URL` | Override the API endpoint (e.g. point at mock server) |
| `CLAW_CONFIG_DIR` | Override the user config directory (default `~/.claw`) |

---

## Cross-compile release builds

Use `cross` (a Rust cross-compilation tool) inside Docker to produce binaries for all three release targets:

```bash
# Install cross (host machine, one-time)
cargo install cross

# Linux x86_64 (same as CI)
cross build -p rusty-claude-cli --release --target x86_64-unknown-linux-gnu

# macOS x86_64
cross build -p rusty-claude-cli --release --target x86_64-apple-darwin

# Windows x86_64
cross build -p rusty-claude-cli --release --target x86_64-pc-windows-msvc
```

`cross` automatically pulls the correct sysroot image from Docker Hub for each target. The binaries land in:

```
rust/target/<target>/release/claw[.exe]
```

---

## Using Podman instead of Docker

All commands above work unchanged with Podman. Either alias it:

```bash
alias docker=podman
```

Or substitute `podman` for `docker` directly:

```bash
podman build -t claw-code:latest .
podman run --rm claw-code:latest --help
```

Rootless Podman (default on most Linux distros) works without any additional flags.

---

## Useful aliases

Add these to your shell profile for daily use:

```bash
# Build the dev image
alias claw-docker-build='docker build -t claw-dev .'

# Run workspace tests
alias claw-docker-test='docker run --rm -v "$(pwd)/rust":/workspace -w /workspace rust:1.78-slim cargo test --workspace -- --skip collects_and_runs_hooks_from_enabled_plugins'

# Drop into a build shell
alias claw-docker-shell='docker run --rm -it -v "$(pwd)/rust":/workspace -w /workspace rust:1.78-slim bash'
```

---

## Troubleshooting

### `permission denied` when mounting volumes

On Linux with rootless Podman, add `--userns=keep-id`:

```bash
podman run --rm --userns=keep-id \
  -v "$(pwd)/rust":/workspace \
  -w /workspace \
  rust:1.78-slim \
  cargo test --workspace
```

### Build cache is slow

Use Docker BuildKit and mount the Cargo registry as a cache:

```bash
DOCKER_BUILDKIT=1 docker build \
  --mount=type=cache,target=/usr/local/cargo/registry \
  -t claw-code:latest .
```

Or with the `--mount` flag at build time (BuildKit syntax in Dockerfile):

```dockerfile
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --workspace --release
```

### Tests fail with `EPIPE / Broken pipe`

The hook-runner test spawns child shell processes that don't survive the container PID namespace. Skip it exactly as CI does:

```bash
cargo test --workspace -- --skip collects_and_runs_hooks_from_enabled_plugins
```
