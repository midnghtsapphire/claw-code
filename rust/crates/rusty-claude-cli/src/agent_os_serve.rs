//! Agent OS control plane — HTTP REST API over the task and worker registries.
//!
//! Exposes a lightweight HTTP/1.1 server that lets external agents and
//! orchestrators inspect and drive the local claw runtime:
//!
//! ```text
//! GET  /v1/status          — runtime health snapshot
//! GET  /v1/tasks           — list all tasks (optional ?status=filter)
//! POST /v1/tasks           — create a new task
//! GET  /v1/tasks/:id       — get a specific task
//! POST /v1/tasks/:id/stop  — stop a running task
//! GET  /v1/workers         — list all workers
//! POST /v1/workers         — create a new worker
//! GET  /v1/workers/:id     — get a specific worker
//! ```
//!
//! Usage: `claw serve [--port N]`

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use runtime::task_registry::{Task, TaskRegistry, TaskStatus};
use runtime::{Worker, WorkerRegistry};
use serde::Deserialize;
use serde_json::{json, Value};

pub const DEFAULT_SERVE_PORT: u16 = 9056;

/// Shared application state for the control plane.
pub struct ControlPlane {
    pub tasks: Arc<TaskRegistry>,
    pub workers: Arc<Mutex<WorkerRegistry>>,
    /// Worker IDs created through this control plane, for listing.
    pub worker_ids: Arc<Mutex<Vec<String>>>,
}

impl ControlPlane {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(TaskRegistry::new()),
            workers: Arc::new(Mutex::new(WorkerRegistry::new())),
            worker_ids: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Default for ControlPlane {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Lock helpers
// ---------------------------------------------------------------------------

/// Acquire a `Mutex` lock, recovering from poison errors by consuming the guard.
/// This follows the project-wide pattern and keeps call sites concise.
fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// ---------------------------------------------------------------------------
// HTTP plumbing
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    query: String,
    body: Vec<u8>,
}

fn parse_request(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).ok()?;

    let mut parts = first_line.split_whitespace();
    let method = parts.next()?.to_string();
    let raw_path = parts.next().unwrap_or("/");
    let (path, query) = raw_path.split_once('?').map_or_else(
        || (raw_path.to_string(), String::new()),
        |(p, q)| (p.to_string(), q.to_string()),
    );

    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if trimmed.to_ascii_lowercase().starts_with("content-length:") {
            content_length = trimmed[15..].trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        use std::io::Read;
        reader.read_exact(&mut body).ok()?;
    }

    Some(Request {
        method,
        path,
        query,
        body,
    })
}

fn send_response(mut stream: TcpStream, status: u16, value: Value) {
    let body = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
    let status_text = match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

fn route(plane: &ControlPlane, req: &Request) -> (u16, Value) {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/v1/status") => handle_status(plane),
        ("GET", "/v1/tasks") => handle_list_tasks(plane, &req.query),
        ("POST", "/v1/tasks") => handle_create_task(plane, &req.body),
        (method, path) if path.starts_with("/v1/tasks/") => {
            let id = &path["/v1/tasks/".len()..];
            if id.ends_with("/stop") {
                let task_id = &id[..id.len() - "/stop".len()];
                if method == "POST" {
                    handle_stop_task(plane, task_id)
                } else {
                    (405, json!({"error": "method not allowed"}))
                }
            } else if method == "GET" {
                handle_get_task(plane, id)
            } else {
                (405, json!({"error": "method not allowed"}))
            }
        }
        ("GET", "/v1/workers") => handle_list_workers(plane),
        ("POST", "/v1/workers") => handle_create_worker(plane, &req.body),
        (method, path) if path.starts_with("/v1/workers/") => {
            let id = &path["/v1/workers/".len()..];
            if method == "GET" {
                handle_get_worker(plane, id)
            } else {
                (405, json!({"error": "method not allowed"}))
            }
        }
        _ => (404, json!({"error": "not found", "path": req.path})),
    }
}

fn handle_status(plane: &ControlPlane) -> (u16, Value) {
    let tasks = plane.tasks.list(None);
    let worker_count = lock_or_recover(&plane.worker_ids).len();
    (
        200,
        json!({
            "status": "ok",
            "tasks": {
                "total": tasks.len(),
                "created": tasks.iter().filter(|t| t.status == TaskStatus::Created).count(),
                "running": tasks.iter().filter(|t| t.status == TaskStatus::Running).count(),
                "completed": tasks.iter().filter(|t| t.status == TaskStatus::Completed).count(),
                "failed": tasks.iter().filter(|t| t.status == TaskStatus::Failed).count(),
                "stopped": tasks.iter().filter(|t| t.status == TaskStatus::Stopped).count(),
            },
            "workers": {
                "total": worker_count,
            }
        }),
    )
}

fn handle_list_tasks(plane: &ControlPlane, query: &str) -> (u16, Value) {
    let status_filter = parse_query_param(query, "status").and_then(|s| match s.as_str() {
        "created" => Some(TaskStatus::Created),
        "running" => Some(TaskStatus::Running),
        "completed" => Some(TaskStatus::Completed),
        "failed" => Some(TaskStatus::Failed),
        "stopped" => Some(TaskStatus::Stopped),
        _ => None,
    });
    let tasks: Vec<Value> = plane
        .tasks
        .list(status_filter)
        .iter()
        .map(task_to_json)
        .collect();
    (200, json!({"tasks": tasks}))
}

fn handle_create_task(plane: &ControlPlane, body: &[u8]) -> (u16, Value) {
    #[derive(Deserialize)]
    struct CreateTask {
        prompt: String,
        description: Option<String>,
    }
    let payload: CreateTask = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(err) => return (400, json!({"error": format!("invalid body: {err}")})),
    };
    let task = plane
        .tasks
        .create(&payload.prompt, payload.description.as_deref());
    (201, task_to_json(&task))
}

fn handle_get_task(plane: &ControlPlane, id: &str) -> (u16, Value) {
    match plane.tasks.get(id) {
        Some(task) => (200, task_to_json(&task)),
        None => (404, json!({"error": "task not found", "id": id})),
    }
}

fn handle_stop_task(plane: &ControlPlane, id: &str) -> (u16, Value) {
    match plane.tasks.stop(id) {
        Ok(task) => (200, task_to_json(&task)),
        Err(err) => (400, json!({"error": err})),
    }
}

fn handle_list_workers(plane: &ControlPlane) -> (u16, Value) {
    let ids = lock_or_recover(&plane.worker_ids).clone();
    let workers_lock = lock_or_recover(&plane.workers);
    let list: Vec<Value> = ids
        .iter()
        .filter_map(|id| workers_lock.get(id))
        .map(|w| worker_to_json(&w))
        .collect();
    (200, json!({"workers": list}))
}

fn handle_create_worker(plane: &ControlPlane, body: &[u8]) -> (u16, Value) {
    #[derive(Deserialize, Default)]
    struct CreateWorker {
        cwd: Option<String>,
    }
    let payload: CreateWorker = serde_json::from_slice(body).unwrap_or_default();
    let cwd = payload.cwd.unwrap_or_else(|| {
        std::env::current_dir().map_or_else(|_| ".".to_string(), |p| p.display().to_string())
    });

    let worker = lock_or_recover(&plane.workers).create(&cwd, &[], false);

    lock_or_recover(&plane.worker_ids).push(worker.worker_id.clone());
    (201, worker_to_json(&worker))
}

fn handle_get_worker(plane: &ControlPlane, id: &str) -> (u16, Value) {
    let worker = lock_or_recover(&plane.workers).get(id);
    match worker {
        Some(w) => (200, worker_to_json(&w)),
        None => (404, json!({"error": "worker not found", "id": id})),
    }
}

// ---------------------------------------------------------------------------
// Serialisation helpers
// ---------------------------------------------------------------------------

fn task_to_json(task: &Task) -> Value {
    json!({
        "id": task.task_id,
        "status": task.status.to_string(),
        "prompt": task.prompt,
        "description": task.description,
        "created_at": task.created_at,
        "updated_at": task.updated_at,
    })
}

fn worker_to_json(worker: &Worker) -> Value {
    json!({
        "id": worker.worker_id,
        "status": format!("{:?}", worker.status),
        "cwd": worker.cwd,
        "created_at": worker.created_at,
        "updated_at": worker.updated_at,
    })
}

fn parse_query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(v.to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Server entry point
// ---------------------------------------------------------------------------

/// Start the Agent OS control plane on `port`.
pub fn run_control_plane(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    let plane = Arc::new(ControlPlane::new());

    eprintln!("claw serve (Agent OS control plane) on port {port}");
    eprintln!("Endpoints:");
    eprintln!("  GET  /v1/status");
    eprintln!("  GET  /v1/tasks?status=[created|running|completed|failed|stopped]");
    eprintln!("  POST /v1/tasks        {{\"prompt\":\"...\",\"description\":\"...\"}}");
    eprintln!("  GET  /v1/tasks/:id");
    eprintln!("  POST /v1/tasks/:id/stop");
    eprintln!("  GET  /v1/workers");
    eprintln!("  POST /v1/workers      {{\"cwd\":\"/path/to/project\"}}");
    eprintln!("  GET  /v1/workers/:id");
    eprintln!("(Ctrl-C to stop)");

    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        let plane = Arc::clone(&plane);

        let peer = stream
            .peer_addr()
            .map_or_else(|_| "?".to_string(), |a| a.to_string());
        if let Some(req) = parse_request(&stream) {
            eprintln!("[{peer}] {} {}", req.method, req.path);
            let (status, body) = route(&plane, &req);
            send_response(stream, status, body);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> ControlPlane {
        ControlPlane::new()
    }

    #[test]
    fn status_returns_zero_counts_initially() {
        let p = plane();
        let (status, body) = handle_status(&p);
        assert_eq!(status, 200);
        assert_eq!(body["status"], "ok");
        assert_eq!(body["tasks"]["total"], 0);
        assert_eq!(body["workers"]["total"], 0);
    }

    #[test]
    fn create_task_and_list_it() {
        let p = plane();
        let body = br#"{"prompt":"test task"}"#;
        let (status, created) = handle_create_task(&p, body);
        assert_eq!(status, 201);
        assert_eq!(created["prompt"], "test task");

        let (status2, list) = handle_list_tasks(&p, "");
        assert_eq!(status2, 200);
        assert_eq!(list["tasks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn get_task_by_id() {
        let p = plane();
        let body = br#"{"prompt":"fetch me"}"#;
        let (_, created) = handle_create_task(&p, body);
        let id = created["id"].as_str().unwrap().to_string();

        let (status, fetched) = handle_get_task(&p, &id);
        assert_eq!(status, 200);
        assert_eq!(fetched["id"], id);
    }

    #[test]
    fn get_missing_task_returns_404() {
        let p = plane();
        let (status, body) = handle_get_task(&p, "nonexistent-xyz");
        assert_eq!(status, 404);
        assert!(body["error"].is_string());
    }

    #[test]
    fn stop_task_changes_status() {
        let p = plane();
        let body = br#"{"prompt":"stoppable task"}"#;
        let (_, created) = handle_create_task(&p, body);
        let id = created["id"].as_str().unwrap().to_string();

        let (status, stopped) = handle_stop_task(&p, &id);
        assert_eq!(status, 200);
        assert!(stopped["status"].as_str().unwrap().contains("stopped"));
    }

    #[test]
    fn create_worker_and_list_it() {
        let p = plane();
        let body = br#"{"cwd":"/tmp/test-project"}"#;
        let (status, created) = handle_create_worker(&p, body);
        assert_eq!(status, 201);
        assert!(created["id"].as_str().unwrap().starts_with("worker_"));

        let (status2, list) = handle_list_workers(&p);
        assert_eq!(status2, 200);
        assert_eq!(list["workers"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn get_worker_by_id() {
        let p = plane();
        let body = br#"{"cwd":"/tmp/test-project"}"#;
        let (_, created) = handle_create_worker(&p, body);
        let id = created["id"].as_str().unwrap().to_string();

        let (status, fetched) = handle_get_worker(&p, &id);
        assert_eq!(status, 200);
        assert_eq!(fetched["id"], id);
    }

    #[test]
    fn get_missing_worker_returns_404() {
        let p = plane();
        let (status, body) = handle_get_worker(&p, "no-such-worker");
        assert_eq!(status, 404);
        assert!(body["error"].is_string());
    }

    #[test]
    fn list_tasks_with_status_filter_created() {
        let p = plane();
        let body = br#"{"prompt":"queued task"}"#;
        handle_create_task(&p, body);

        let (status, list) = handle_list_tasks(&p, "status=created");
        assert_eq!(status, 200);
        assert!(!list["tasks"].as_array().unwrap().is_empty());

        let (status2, list2) = handle_list_tasks(&p, "status=running");
        assert_eq!(status2, 200);
        assert!(list2["tasks"].as_array().unwrap().is_empty());
    }

    #[test]
    fn status_counts_update_after_create() {
        let p = plane();
        handle_create_task(&p, br#"{"prompt":"task 1"}"#);
        handle_create_task(&p, br#"{"prompt":"task 2"}"#);

        let (_, body) = handle_status(&p);
        assert_eq!(body["tasks"]["total"], 2);
        assert_eq!(body["tasks"]["created"], 2);
    }

    #[test]
    fn create_task_invalid_body_returns_400() {
        let p = plane();
        let (status, body) = handle_create_task(&p, b"not json");
        assert_eq!(status, 400);
        assert!(body["error"].is_string());
    }

    #[test]
    fn route_unknown_path_returns_404() {
        let p = plane();
        let req = Request {
            method: "GET".to_string(),
            path: "/v1/unknown-endpoint".to_string(),
            query: String::new(),
            body: Vec::new(),
        };
        let (status, _) = route(&p, &req);
        assert_eq!(status, 404);
    }
}
