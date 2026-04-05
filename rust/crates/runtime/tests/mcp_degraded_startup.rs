//! E4-5: MCP degraded-startup integration tests.
//!
//! These tests verify that when one or more MCP servers fail to start, the
//! runtime surfaces a structured [`McpDegradedReport`] rather than silently
//! swallowing errors or panicking.  Tests cover:
//!
//! 1. **Single-server failure** — the validator detects `SpawnConnect` failure
//!    and the degraded report contains the failed server with the expected error.
//! 2. **Multi-server partial failure** — some servers succeed, others fail; the
//!    degraded report correctly splits them into `working_servers` and
//!    `failed_servers`.
//! 3. **All-servers failure** — working_servers is empty, all tools are listed
//!    as missing.
//! 4. **Timeout at SpawnConnect** — a timeout result is treated as recoverable
//!    and flows into the failed-server list.
//! 5. **Failure after successful config load** — only SpawnConnect fails, not
//!    ConfigLoad; the report correctly identifies the failing phase.
//! 6. **JSON serializability of degraded report** — the report is fully
//!    serializable to JSON without loss of data.

use std::collections::BTreeMap;
use std::time::Duration;

use runtime::{
    McpDegradedReport, McpErrorSurface, McpFailedServer, McpLifecyclePhase, McpLifecycleValidator,
    McpPhaseResult,
};

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn spawn_error(server: &str, message: &str) -> McpErrorSurface {
    McpErrorSurface::new(
        McpLifecyclePhase::SpawnConnect,
        Some(server.to_string()),
        message,
        BTreeMap::from([("server".to_string(), server.to_string())]),
        false,
    )
}

fn failed_server(name: &str, phase: McpLifecyclePhase, error: McpErrorSurface) -> McpFailedServer {
    McpFailedServer {
        server_name: name.to_string(),
        phase,
        error,
    }
}

fn run_to_spawn(validator: &mut McpLifecycleValidator) {
    validator.run_phase(McpLifecyclePhase::ConfigLoad);
    validator.run_phase(McpLifecyclePhase::ServerRegistration);
}

// ──────────────────────────────────────────────────────────────────────────────
// 1. Single-server failure
// ──────────────────────────────────────────────────────────────────────────────

/// When the only configured server fails to spawn, the degraded report must
/// contain that server in `failed_servers` and leave `working_servers` empty.
#[test]
fn single_server_spawn_failure_produces_degraded_report_with_no_working_servers() {
    // given — validator has reached ServerRegistration for "alpha"
    let mut validator = McpLifecycleValidator::new();
    run_to_spawn(&mut validator);

    // when — SpawnConnect fails
    let error = spawn_error("alpha", "connection refused: port 9000");
    let result = validator.record_failure(error.clone());

    // then — result is a Failure
    match &result {
        McpPhaseResult::Failure { phase, error: e } => {
            assert_eq!(*phase, McpLifecyclePhase::SpawnConnect);
            assert_eq!(e.server_name.as_deref(), Some("alpha"));
            assert!(!e.recoverable);
        }
        other => panic!("expected Failure, got {other:?}"),
    }

    // and the validator moved into ErrorSurfacing
    assert_eq!(
        validator.state().current_phase(),
        Some(McpLifecyclePhase::ErrorSurfacing)
    );

    // build degraded report from the failure
    let report = McpDegradedReport::new(
        vec![],
        vec![failed_server(
            "alpha",
            McpLifecyclePhase::SpawnConnect,
            error,
        )],
        vec![],
        vec!["search".to_string(), "read".to_string()],
    );

    assert!(report.working_servers.is_empty());
    assert_eq!(report.failed_servers.len(), 1);
    assert_eq!(report.failed_servers[0].server_name, "alpha");
    assert_eq!(
        report.failed_servers[0].phase,
        McpLifecyclePhase::SpawnConnect
    );
    assert_eq!(
        report.missing_tools,
        vec!["read".to_string(), "search".to_string()]
    );
    assert!(report.available_tools.is_empty());
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. Multi-server partial failure
// ──────────────────────────────────────────────────────────────────────────────

/// When some servers succeed and one fails, the degraded report correctly
/// partitions the server list and only marks tools from the failed server as
/// missing.
#[test]
fn partial_server_failure_splits_working_and_failed_servers() {
    // given
    let error = spawn_error("beta", "handshake timeout");

    // when — alpha and gamma succeeded; beta failed
    let report = McpDegradedReport::new(
        vec!["alpha".to_string(), "gamma".to_string()],
        vec![failed_server(
            "beta",
            McpLifecyclePhase::SpawnConnect,
            error,
        )],
        vec!["search".to_string(), "read".to_string()], // discovered from alpha+gamma
        vec![
            "search".to_string(),
            "read".to_string(),
            "write".to_string(),
        ], // write was on beta
    );

    // then
    assert_eq!(
        report.working_servers,
        vec!["alpha".to_string(), "gamma".to_string()]
    );
    assert_eq!(report.failed_servers.len(), 1);
    assert_eq!(report.failed_servers[0].server_name, "beta");
    assert_eq!(
        report.available_tools,
        vec!["read".to_string(), "search".to_string()]
    );
    assert_eq!(report.missing_tools, vec!["write".to_string()]);
}

// ──────────────────────────────────────────────────────────────────────────────
// 3. All-servers failure
// ──────────────────────────────────────────────────────────────────────────────

/// When every server fails, the degraded report has an empty working_servers
/// list and all expected tools are marked as missing.
#[test]
fn all_servers_failing_produces_empty_working_set_and_all_tools_missing() {
    // given
    let alpha_err = spawn_error("alpha", "port in use");
    let beta_err = spawn_error("beta", "binary not found");

    // when
    let report = McpDegradedReport::new(
        vec![],
        vec![
            failed_server("alpha", McpLifecyclePhase::SpawnConnect, alpha_err),
            failed_server("beta", McpLifecyclePhase::SpawnConnect, beta_err),
        ],
        vec![],
        vec![
            "search".to_string(),
            "write".to_string(),
            "read".to_string(),
        ],
    );

    // then
    assert!(report.working_servers.is_empty());
    assert!(report.available_tools.is_empty());
    assert_eq!(
        report.missing_tools,
        vec![
            "read".to_string(),
            "search".to_string(),
            "write".to_string()
        ]
    );
    assert_eq!(report.failed_servers.len(), 2);
}

// ──────────────────────────────────────────────────────────────────────────────
// 4. Timeout at SpawnConnect treated as recoverable
// ──────────────────────────────────────────────────────────────────────────────

/// A timeout recorded at SpawnConnect should be marked `recoverable = true`,
/// meaning the operator can retry without invalidating the lifecycle state.
#[test]
fn spawn_connect_timeout_is_marked_recoverable() {
    // given
    let mut validator = McpLifecycleValidator::new();
    run_to_spawn(&mut validator);
    let waited = Duration::from_millis(500);

    // when
    let result = validator.record_timeout(
        McpLifecyclePhase::SpawnConnect,
        waited,
        Some("slow-server".to_string()),
        BTreeMap::from([("attempt".to_string(), "1".to_string())]),
    );

    // then
    match result {
        McpPhaseResult::Timeout {
            phase,
            waited: actual,
            error,
        } => {
            assert_eq!(phase, McpLifecyclePhase::SpawnConnect);
            assert_eq!(actual, waited);
            assert!(
                error.recoverable,
                "timeout at SpawnConnect should be recoverable"
            );
            assert_eq!(error.server_name.as_deref(), Some("slow-server"));
        }
        other => panic!("expected Timeout, got {other:?}"),
    }

    // error is recorded in state
    let errors = validator
        .state()
        .errors_for_phase(McpLifecyclePhase::SpawnConnect);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].recoverable);
}

// ──────────────────────────────────────────────────────────────────────────────
// 5. Failure phase attribution
// ──────────────────────────────────────────────────────────────────────────────

/// A failure at InitializeHandshake (not SpawnConnect) should be attributed
/// to the handshake phase in the degraded report, distinguishing it from a
/// spawn failure.
#[test]
fn handshake_failure_phase_is_correctly_attributed_in_failed_server() {
    // given
    let handshake_error = McpErrorSurface::new(
        McpLifecyclePhase::InitializeHandshake,
        Some("alpha".to_string()),
        "protocol version mismatch",
        BTreeMap::from([("expected".to_string(), "2024-11-05".to_string())]),
        false,
    );

    // when
    let failed = failed_server(
        "alpha",
        McpLifecyclePhase::InitializeHandshake,
        handshake_error.clone(),
    );
    let report = McpDegradedReport::new(vec![], vec![failed], vec![], vec!["search".to_string()]);

    // then
    assert_eq!(
        report.failed_servers[0].phase,
        McpLifecyclePhase::InitializeHandshake
    );
    assert!(report.failed_servers[0]
        .error
        .message
        .contains("protocol version mismatch"));
    assert!(!report.failed_servers[0].error.recoverable);
}

// ──────────────────────────────────────────────────────────────────────────────
// 6. JSON serializability
// ──────────────────────────────────────────────────────────────────────────────

/// The degraded report must serialize to JSON without loss of data and must
/// deserialize back to an equal value.
#[test]
fn degraded_report_round_trips_through_json() {
    // given
    let error = McpErrorSurface::new(
        McpLifecyclePhase::SpawnConnect,
        Some("alpha".to_string()),
        "connection refused",
        BTreeMap::from([("port".to_string(), "9000".to_string())]),
        false,
    );
    let report = McpDegradedReport::new(
        vec!["gamma".to_string()],
        vec![failed_server(
            "alpha",
            McpLifecyclePhase::SpawnConnect,
            error,
        )],
        vec!["read".to_string()],
        vec!["read".to_string(), "search".to_string()],
    );

    // when
    let json = serde_json::to_string(&report).expect("serialize degraded report");
    let deserialized: McpDegradedReport =
        serde_json::from_str(&json).expect("deserialize degraded report");

    // then
    assert_eq!(report, deserialized);
    assert!(json.contains("connection refused"));
    assert!(json.contains("spawn_connect"));
    assert!(json.contains("alpha"));
    assert!(json.contains("gamma"));
}

// ──────────────────────────────────────────────────────────────────────────────
// 7. Degraded report deduplicates inputs
// ──────────────────────────────────────────────────────────────────────────────

/// If the caller passes duplicate server names or tool names, the degraded
/// report must deduplicate them.
#[test]
fn degraded_report_deduplicates_server_and_tool_lists() {
    // given / when
    let report = McpDegradedReport::new(
        vec![
            "alpha".to_string(),
            "gamma".to_string(),
            "alpha".to_string(),
        ],
        vec![],
        vec![
            "search".to_string(),
            "search".to_string(),
            "read".to_string(),
        ],
        vec!["read".to_string(), "write".to_string(), "write".to_string()],
    );

    // then
    assert_eq!(
        report.working_servers,
        vec!["alpha".to_string(), "gamma".to_string()]
    );
    assert_eq!(
        report.available_tools,
        vec!["read".to_string(), "search".to_string()]
    );
    assert_eq!(report.missing_tools, vec!["write".to_string()]);
}
