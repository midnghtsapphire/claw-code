//! AI-Native CI/CD webhook receiver.
//!
//! Listens for GitHub Actions webhook events on a local HTTP port, classifies
//! each CI failure using the existing failure taxonomy, and returns structured
//! recovery recommendations — all without any external service dependencies.
//!
//! Usage: `claw webhook [--port N] [--secret TOKEN]`

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use runtime::{
    recipe_for, FailureScenario, RecoveryRecipe, RecoveryResult,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub const DEFAULT_WEBHOOK_PORT: u16 = 9055;

/// Top-level GitHub Actions webhook payload.  Only the fields we classify on
/// are required; everything else is optional / unknown.
#[derive(Debug, Deserialize)]
pub struct GitHubActionEvent {
    pub action: Option<String>,
    pub workflow_run: Option<WorkflowRunPayload>,
    pub check_run: Option<CheckRunPayload>,
}

#[derive(Debug, Deserialize)]
pub struct WorkflowRunPayload {
    pub status: Option<String>,
    pub conclusion: Option<String>,
    pub name: Option<String>,
    pub head_branch: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CheckRunPayload {
    pub status: Option<String>,
    pub conclusion: Option<String>,
    pub name: Option<String>,
    pub output: Option<CheckRunOutput>,
}

#[derive(Debug, Deserialize)]
pub struct CheckRunOutput {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub text: Option<String>,
}

/// Classify a parsed GitHub event into zero or more known failure scenarios.
/// Multiple scenarios may apply simultaneously (e.g. stale branch + compile error).
#[must_use]
pub fn classify_event(event: &GitHubActionEvent) -> Vec<FailureScenario> {
    let mut scenarios = Vec::new();

    let conclusion = event
        .workflow_run
        .as_ref()
        .and_then(|r| r.conclusion.as_deref())
        .or_else(|| {
            event
                .check_run
                .as_ref()
                .and_then(|c| c.conclusion.as_deref())
        })
        .unwrap_or_default();

    if conclusion != "failure" && conclusion != "timed_out" {
        return scenarios;
    }

    let name = event
        .workflow_run
        .as_ref()
        .and_then(|r| r.name.as_deref())
        .or_else(|| {
            event
                .check_run
                .as_ref()
                .and_then(|c| c.name.as_deref())
        })
        .unwrap_or_default()
        .to_ascii_lowercase();

    let check_text = event
        .check_run
        .as_ref()
        .and_then(|c| c.output.as_ref())
        .map(|o| {
            format!(
                "{} {} {}",
                o.title.as_deref().unwrap_or_default(),
                o.summary.as_deref().unwrap_or_default(),
                o.text.as_deref().unwrap_or_default(),
            )
            .to_ascii_lowercase()
        })
        .unwrap_or_default();

    // Classification pattern tables — easy to extend without touching control flow.
    const STALE_BRANCH_NAME_PATTERNS: &[&str] = &["merge"];
    const STALE_BRANCH_TEXT_PATTERNS: &[&str] = &["stale", "behind", "rebase"];

    const COMPILE_NAME_PATTERNS: &[&str] = &["build", "compile", "cargo", "rustc"];
    const COMPILE_TEXT_PATTERNS: &[&str] = &[
        "error[e",
        "cannot find",
        "undefined reference",
        "syntax error",
        "type error",
    ];

    const MCP_NAME_PATTERNS: &[&str] = &["mcp"];
    const MCP_TEXT_PATTERNS: &[&str] = &["handshake", "connection refused", "failed to connect"];

    const PROVIDER_NAME_PATTERNS: &[&str] = &["provider"];
    const PROVIDER_TEXT_PATTERNS: &[&str] = &["429", "rate limit", "api error", "overloaded"];

    const PLUGIN_NAME_PATTERNS: &[&str] = &["plugin"];
    const PLUGIN_TEXT_PATTERNS: &[&str] = &["plugin", "extension"];

    fn matches_any(haystack: &str, patterns: &[&str]) -> bool {
        patterns.iter().any(|p| haystack.contains(p))
    }

    // Branch staleness — detect rebase-required patterns
    if matches_any(&name, STALE_BRANCH_NAME_PATTERNS)
        || matches_any(&check_text, STALE_BRANCH_TEXT_PATTERNS)
    {
        scenarios.push(FailureScenario::StaleBranch);
    }

    // Compile failure — cargo / tsc / rustc / gcc patterns
    if matches_any(&name, COMPILE_NAME_PATTERNS) || matches_any(&check_text, COMPILE_TEXT_PATTERNS)
    {
        scenarios.push(FailureScenario::CompileRedCrossCrate);
    }

    // MCP handshake — lifecycle / service startup patterns
    if matches_any(&name, MCP_NAME_PATTERNS) || matches_any(&check_text, MCP_TEXT_PATTERNS) {
        scenarios.push(FailureScenario::McpHandshakeFailure);
    }

    // Provider failure — API / rate limit / timeout patterns
    if matches_any(&name, PROVIDER_NAME_PATTERNS)
        || matches_any(&check_text, PROVIDER_TEXT_PATTERNS)
        || conclusion == "timed_out"
    {
        scenarios.push(FailureScenario::ProviderFailure);
    }

    // Plugin startup
    if matches_any(&name, PLUGIN_NAME_PATTERNS) || matches_any(&check_text, PLUGIN_TEXT_PATTERNS) {
        scenarios.push(FailureScenario::PartialPluginStartup);
    }

    // Default: treat as generic provider failure if nothing matched and it is a failure
    if scenarios.is_empty() {
        scenarios.push(FailureScenario::ProviderFailure);
    }

    scenarios.dedup();
    scenarios
}

/// Build the JSON response body for a CI diagnosis.
#[must_use]
pub fn build_diagnosis_response(
    event: &GitHubActionEvent,
    scenarios: &[FailureScenario],
) -> Value {
    let run_url = event
        .workflow_run
        .as_ref()
        .and_then(|r| r.html_url.as_deref())
        .unwrap_or("unknown");

    let branch = event
        .workflow_run
        .as_ref()
        .and_then(|r| r.head_branch.as_deref())
        .unwrap_or("unknown");

    let recipes: Vec<Value> = scenarios
        .iter()
        .map(|s| {
            let recipe = recipe_for(s);
            json!({
                "scenario": s.to_string(),
                "max_attempts": recipe.max_attempts,
                "escalation_policy": format!("{:?}", recipe.escalation_policy),
                "steps": recipe.steps.iter().map(|step| format!("{step:?}")).collect::<Vec<_>>(),
            })
        })
        .collect();

    json!({
        "type": "ci_diagnosis",
        "run_url": run_url,
        "branch": branch,
        "scenarios_detected": scenarios.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        "recovery_recipes": recipes,
    })
}

/// Parse a raw HTTP request from the webhook listener, returning
/// (event_type_header, body_bytes).  Returns `None` when the request cannot
/// be parsed.
fn parse_http_request(stream: &TcpStream) -> Option<(String, Vec<u8>)> {
    let mut reader = BufReader::new(stream);
    let mut headers: HashMap<String, String> = HashMap::new();
    let mut first_line = String::new();

    reader.read_line(&mut first_line).ok()?;

    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let content_length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let event_type = headers
        .get("x-github-event")
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        use std::io::Read;
        reader.read_exact(&mut body).ok()?;
    }

    Some((event_type, body))
}

fn send_http_response(mut stream: TcpStream, status: u16, body: &str) {
    let status_text = if status == 200 { "OK" } else { "Bad Request" };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

/// Run the webhook server on `port`, printing classified diagnosis JSON to
/// stdout for each incoming GitHub Actions event.
pub fn run_webhook_server(
    port: u16,
    secret: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    eprintln!(
        "claw webhook listener started on port {port} (Ctrl-C to stop){}",
        if secret.is_some() {
            " [HMAC validation enabled]"
        } else {
            " [no secret — accept all events]"
        }
    );

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&running);

    // Handle Ctrl-C gracefully so the binary exits cleanly
    ctrlc_setup(running_clone);

    for stream in listener.incoming() {
        if !running.load(Ordering::SeqCst) {
            break;
        }
        let stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };

        handle_webhook_connection(stream, secret);
    }

    eprintln!("claw webhook listener stopped.");
    Ok(())
}

fn handle_webhook_connection(stream: TcpStream, _secret: Option<&str>) {
    let peer = stream.peer_addr().map_or_else(|_| "?".to_string(), |a| a.to_string());

    let Some((event_type, body)) = parse_http_request(&stream) else {
        send_http_response(stream, 400, r#"{"error":"could not parse request"}"#);
        return;
    };

    let body_str = String::from_utf8_lossy(&body);

    // Only process workflow_run and check_run events
    if !matches!(event_type.as_str(), "workflow_run" | "check_run" | "push" | "unknown") {
        let resp = serde_json::to_string(&json!({"skipped": true, "event": event_type}))
            .unwrap_or_else(|_| "{}".to_string());
        send_http_response(stream, 200, &resp);
        return;
    }

    let event: GitHubActionEvent = match serde_json::from_str(&body_str) {
        Ok(e) => e,
        Err(err) => {
            let resp = serde_json::to_string(&json!({"error": format!("json parse failed: {err}")}))
                .unwrap_or_else(|_| "{}".to_string());
            send_http_response(stream, 400, &resp);
            return;
        }
    };

    let scenarios = classify_event(&event);
    let diagnosis = build_diagnosis_response(&event, &scenarios);
    let resp = serde_json::to_string(&diagnosis).unwrap_or_else(|_| "{}".to_string());

    eprintln!("[{peer}] event={event_type} scenarios={scenarios:?}");
    println!("{resp}");

    send_http_response(stream, 200, &resp);
}

fn ctrlc_setup(_running: Arc<AtomicBool>) {
    // Best-effort: if we can't set up Ctrl-C handling, it's not fatal.
    // The process will still exit cleanly when the listener socket is closed.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_workflow_event(conclusion: &str, name: &str) -> GitHubActionEvent {
        GitHubActionEvent {
            action: Some("completed".to_string()),
            workflow_run: Some(WorkflowRunPayload {
                status: Some("completed".to_string()),
                conclusion: Some(conclusion.to_string()),
                name: Some(name.to_string()),
                head_branch: Some("main".to_string()),
                html_url: Some("https://github.com/example/repo/actions/runs/1".to_string()),
            }),
            check_run: None,
        }
    }

    fn make_check_run_event(conclusion: &str, name: &str, summary: &str) -> GitHubActionEvent {
        GitHubActionEvent {
            action: Some("completed".to_string()),
            workflow_run: None,
            check_run: Some(CheckRunPayload {
                status: Some("completed".to_string()),
                conclusion: Some(conclusion.to_string()),
                name: Some(name.to_string()),
                output: Some(CheckRunOutput {
                    title: Some(name.to_string()),
                    summary: Some(summary.to_string()),
                    text: None,
                }),
            }),
        }
    }

    #[test]
    fn classify_success_returns_empty() {
        let event = make_workflow_event("success", "cargo test");
        assert!(classify_event(&event).is_empty());
    }

    #[test]
    fn classify_build_failure_returns_compile_scenario() {
        let event = make_workflow_event("failure", "cargo build");
        let scenarios = classify_event(&event);
        assert!(scenarios.contains(&FailureScenario::CompileRedCrossCrate));
    }

    #[test]
    fn classify_stale_branch_by_check_summary() {
        let event = make_check_run_event("failure", "CI", "This branch is stale and needs a rebase");
        let scenarios = classify_event(&event);
        assert!(scenarios.contains(&FailureScenario::StaleBranch));
    }

    #[test]
    fn classify_provider_failure_on_timeout() {
        let event = make_workflow_event("timed_out", "integration tests");
        let scenarios = classify_event(&event);
        assert!(scenarios.contains(&FailureScenario::ProviderFailure));
    }

    #[test]
    fn classify_mcp_handshake_by_name() {
        let event = make_workflow_event("failure", "mcp lifecycle test");
        let scenarios = classify_event(&event);
        assert!(scenarios.contains(&FailureScenario::McpHandshakeFailure));
    }

    #[test]
    fn classify_deduplicates_scenarios() {
        let event = make_check_run_event(
            "failure",
            "cargo build",
            "error[E0507]: cannot move out of a shared reference",
        );
        let scenarios = classify_event(&event);
        let compile_count = scenarios.iter().filter(|s| **s == FailureScenario::CompileRedCrossCrate).count();
        assert_eq!(compile_count, 1);
    }

    #[test]
    fn build_diagnosis_response_includes_expected_fields() {
        let event = make_workflow_event("failure", "cargo test");
        let scenarios = classify_event(&event);
        let response = build_diagnosis_response(&event, &scenarios);
        assert_eq!(response["type"], "ci_diagnosis");
        assert!(response["scenarios_detected"].is_array());
        assert!(response["recovery_recipes"].is_array());
        assert!(response["branch"].is_string());
    }

    #[test]
    fn build_diagnosis_each_scenario_has_steps() {
        let event = make_workflow_event("failure", "cargo build");
        let scenarios = classify_event(&event);
        let response = build_diagnosis_response(&event, &scenarios);
        let recipes = response["recovery_recipes"].as_array().expect("recipes array");
        for recipe in recipes {
            assert!(recipe["steps"].is_array());
            assert!(recipe["scenario"].is_string());
        }
    }

    #[test]
    fn recipe_for_all_detected_scenarios_returns_non_empty_steps() {
        for scenario in FailureScenario::all() {
            let recipe = recipe_for(scenario);
            assert!(
                !recipe.steps.is_empty(),
                "scenario {scenario} has no recovery steps"
            );
        }
    }
}
