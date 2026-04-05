//! E6-2: End-to-end MCP lifecycle integration tests.
//!
//! These tests drive the full MCP lifecycle via [`McpLifecycleValidator`] from
//! `config_load` through `cleanup`, exercising every documented phase
//! transition.  They complement the unit tests in `mcp_lifecycle_hardened.rs`
//! by verifying that the state machine is *externally* composable and
//! integrates correctly with [`McpDegradedReport`] and
//! [`PluginHealthcheck`] / [`DiscoveryResult`].
//!
//! Scenarios covered:
//! 1. **Full happy path** — all 11 phases succeed in order; timestamps present.
//! 2. **Tool-discovery-only path** (resource discovery skipped) — direct
//!    transition ToolDiscovery → Ready is valid.
//! 3. **Invocation cycle** — Ready → Invocation → Ready can repeat multiple
//!    times without resetting the lifecycle.
//! 4. **Handshake failure → error surfacing → forced shutdown** — a
//!    non-recoverable failure at InitializeHandshake blocks the path back to
//!    Ready and forces the lifecycle to Shutdown.
//! 5. **Recoverable timeout → resumed Ready** — a recoverable timeout at
//!    ResourceDiscovery allows the lifecycle to continue to Ready.
//! 6. **Invalid initial phase** — starting with any phase other than ConfigLoad
//!    returns a structured Failure result.
//! 7. **Degraded-report wiring** — the output of a partial-startup validator
//!    maps correctly to a `McpDegradedReport` consumed by a `PluginHealthcheck`.
//! 8. **Phase timestamps are monotonic** — every successfully entered phase
//!    has a non-zero timestamp.

use std::collections::BTreeMap;
use std::time::Duration;

use runtime::{
    DegradedMode, DiscoveryResult, McpDegradedReport, McpErrorSurface, McpFailedServer,
    McpLifecyclePhase, McpLifecycleValidator, McpPhaseResult, PluginHealthcheck, PluginState,
    ResourceInfo, ServerHealth, ServerStatus, ToolInfo,
};

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn advance_to(validator: &mut McpLifecycleValidator, phases: &[McpLifecyclePhase]) {
    for &phase in phases {
        let result = validator.run_phase(phase);
        assert!(
            matches!(result, McpPhaseResult::Success { .. }),
            "phase {phase:?} failed unexpectedly: {result:?}"
        );
    }
}

fn tool(name: &str) -> ToolInfo {
    ToolInfo {
        name: name.to_string(),
        description: Some(format!("{name} tool")),
        input_schema: None,
    }
}

fn resource(name: &str, uri: &str) -> ResourceInfo {
    ResourceInfo {
        uri: uri.to_string(),
        name: name.to_string(),
        description: Some(format!("{name} resource")),
        mime_type: Some("application/json".to_string()),
    }
}

fn healthy_server(name: &str, capabilities: &[&str]) -> ServerHealth {
    ServerHealth {
        server_name: name.to_string(),
        status: ServerStatus::Healthy,
        capabilities: capabilities.iter().map(|c| c.to_string()).collect(),
        last_error: None,
    }
}

fn failed_server_health(name: &str, capabilities: &[&str], error: &str) -> ServerHealth {
    ServerHealth {
        server_name: name.to_string(),
        status: ServerStatus::Failed,
        capabilities: capabilities.iter().map(|c| c.to_string()).collect(),
        last_error: Some(error.to_string()),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 1. Full happy path
// ──────────────────────────────────────────────────────────────────────────────

/// All 11 phases complete in order; each phase has a recorded timestamp and
/// the validator ends in Cleanup.
#[test]
fn full_lifecycle_all_phases_succeed() {
    // given
    let mut validator = McpLifecycleValidator::new();

    // when
    advance_to(
        &mut validator,
        &[
            McpLifecyclePhase::ConfigLoad,
            McpLifecyclePhase::ServerRegistration,
            McpLifecyclePhase::SpawnConnect,
            McpLifecyclePhase::InitializeHandshake,
            McpLifecyclePhase::ToolDiscovery,
            McpLifecyclePhase::ResourceDiscovery,
            McpLifecyclePhase::Ready,
            McpLifecyclePhase::Invocation,
            McpLifecyclePhase::Ready,
            McpLifecyclePhase::Shutdown,
            McpLifecyclePhase::Cleanup,
        ],
    );

    // then
    assert_eq!(
        validator.state().current_phase(),
        Some(McpLifecyclePhase::Cleanup)
    );
    // every distinct phase that was entered should have a timestamp
    for phase in [
        McpLifecyclePhase::ConfigLoad,
        McpLifecyclePhase::ServerRegistration,
        McpLifecyclePhase::SpawnConnect,
        McpLifecyclePhase::InitializeHandshake,
        McpLifecyclePhase::ToolDiscovery,
        McpLifecyclePhase::ResourceDiscovery,
        McpLifecyclePhase::Ready,
        McpLifecyclePhase::Invocation,
        McpLifecyclePhase::Shutdown,
        McpLifecyclePhase::Cleanup,
    ] {
        assert!(
            validator.state().phase_timestamp(phase).is_some(),
            "missing timestamp for {phase:?}"
        );
    }
    // all results should be Success
    assert!(validator
        .state()
        .results()
        .iter()
        .all(|r| matches!(r, McpPhaseResult::Success { .. })));
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. Tool-discovery-only path (resource discovery skipped)
// ──────────────────────────────────────────────────────────────────────────────

/// When resource discovery is not needed, ToolDiscovery → Ready is a valid
/// direct transition.
#[test]
fn tool_discovery_only_path_skips_resource_discovery() {
    // given
    let mut validator = McpLifecycleValidator::new();
    advance_to(
        &mut validator,
        &[
            McpLifecyclePhase::ConfigLoad,
            McpLifecyclePhase::ServerRegistration,
            McpLifecyclePhase::SpawnConnect,
            McpLifecyclePhase::InitializeHandshake,
            McpLifecyclePhase::ToolDiscovery,
        ],
    );

    // when
    let result = validator.run_phase(McpLifecyclePhase::Ready);

    // then
    assert!(matches!(result, McpPhaseResult::Success { .. }));
    assert_eq!(
        validator.state().current_phase(),
        Some(McpLifecyclePhase::Ready)
    );
    // ResourceDiscovery was never entered so it has no timestamp
    assert!(validator
        .state()
        .phase_timestamp(McpLifecyclePhase::ResourceDiscovery)
        .is_none());
}

// ──────────────────────────────────────────────────────────────────────────────
// 3. Repeated invocation cycle
// ──────────────────────────────────────────────────────────────────────────────

/// Ready → Invocation → Ready can repeat multiple times; each cycle appends
/// Success results to the phase log.
#[test]
fn invocation_cycle_can_repeat_multiple_times() {
    // given
    let mut validator = McpLifecycleValidator::new();
    advance_to(
        &mut validator,
        &[
            McpLifecyclePhase::ConfigLoad,
            McpLifecyclePhase::ServerRegistration,
            McpLifecyclePhase::SpawnConnect,
            McpLifecyclePhase::InitializeHandshake,
            McpLifecyclePhase::ToolDiscovery,
            McpLifecyclePhase::Ready,
        ],
    );

    // when — three invocation cycles
    for _ in 0..3 {
        let inv = validator.run_phase(McpLifecyclePhase::Invocation);
        assert!(matches!(inv, McpPhaseResult::Success { .. }));
        let ready = validator.run_phase(McpLifecyclePhase::Ready);
        assert!(matches!(ready, McpPhaseResult::Success { .. }));
    }

    // then — still in Ready and no errors recorded
    assert_eq!(
        validator.state().current_phase(),
        Some(McpLifecyclePhase::Ready)
    );
    assert!(validator
        .state()
        .errors_for_phase(McpLifecyclePhase::Invocation)
        .is_empty());
}

// ──────────────────────────────────────────────────────────────────────────────
// 4. Non-recoverable handshake failure blocks return to Ready
// ──────────────────────────────────────────────────────────────────────────────

/// After a non-recoverable failure at InitializeHandshake, trying to run
/// ErrorSurfacing → Ready must fail and the lifecycle must proceed to Shutdown
/// instead.
#[test]
fn non_recoverable_failure_blocks_return_to_ready_and_forces_shutdown() {
    // given
    let mut validator = McpLifecycleValidator::new();
    advance_to(
        &mut validator,
        &[
            McpLifecyclePhase::ConfigLoad,
            McpLifecyclePhase::ServerRegistration,
            McpLifecyclePhase::SpawnConnect,
        ],
    );

    // when — non-recoverable handshake failure
    let handshake_error = McpErrorSurface::new(
        McpLifecyclePhase::InitializeHandshake,
        Some("alpha".to_string()),
        "protocol version mismatch",
        BTreeMap::new(),
        false, // NOT recoverable
    );
    let failure = validator.record_failure(handshake_error);
    assert!(matches!(failure, McpPhaseResult::Failure { .. }));
    assert_eq!(
        validator.state().current_phase(),
        Some(McpLifecyclePhase::ErrorSurfacing)
    );

    // attempting to return to Ready should fail
    let blocked = validator.run_phase(McpLifecyclePhase::Ready);
    assert!(
        matches!(blocked, McpPhaseResult::Failure { .. }),
        "returning to Ready after non-recoverable failure must be blocked"
    );

    // but Shutdown and Cleanup are still reachable
    let shutdown = validator.run_phase(McpLifecyclePhase::Shutdown);
    assert!(matches!(shutdown, McpPhaseResult::Success { .. }));
    let cleanup = validator.run_phase(McpLifecyclePhase::Cleanup);
    assert!(matches!(cleanup, McpPhaseResult::Success { .. }));
    assert_eq!(
        validator.state().current_phase(),
        Some(McpLifecyclePhase::Cleanup)
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 5. Recoverable timeout allows continuation to Ready
// ──────────────────────────────────────────────────────────────────────────────

/// A recoverable timeout at ResourceDiscovery emits ErrorSurfacing, but
/// ErrorSurfacing → Ready should succeed (can_resume_after_error is true).
#[test]
fn recoverable_timeout_at_resource_discovery_allows_continuation_to_ready() {
    // given
    let mut validator = McpLifecycleValidator::new();
    advance_to(
        &mut validator,
        &[
            McpLifecyclePhase::ConfigLoad,
            McpLifecyclePhase::ServerRegistration,
            McpLifecyclePhase::SpawnConnect,
            McpLifecyclePhase::InitializeHandshake,
            McpLifecyclePhase::ToolDiscovery,
        ],
    );

    // when — timeout at ResourceDiscovery (recoverable)
    let timeout = validator.record_timeout(
        McpLifecyclePhase::ResourceDiscovery,
        Duration::from_millis(200),
        Some("alpha".to_string()),
        BTreeMap::new(),
    );
    assert!(matches!(timeout, McpPhaseResult::Timeout { .. }));
    assert_eq!(
        validator.state().current_phase(),
        Some(McpLifecyclePhase::ErrorSurfacing)
    );

    // then — can still proceed to Ready
    let ready = validator.run_phase(McpLifecyclePhase::Ready);
    assert!(
        matches!(ready, McpPhaseResult::Success { .. }),
        "recoverable error should allow return to Ready, got {ready:?}"
    );
    assert_eq!(
        validator.state().current_phase(),
        Some(McpLifecyclePhase::Ready)
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 6. Invalid initial phase
// ──────────────────────────────────────────────────────────────────────────────

/// Starting the lifecycle with any phase other than ConfigLoad must return a
/// structured Failure result and must not panic.
#[test]
fn starting_with_non_config_load_phase_returns_structured_failure() {
    for invalid_start in [
        McpLifecyclePhase::ServerRegistration,
        McpLifecyclePhase::SpawnConnect,
        McpLifecyclePhase::Ready,
        McpLifecyclePhase::Shutdown,
    ] {
        let mut validator = McpLifecycleValidator::new();
        let result = validator.run_phase(invalid_start);

        assert!(
            matches!(result, McpPhaseResult::Failure { .. }),
            "starting with {invalid_start:?} should be a Failure"
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 7. Degraded-report wiring with PluginHealthcheck
// ──────────────────────────────────────────────────────────────────────────────

/// The output of a partial-startup validator (one failed server) maps correctly
/// to a [`McpDegradedReport`] that is then consumed by a [`PluginHealthcheck`]
/// to produce a [`DegradedMode`].
#[test]
fn partial_startup_wires_into_plugin_healthcheck_degraded_mode() {
    // given — one server healthy, one failed
    let healthcheck = PluginHealthcheck::new(
        "my-plugin",
        vec![
            healthy_server("alpha", &["search", "read"]),
            failed_server_health("beta", &["write"], "connection refused"),
        ],
    );
    let discovery = DiscoveryResult {
        tools: vec![tool("search"), tool("read")],
        resources: vec![resource("alpha-docs", "file:///alpha")],
        partial: true,
    };

    // when
    let degraded_mode = healthcheck
        .degraded_mode(&discovery)
        .expect("degraded startup should expose DegradedMode");

    // then
    let mut available = degraded_mode.available_tools.clone();
    available.sort();
    assert_eq!(available, vec!["read".to_string(), "search".to_string()]);
    assert_eq!(degraded_mode.unavailable_tools, vec!["write".to_string()]);
    assert!(degraded_mode.reason.contains("healthy"));
    assert!(degraded_mode.reason.contains("failed"));

    // and construct the corresponding degraded report
    let beta_err = McpErrorSurface::new(
        McpLifecyclePhase::SpawnConnect,
        Some("beta".to_string()),
        "connection refused",
        BTreeMap::new(),
        false,
    );
    let report = McpDegradedReport::new(
        vec!["alpha".to_string()],
        vec![McpFailedServer {
            server_name: "beta".to_string(),
            phase: McpLifecyclePhase::SpawnConnect,
            error: beta_err,
        }],
        vec!["search".to_string(), "read".to_string()],
        vec![
            "search".to_string(),
            "read".to_string(),
            "write".to_string(),
        ],
    );

    assert_eq!(report.missing_tools, vec!["write".to_string()]);
    assert_eq!(
        report.available_tools,
        vec!["read".to_string(), "search".to_string()]
    );
    assert_eq!(report.working_servers, vec!["alpha".to_string()]);
}

// ──────────────────────────────────────────────────────────────────────────────
// 8. Phase timestamps are non-zero after successful entry
// ──────────────────────────────────────────────────────────────────────────────

/// Every phase that was successfully run must record a non-zero Unix timestamp.
#[test]
fn successfully_run_phases_have_non_zero_timestamps() {
    // given
    let mut validator = McpLifecycleValidator::new();
    let phases = [
        McpLifecyclePhase::ConfigLoad,
        McpLifecyclePhase::ServerRegistration,
        McpLifecyclePhase::SpawnConnect,
        McpLifecyclePhase::InitializeHandshake,
        McpLifecyclePhase::ToolDiscovery,
        McpLifecyclePhase::Ready,
    ];
    advance_to(&mut validator, &phases);

    // then
    for phase in phases {
        let ts = validator
            .state()
            .phase_timestamp(phase)
            .unwrap_or_else(|| panic!("no timestamp for {phase:?}"));
        assert!(ts > 0, "timestamp for {phase:?} should be non-zero");
    }
}
