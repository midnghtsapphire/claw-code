//! E2-5: JSON output contract tests.
//!
//! For every command that claims `--output-format json` support, these tests
//! verify that:
//!   1. The command exits successfully.
//!   2. stdout is valid JSON.
//!   3. The required top-level keys are present.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir(label: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_millis();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "claw-json-contract-{label}-{}-{millis}-{counter}",
        std::process::id()
    ))
}

fn run_claw(current_dir: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_claw"))
        .current_dir(current_dir)
        .args(args)
        .output()
        .expect("claw should launch")
}

fn assert_json_contract(output: &Output, required_keys: &[&str]) {
    assert!(
        output.status.success(),
        "command should exit 0\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8(output.stdout.clone()).expect("stdout should be utf8");
    let value: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout should be valid JSON, got:\n{stdout}\nerror: {e}"));

    for key in required_keys {
        assert!(
            value.get(key).is_some(),
            "JSON response should contain key `{key}`, got:\n{stdout}"
        );
    }
}

// ──────────────────────────────────────────────
// status
// ──────────────────────────────────────────────

#[test]
fn status_json_is_valid_and_has_required_keys() {
    let dir = unique_temp_dir("status");
    std::fs::create_dir_all(&dir).expect("temp dir");

    let output = run_claw(&dir, &["--output-format", "json", "status"]);
    assert_json_contract(&output, &["sandbox", "status", "workspace"]);
}

#[test]
fn status_json_equals_format_eq_json() {
    let dir = unique_temp_dir("status-eq");
    std::fs::create_dir_all(&dir).expect("temp dir");

    let output = run_claw(&dir, &["--output-format=json", "status"]);
    assert_json_contract(&output, &["sandbox", "status", "workspace"]);
}

// ──────────────────────────────────────────────
// sandbox
// ──────────────────────────────────────────────

#[test]
fn sandbox_json_is_valid_and_has_required_keys() {
    let dir = unique_temp_dir("sandbox");
    std::fs::create_dir_all(&dir).expect("temp dir");

    let output = run_claw(&dir, &["--output-format", "json", "sandbox"]);
    assert_json_contract(&output, &["sandbox"]);
}

// ──────────────────────────────────────────────
// doctor
// ──────────────────────────────────────────────

#[test]
fn doctor_json_is_valid_and_has_required_keys() {
    let dir = unique_temp_dir("doctor");
    std::fs::create_dir_all(&dir).expect("temp dir");

    let output = run_claw(&dir, &["--output-format", "json", "doctor"]);
    assert_json_contract(&output, &["checks", "overall"]);
}

// ──────────────────────────────────────────────
// skills
// ──────────────────────────────────────────────

#[test]
fn skills_json_is_valid_and_has_required_keys() {
    let dir = unique_temp_dir("skills");
    std::fs::create_dir_all(&dir).expect("temp dir");

    let output = run_claw(&dir, &["--output-format", "json", "skills"]);
    assert_json_contract(&output, &["skills"]);

    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert!(
        value["skills"].is_array(),
        "`skills` key should be an array, got: {stdout}"
    );
}

#[test]
fn skills_json_array_items_have_expected_shape() {
    let dir = unique_temp_dir("skills-shape");
    // Add a minimal skill definition so there is at least one item to validate.
    let skills_dir = dir.join(".claude").join("skills");
    std::fs::create_dir_all(&skills_dir).expect("skills dir");
    std::fs::write(
        skills_dir.join("demo.md"),
        "# demo\nA demo skill for testing.\n",
    )
    .expect("write skill fixture");

    let output = run_claw(&dir, &["--output-format", "json", "skills"]);
    assert!(output.status.success(), "skills should succeed");

    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    let items = value["skills"].as_array().expect("skills is array");

    for item in items {
        assert!(item.get("name").is_some(), "each skill should have `name`");
        assert!(
            item.get("source").is_some(),
            "each skill should have `source`"
        );
    }
}

// ──────────────────────────────────────────────
// mcp
// ──────────────────────────────────────────────

#[test]
fn mcp_json_is_valid_and_has_required_keys() {
    let dir = unique_temp_dir("mcp");
    std::fs::create_dir_all(&dir).expect("temp dir");

    let output = run_claw(&dir, &["--output-format", "json", "mcp"]);
    assert_json_contract(&output, &["cwd", "server_count", "servers"]);

    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert!(
        value["servers"].is_array(),
        "`servers` key should be an array, got: {stdout}"
    );
}

#[test]
fn mcp_json_server_count_matches_servers_array_length() {
    let dir = unique_temp_dir("mcp-count");
    std::fs::create_dir_all(&dir).expect("temp dir");

    let output = run_claw(&dir, &["--output-format", "json", "mcp"]);
    assert!(output.status.success(), "mcp json should succeed");

    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    let count = value["server_count"].as_u64().expect("server_count is u64");
    let servers = value["servers"].as_array().expect("servers is array");
    assert_eq!(
        count as usize,
        servers.len(),
        "server_count should equal servers array length"
    );
}

// ──────────────────────────────────────────────
// agents
// ──────────────────────────────────────────────

#[test]
fn agents_json_is_valid_and_has_required_keys() {
    let dir = unique_temp_dir("agents");
    std::fs::create_dir_all(&dir).expect("temp dir");

    let output = run_claw(&dir, &["--output-format", "json", "agents"]);
    assert_json_contract(&output, &["agents"]);

    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    assert!(
        value["agents"].is_array(),
        "`agents` key should be an array, got: {stdout}"
    );
}

#[test]
fn agents_json_array_items_have_expected_shape() {
    let dir = unique_temp_dir("agents-shape");
    // Add a minimal agent definition so there is at least one item to validate.
    let agents_dir = dir.join(".claude").join("agents");
    std::fs::create_dir_all(&agents_dir).expect("agents dir");
    std::fs::write(
        agents_dir.join("reviewer.md"),
        "# reviewer\nA review agent for testing.\n",
    )
    .expect("write agent fixture");

    let output = run_claw(&dir, &["--output-format", "json", "agents"]);
    assert!(output.status.success(), "agents should succeed");

    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("valid json");
    let items = value["agents"].as_array().expect("agents is array");

    for item in items {
        assert!(item.get("name").is_some(), "each agent should have `name`");
        assert!(
            item.get("source").is_some(),
            "each agent should have `source`"
        );
    }
}
