//! Commit provenance tracking for parallel swarm worktrees.
//!
//! Every commit produced by a swarm worker carries a [`CommitProvenanceRecord`]
//! that answers the questions:
//!
//! * **Which branch** was the commit pushed to?
//! * **Which worktree** produced the commit?
//! * **Does this commit supersede** another earlier commit (e.g. a rebase or
//!   amended push)?
//! * **What is the lineage** — the ordered chain of commit SHAs leading to this
//!   one within the current worktree's push history?
//!
//! The record is attached to a [`PushEvent`] that is emitted synchronously by
//! the orchestrator after each successful `git push`.  Downstream consumers
//! (audit logs, merge supervisors, conflict detectors) subscribe to these events
//! to build a complete picture of parallel work without inspecting the git
//! object database directly.
//!
//! # Design
//! * All types are `Clone + Serialize + Deserialize` so they can be persisted
//!   as JSON or forwarded over a message bus.
//! * A [`CommitLineage`] is a thin ordered newtype around `Vec<String>`
//!   (commit SHAs, oldest-first) with helpers for extending and comparing.
//! * [`CommitProvenanceRegistry`] is an `Arc<Mutex<…>>` registry that workers
//!   use to record every push; it supports querying the full push history for
//!   a given worktree or branch.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ──────────────────────────────────────────────────────────────────────────────
// Core types
// ──────────────────────────────────────────────────────────────────────────────

/// An ordered list of commit SHAs, oldest-first, representing the chain of
/// commits produced by a single worktree up to and including a given push.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CommitLineage(Vec<String>);

impl CommitLineage {
    /// Creates an empty lineage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a lineage from a pre-existing list of commit SHAs (oldest-first).
    #[must_use]
    pub fn from_shas(shas: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self(shas.into_iter().map(Into::into).collect())
    }

    /// Appends a new commit SHA to the tail of the lineage (newest commit).
    pub fn push_sha(&mut self, sha: impl Into<String>) {
        self.0.push(sha.into());
    }

    /// Returns a slice of all SHAs in oldest-first order.
    #[must_use]
    pub fn shas(&self) -> &[String] {
        &self.0
    }

    /// Returns the most recent (tip) commit SHA, if any.
    #[must_use]
    pub fn tip(&self) -> Option<&str> {
        self.0.last().map(String::as_str)
    }

    /// Returns the number of commits in the lineage.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the lineage contains no commits.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns `true` if `sha` appears anywhere in this lineage.
    #[must_use]
    pub fn contains(&self, sha: &str) -> bool {
        self.0.iter().any(|s| s == sha)
    }
}

impl std::fmt::Display for CommitLineage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]", self.0.join(" → "))
    }
}

/// A record capturing the full provenance of a single push event.
///
/// Produced by the swarm orchestrator immediately after `git push` succeeds.
/// Consumers should treat the tuple `(worktree, branch, commit_sha)` as the
/// primary key; `superseded_by` and `lineage` provide audit context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitProvenanceRecord {
    /// The commit SHA that was pushed.
    pub commit_sha: String,
    /// The branch to which the commit was pushed.
    pub branch: String,
    /// The filesystem path of the worktree that produced this commit.
    pub worktree: String,
    /// If this commit supersedes an earlier one (e.g. after a force-push or
    /// rebase), this field contains the SHA of the commit being replaced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    /// The ordered history of commit SHAs (oldest-first) produced by this
    /// worktree for this branch, up to and including `commit_sha`.
    pub lineage: CommitLineage,
    /// Unix timestamp (seconds) when the push was recorded.
    pub pushed_at: u64,
    /// Optional human-readable message, e.g. the first line of the commit
    /// message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// A structured event emitted by the orchestrator for each successful push.
///
/// Wraps a [`CommitProvenanceRecord`] with an event-level identifier and
/// sequence number so that consumers can order events across multiple worktrees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushEvent {
    /// Monotonically increasing sequence number within the registry.
    pub seq: u64,
    /// The event name (always `"commit.pushed"`).
    pub event: String,
    /// Provenance details for the push.
    pub provenance: CommitProvenanceRecord,
}

impl PushEvent {
    /// The canonical wire name for push events.
    pub const EVENT_NAME: &'static str = "commit.pushed";
}

impl std::fmt::Display for PushEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PushEvent[seq={}] {}/{} → {}",
            self.seq, self.provenance.worktree, self.provenance.branch, self.provenance.commit_sha
        )
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Registry
// ──────────────────────────────────────────────────────────────────────────────

/// Thread-safe in-memory registry of all push events recorded during a swarm
/// run.
///
/// Workers call [`record_push`](Self::record_push) after each successful `git
/// push`.  Supervisors and audit consumers call the read helpers to reconstruct
/// the provenance chain for any worktree or branch.
#[derive(Debug, Clone, Default)]
pub struct CommitProvenanceRegistry {
    inner: Arc<Mutex<CommitProvenanceRegistryInner>>,
}

#[derive(Debug, Default)]
struct CommitProvenanceRegistryInner {
    events: Vec<PushEvent>,
    seq: u64,
}

impl CommitProvenanceRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a successful push and returns the resulting [`PushEvent`].
    ///
    /// The `lineage` should be built by the caller by extending the previous
    /// lineage (obtained via [`lineage_for`](Self::lineage_for)) with the new
    /// `commit_sha`.
    pub fn record_push(
        &self,
        commit_sha: impl Into<String>,
        branch: impl Into<String>,
        worktree: impl Into<String>,
        superseded_by: Option<String>,
        lineage: CommitLineage,
        summary: Option<String>,
    ) -> PushEvent {
        let mut inner = self
            .inner
            .lock()
            .expect("provenance registry lock poisoned");
        inner.seq += 1;
        let event = PushEvent {
            seq: inner.seq,
            event: PushEvent::EVENT_NAME.to_string(),
            provenance: CommitProvenanceRecord {
                commit_sha: commit_sha.into(),
                branch: branch.into(),
                worktree: worktree.into(),
                superseded_by,
                lineage,
                pushed_at: now_secs(),
                summary,
            },
        };
        inner.events.push(event.clone());
        event
    }

    /// Returns all push events in insertion order.
    #[must_use]
    pub fn all_events(&self) -> Vec<PushEvent> {
        self.inner
            .lock()
            .expect("provenance registry lock poisoned")
            .events
            .clone()
    }

    /// Returns all push events for a specific worktree, in insertion order.
    #[must_use]
    pub fn events_for_worktree(&self, worktree: &str) -> Vec<PushEvent> {
        self.inner
            .lock()
            .expect("provenance registry lock poisoned")
            .events
            .iter()
            .filter(|e| e.provenance.worktree == worktree)
            .cloned()
            .collect()
    }

    /// Returns all push events for a specific branch, in insertion order.
    #[must_use]
    pub fn events_for_branch(&self, branch: &str) -> Vec<PushEvent> {
        self.inner
            .lock()
            .expect("provenance registry lock poisoned")
            .events
            .iter()
            .filter(|e| e.provenance.branch == branch)
            .cloned()
            .collect()
    }

    /// Returns the current accumulated lineage for `(worktree, branch)` — i.e.
    /// all commit SHAs pushed by `worktree` to `branch`, oldest-first.
    ///
    /// Returns an empty [`CommitLineage`] if no pushes have been recorded for
    /// this pair.
    #[must_use]
    pub fn lineage_for(&self, worktree: &str, branch: &str) -> CommitLineage {
        let inner = self
            .inner
            .lock()
            .expect("provenance registry lock poisoned");
        let mut shas: Vec<String> = Vec::new();
        for event in &inner.events {
            let p = &event.provenance;
            if p.worktree == worktree && p.branch == branch {
                // Use the lineage stored in the most recent event; it is
                // cumulative so we only need the last one.
                shas.clone_from(&p.lineage.0);
            }
        }
        CommitLineage(shas)
    }

    /// Returns a map from branch name to the most recent commit SHA pushed to
    /// that branch across all worktrees.
    #[must_use]
    pub fn latest_per_branch(&self) -> BTreeMap<String, String> {
        let inner = self
            .inner
            .lock()
            .expect("provenance registry lock poisoned");
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        for event in &inner.events {
            map.insert(
                event.provenance.branch.clone(),
                event.provenance.commit_sha.clone(),
            );
        }
        map
    }

    /// Returns the total number of push events recorded.
    #[must_use]
    pub fn event_count(&self) -> usize {
        self.inner
            .lock()
            .expect("provenance registry lock poisoned")
            .events
            .len()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Git push output parsing
// ──────────────────────────────────────────────────────────────────────────────

/// Parsed result from a single `git push` ref-update line.
///
/// `git push` writes one line per ref to stderr in the form:
/// ```text
///   <flag> <from>..<to> <from-name> -> <to-name>
/// ```
/// or for a forced push:
/// ```text
/// + <from>..<to> <from-name> -> <to-name> (forced update)
/// ```
/// This struct captures the fields that are relevant for provenance tracking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitPushRefUpdate {
    /// The local commit SHA that was pushed (the "to" side of the range).
    pub commit_sha: String,
    /// The branch name on the remote (e.g. `refs/heads/feat/x` → `feat/x`).
    pub branch: String,
    /// If this was a forced push (`+` flag or `(forced update)` annotation),
    /// the previous tip SHA that was overwritten (the "from" side of the range).
    pub superseded_sha: Option<String>,
}

/// Parse the stderr output of `git push` and extract ref-update information.
///
/// Supports the standard push output format:
/// ```text
///    <sha1>..<sha2>  <branch> -> <remote>/<branch>
/// + <sha1>..<sha2>  <branch> -> <remote>/<branch> (forced update)
/// * [new branch]    <branch> -> <remote>/<branch>
/// ```
/// Returns one [`GitPushRefUpdate`] per successfully parsed ref-update line.
/// Lines that do not match the expected pattern are silently skipped.
///
/// # Example
///
/// ```rust
/// use runtime::commit_provenance::parse_git_push_output;
///
/// let output = "   abc123..def456  feat/x -> origin/feat/x";
/// let updates = parse_git_push_output(output);
/// assert_eq!(updates.len(), 1);
/// assert_eq!(updates[0].commit_sha, "def456");
/// assert_eq!(updates[0].branch, "feat/x");
/// ```
#[must_use]
pub fn parse_git_push_output(stderr: &str) -> Vec<GitPushRefUpdate> {
    let mut updates = Vec::new();

    for line in stderr.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Detect forced-push flag (leading `+`) or new-branch (`*`).
        let (is_forced, rest) = if let Some(r) = trimmed.strip_prefix('+') {
            (true, r.trim_start())
        } else if let Some(r) = trimmed.strip_prefix('*') {
            // New branch — no "from" SHA.
            let _ = r;
            continue;
        } else {
            (false, trimmed)
        };

        // rest should now start with "<from_sha>..<to_sha>  <branch> -> ..."
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        // parts[0] = "<from>..<to>" range
        let range = parts[0];
        let Some(arrow_pos) = parts.iter().position(|p| *p == "->") else {
            continue;
        };
        // Branch name is the token immediately before "->".
        if arrow_pos == 0 {
            continue;
        }
        let branch_raw = parts[arrow_pos - 1];
        // Strip "refs/heads/" prefix if present.
        let branch = branch_raw
            .strip_prefix("refs/heads/")
            .unwrap_or(branch_raw)
            .to_string();

        // Parse the SHA range.
        let (superseded_sha, commit_sha) = if let Some((from, to)) = range.split_once("..") {
            // from may be the zero-SHA for a brand-new branch.
            let superseded = if from.chars().all(|c| c == '0') || from.is_empty() {
                None
            } else {
                Some(from.to_string())
            };
            (if is_forced { superseded } else { None }, to.to_string())
        } else {
            // Not a range — skip.
            continue;
        };

        if commit_sha.is_empty() {
            continue;
        }

        updates.push(GitPushRefUpdate {
            commit_sha,
            branch,
            superseded_sha,
        });
    }

    updates
}

/// Helper: parse `git push` stderr and call
/// [`CommitProvenanceRegistry::record_push`] for each ref-update, building
/// the cumulative lineage automatically.
///
/// `worktree` is the filesystem path of the worktree that ran `git push`.
/// `summary` is an optional commit summary line (e.g. the first line of the
/// commit message).
///
/// Returns the list of [`PushEvent`]s that were recorded.
#[must_use]
pub fn record_push_from_git_output(
    registry: &CommitProvenanceRegistry,
    stderr: &str,
    worktree: &str,
    summary: Option<&str>,
) -> Vec<PushEvent> {
    let updates = parse_git_push_output(stderr);
    let mut events = Vec::new();

    for update in updates {
        let mut lineage = registry.lineage_for(worktree, &update.branch);
        lineage.push_sha(&update.commit_sha);
        let event = registry.record_push(
            &update.commit_sha,
            &update.branch,
            worktree,
            update.superseded_sha,
            lineage,
            summary.map(ToOwned::to_owned),
        );
        events.push(event);
    }

    events
}

// ──────────────────────────────────────────────────────────────────────────────
// Unit tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn lineage(shas: &[&str]) -> CommitLineage {
        CommitLineage::from_shas(shas.iter().copied())
    }

    #[test]
    fn push_event_carries_branch_worktree_and_lineage() {
        // given
        let registry = CommitProvenanceRegistry::new();

        // when
        let event = registry.record_push(
            "abc123",
            "feat/new-tool",
            "/worktrees/repo-a",
            None,
            lineage(&["abc123"]),
            Some("add new tool".to_string()),
        );

        // then
        assert_eq!(event.event, PushEvent::EVENT_NAME);
        assert_eq!(event.provenance.commit_sha, "abc123");
        assert_eq!(event.provenance.branch, "feat/new-tool");
        assert_eq!(event.provenance.worktree, "/worktrees/repo-a");
        assert!(event.provenance.superseded_by.is_none());
        assert_eq!(event.provenance.lineage.tip(), Some("abc123"));
    }

    #[test]
    fn push_event_includes_superseded_by_when_provided() {
        // given
        let registry = CommitProvenanceRegistry::new();

        // when
        let event = registry.record_push(
            "def456",
            "feat/fix",
            "/worktrees/repo-b",
            Some("abc123".to_string()),
            lineage(&["abc123", "def456"]),
            None,
        );

        // then
        assert_eq!(event.provenance.superseded_by.as_deref(), Some("abc123"));
        assert_eq!(event.provenance.lineage.len(), 2);
    }

    #[test]
    fn lineage_for_returns_accumulated_shas_for_worktree_branch_pair() {
        // given
        let registry = CommitProvenanceRegistry::new();
        let wt = "/worktrees/wt1";
        let branch = "feat/x";

        // when — three sequential pushes
        let mut lin = CommitLineage::new();
        for sha in &["a1b2c3", "d4e5f6", "9a8b7c"] {
            lin.push_sha(*sha);
            registry.record_push(*sha, branch, wt, None, lin.clone(), None);
        }

        // then
        let result = registry.lineage_for(wt, branch);
        assert_eq!(result.shas(), &["a1b2c3", "d4e5f6", "9a8b7c"]);
        assert_eq!(result.tip(), Some("9a8b7c"));
    }

    #[test]
    fn events_for_worktree_filters_to_single_worktree() {
        // given
        let registry = CommitProvenanceRegistry::new();
        registry.record_push("sha1", "main", "/wt/a", None, lineage(&["sha1"]), None);
        registry.record_push("sha2", "main", "/wt/b", None, lineage(&["sha2"]), None);
        registry.record_push(
            "sha3",
            "main",
            "/wt/a",
            None,
            lineage(&["sha1", "sha3"]),
            None,
        );

        // when
        let events = registry.events_for_worktree("/wt/a");

        // then
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e.provenance.worktree == "/wt/a"));
    }

    #[test]
    fn events_for_branch_filters_to_single_branch() {
        // given
        let registry = CommitProvenanceRegistry::new();
        registry.record_push("s1", "feat/a", "/wt/x", None, lineage(&["s1"]), None);
        registry.record_push("s2", "feat/b", "/wt/x", None, lineage(&["s2"]), None);
        registry.record_push("s3", "feat/a", "/wt/y", None, lineage(&["s3"]), None);

        // when
        let events = registry.events_for_branch("feat/a");

        // then
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|e| e.provenance.branch == "feat/a"));
    }

    #[test]
    fn latest_per_branch_returns_most_recent_sha_per_branch() {
        // given
        let registry = CommitProvenanceRegistry::new();
        registry.record_push("v1", "main", "/wt/a", None, lineage(&["v1"]), None);
        registry.record_push("v2", "main", "/wt/a", None, lineage(&["v1", "v2"]), None);
        registry.record_push("f1", "feat/z", "/wt/b", None, lineage(&["f1"]), None);

        // when
        let map = registry.latest_per_branch();

        // then
        assert_eq!(map.get("main").map(String::as_str), Some("v2"));
        assert_eq!(map.get("feat/z").map(String::as_str), Some("f1"));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn seq_numbers_are_monotonically_increasing() {
        // given
        let registry = CommitProvenanceRegistry::new();

        // when
        let e1 = registry.record_push("c1", "main", "/wt/a", None, lineage(&["c1"]), None);
        let e2 = registry.record_push("c2", "main", "/wt/a", None, lineage(&["c1", "c2"]), None);
        let e3 = registry.record_push("c3", "feat/x", "/wt/b", None, lineage(&["c3"]), None);

        // then
        assert!(e1.seq < e2.seq);
        assert!(e2.seq < e3.seq);
    }

    #[test]
    fn push_event_serializes_and_deserializes_without_loss() {
        // given
        let registry = CommitProvenanceRegistry::new();
        let original = registry.record_push(
            "deadbeef",
            "feat/ser",
            "/worktrees/ser",
            Some("prev_sha".to_string()),
            lineage(&["prev_sha", "deadbeef"]),
            Some("fix: serialization".to_string()),
        );

        // when
        let json = serde_json::to_string(&original).expect("serialize");
        let back: PushEvent = serde_json::from_str(&json).expect("deserialize");

        // then
        assert_eq!(original, back);
        assert!(json.contains("commit.pushed"));
        assert!(json.contains("deadbeef"));
        assert!(json.contains("feat/ser"));
        assert!(json.contains("superseded_by"));
        assert!(json.contains("prev_sha"));
        assert!(json.contains("lineage"));
    }

    #[test]
    fn commit_lineage_display_formats_as_arrow_chain() {
        // given
        let lineage = CommitLineage::from_shas(["abc", "def", "ghi"]);

        // when
        let s = lineage.to_string();

        // then
        assert_eq!(s, "[abc → def → ghi]");
    }

    #[test]
    fn push_event_display_includes_worktree_branch_and_sha() {
        // given
        let registry = CommitProvenanceRegistry::new();
        let event =
            registry.record_push("sha99", "feat/ui", "/wt/x", None, lineage(&["sha99"]), None);

        // when
        let s = event.to_string();

        // then
        assert!(s.contains("/wt/x"), "display should include worktree: {s}");
        assert!(s.contains("feat/ui"), "display should include branch: {s}");
        assert!(s.contains("sha99"), "display should include sha: {s}");
    }

    #[test]
    fn empty_registry_returns_empty_lineage_for_unknown_pair() {
        // given
        let registry = CommitProvenanceRegistry::new();

        // when
        let lineage = registry.lineage_for("/wt/ghost", "feat/ghost");

        // then
        assert!(lineage.is_empty());
        assert_eq!(lineage.tip(), None);
    }

    #[test]
    fn event_count_tracks_all_recorded_pushes() {
        // given
        let registry = CommitProvenanceRegistry::new();
        assert_eq!(registry.event_count(), 0);

        // when
        for i in 0..5u32 {
            registry.record_push(format!("sha{i}"), "main", "/wt/a", None, lineage(&[]), None);
        }

        // then
        assert_eq!(registry.event_count(), 5);
    }

    #[test]
    fn lineage_for_returns_empty_for_different_worktree_same_branch() {
        // given
        let registry = CommitProvenanceRegistry::new();
        registry.record_push("sha1", "main", "/wt/a", None, lineage(&["sha1"]), None);

        // when
        let result = registry.lineage_for("/wt/b", "main");

        // then
        assert!(
            result.is_empty(),
            "lineage for a different worktree should be empty"
        );
    }
}
