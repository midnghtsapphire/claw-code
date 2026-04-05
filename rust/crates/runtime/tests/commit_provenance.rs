//! E7-2: Commit provenance tracking integration tests.
//!
//! These tests verify that [`CommitProvenanceRegistry`] correctly records push
//! events with all required fields — branch, worktree, superseded-by, lineage —
//! and that the resulting [`PushEvent`] objects are inspectable, filterable, and
//! serde-stable.
//!
//! Scenarios:
//! 1. **Push event contains branch, worktree, commit SHA** — mandatory fields.
//! 2. **Push event includes superseded-by field** — when a commit replaces an
//!    earlier one (rebase/force-push).
//! 3. **Lineage accumulates across sequential pushes** — oldest-first SHA list.
//! 4. **lineage_for returns correct chain for (worktree, branch) pair**.
//! 5. **events_for_worktree isolates by worktree** — other worktrees excluded.
//! 6. **events_for_branch isolates by branch** — other branches excluded.
//! 7. **latest_per_branch reflects most recent push per branch**.
//! 8. **Sequence numbers are monotonically increasing** across all pushes.
//! 9. **PushEvent JSON is stable** — serde round-trip preserves every field.
//! 10. **CommitLineage Display formats as arrow chain**.
//! 11. **PushEvent Display includes worktree, branch, SHA**.
//! 12. **Empty registry returns empty lineage** for unknown pair.
//! 13. **event_count tracks total pushes**.
//! 14. **Parallel worktrees on same branch have independent lineages**.
//! 15. **superseded_by field is omitted from JSON when None** (clean output).

use runtime::{CommitLineage, CommitProvenanceRecord, CommitProvenanceRegistry, PushEvent};

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn lin(shas: &[&str]) -> CommitLineage {
    CommitLineage::from_shas(shas.iter().copied())
}

// ──────────────────────────────────────────────────────────────────────────────
// 1. Push event contains branch, worktree, commit SHA
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn push_event_contains_branch_worktree_and_commit_sha() {
    // given
    let registry = CommitProvenanceRegistry::new();

    // when
    let event = registry.record_push(
        "abc123",
        "feat/new-tool",
        "/worktrees/repo-a",
        None,
        lin(&["abc123"]),
        Some("add new tool".to_string()),
    );

    // then
    assert_eq!(event.provenance.commit_sha, "abc123");
    assert_eq!(event.provenance.branch, "feat/new-tool");
    assert_eq!(event.provenance.worktree, "/worktrees/repo-a");
    assert_eq!(event.event, PushEvent::EVENT_NAME);
    assert_eq!(event.event, "commit.pushed");
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. Push event includes superseded-by when provided
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn push_event_includes_superseded_by_field() {
    // given
    let registry = CommitProvenanceRegistry::new();
    // first commit
    registry.record_push("sha_old", "feat/fix", "/wt/a", None, lin(&["sha_old"]), None);

    // when — force push supersedes sha_old
    let event = registry.record_push(
        "sha_new",
        "feat/fix",
        "/wt/a",
        Some("sha_old".to_string()),
        lin(&["sha_new"]),
        Some("fix: rebase".to_string()),
    );

    // then
    assert_eq!(
        event.provenance.superseded_by.as_deref(),
        Some("sha_old"),
        "superseded_by must name the replaced commit"
    );
    assert_eq!(event.provenance.commit_sha, "sha_new");
}

// ──────────────────────────────────────────────────────────────────────────────
// 3. Lineage accumulates across sequential pushes
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn lineage_accumulates_across_sequential_pushes() {
    // given
    let registry = CommitProvenanceRegistry::new();
    let wt = "/wt/linear";
    let branch = "main";

    // when — three sequential pushes, each extending the lineage
    let mut lin_acc = CommitLineage::new();
    let shas = ["a111", "b222", "c333"];
    for sha in &shas {
        lin_acc.push_sha(*sha);
        registry.record_push(*sha, branch, wt, None, lin_acc.clone(), None);
    }

    // then — the last push's lineage is the full chain
    let events = registry.events_for_worktree(wt);
    let last = events.last().expect("should have events");
    assert_eq!(last.provenance.lineage.shas(), &["a111", "b222", "c333"]);
    assert_eq!(last.provenance.lineage.tip(), Some("c333"));
    assert_eq!(last.provenance.lineage.len(), 3);
}

// ──────────────────────────────────────────────────────────────────────────────
// 4. lineage_for returns correct chain for (worktree, branch) pair
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn lineage_for_returns_accumulated_chain_for_worktree_branch_pair() {
    // given
    let registry = CommitProvenanceRegistry::new();
    let wt = "/wt/chained";
    let branch = "feat/chain";

    let mut acc = CommitLineage::new();
    for sha in &["x1", "x2", "x3"] {
        acc.push_sha(*sha);
        registry.record_push(*sha, branch, wt, None, acc.clone(), None);
    }

    // when
    let result = registry.lineage_for(wt, branch);

    // then
    assert_eq!(result.shas(), &["x1", "x2", "x3"]);
    assert_eq!(result.len(), 3);
    assert!(result.contains("x2"));
    assert!(!result.contains("x99"));
}

// ──────────────────────────────────────────────────────────────────────────────
// 5. events_for_worktree isolates by worktree
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn events_for_worktree_returns_only_that_worktrees_events() {
    // given
    let registry = CommitProvenanceRegistry::new();
    registry.record_push("s1", "main", "/wt/alpha", None, lin(&["s1"]), None);
    registry.record_push("s2", "main", "/wt/beta", None, lin(&["s2"]), None);
    registry.record_push("s3", "main", "/wt/alpha", None, lin(&["s1", "s3"]), None);

    // when
    let alpha_events = registry.events_for_worktree("/wt/alpha");
    let beta_events = registry.events_for_worktree("/wt/beta");

    // then
    assert_eq!(alpha_events.len(), 2);
    assert_eq!(beta_events.len(), 1);
    assert!(alpha_events
        .iter()
        .all(|e| e.provenance.worktree == "/wt/alpha"));
}

// ──────────────────────────────────────────────────────────────────────────────
// 6. events_for_branch isolates by branch
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn events_for_branch_returns_only_that_branchs_events() {
    // given
    let registry = CommitProvenanceRegistry::new();
    registry.record_push("p1", "feat/a", "/wt/x", None, lin(&["p1"]), None);
    registry.record_push("p2", "feat/b", "/wt/x", None, lin(&["p2"]), None);
    registry.record_push("p3", "feat/a", "/wt/y", None, lin(&["p3"]), None);
    registry.record_push("p4", "feat/c", "/wt/z", None, lin(&["p4"]), None);

    // when
    let feat_a = registry.events_for_branch("feat/a");
    let feat_b = registry.events_for_branch("feat/b");

    // then
    assert_eq!(feat_a.len(), 2);
    assert_eq!(feat_b.len(), 1);
    assert!(feat_a
        .iter()
        .all(|e| e.provenance.branch == "feat/a"));
}

// ──────────────────────────────────────────────────────────────────────────────
// 7. latest_per_branch reflects most recent push per branch
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn latest_per_branch_returns_most_recent_sha_for_each_branch() {
    // given
    let registry = CommitProvenanceRegistry::new();
    registry.record_push("v1", "main", "/wt/a", None, lin(&["v1"]), None);
    registry.record_push("v2", "main", "/wt/a", None, lin(&["v1", "v2"]), None);
    registry.record_push("v3", "main", "/wt/a", None, lin(&["v1", "v2", "v3"]), None);
    registry.record_push("f1", "feat/z", "/wt/b", None, lin(&["f1"]), None);

    // when
    let map = registry.latest_per_branch();

    // then
    assert_eq!(map.get("main").map(String::as_str), Some("v3"));
    assert_eq!(map.get("feat/z").map(String::as_str), Some("f1"));
    assert_eq!(map.len(), 2);
}

// ──────────────────────────────────────────────────────────────────────────────
// 8. Sequence numbers are monotonically increasing
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn push_event_seq_numbers_are_monotonically_increasing() {
    // given
    let registry = CommitProvenanceRegistry::new();

    // when
    let events: Vec<PushEvent> = (0..6u32)
        .map(|i| {
            registry.record_push(
                format!("sha{i}"),
                "main",
                "/wt/seq",
                None,
                lin(&[]),
                None,
            )
        })
        .collect();

    // then
    for window in events.windows(2) {
        assert!(
            window[0].seq < window[1].seq,
            "seq must be strictly increasing: {} ≮ {}",
            window[0].seq,
            window[1].seq
        );
    }
    assert_eq!(events[0].seq + 5, events[5].seq);
}

// ──────────────────────────────────────────────────────────────────────────────
// 9. PushEvent JSON round-trip
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn push_event_serializes_to_json_and_round_trips_without_loss() {
    // given
    let registry = CommitProvenanceRegistry::new();
    let original = registry.record_push(
        "deadbeef",
        "feat/ser",
        "/worktrees/ser",
        Some("prev_sha".to_string()),
        lin(&["prev_sha", "deadbeef"]),
        Some("fix: serialization".to_string()),
    );

    // when
    let json = serde_json::to_string(&original).expect("serialize");
    let back: PushEvent = serde_json::from_str(&json).expect("deserialize");

    // then
    assert_eq!(original, back);
    assert!(json.contains("commit.pushed"), "event name in JSON");
    assert!(json.contains("deadbeef"), "SHA in JSON");
    assert!(json.contains("feat/ser"), "branch in JSON");
    assert!(json.contains("superseded_by"), "superseded_by key in JSON");
    assert!(json.contains("prev_sha"), "superseded SHA in JSON");
    assert!(json.contains("lineage"), "lineage key in JSON");
    assert!(json.contains("worktree"), "worktree key in JSON");
    assert!(json.contains("branch"), "branch key in JSON");
    assert!(json.contains("pushed_at"), "timestamp in JSON");
}

// ──────────────────────────────────────────────────────────────────────────────
// 10. CommitLineage Display formats as arrow chain
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn commit_lineage_display_formats_as_arrow_separated_chain() {
    // given
    let lineage = CommitLineage::from_shas(["abc", "def", "ghi"]);

    // when
    let s = lineage.to_string();

    // then
    assert_eq!(s, "[abc → def → ghi]");
}

// ──────────────────────────────────────────────────────────────────────────────
// 11. PushEvent Display includes worktree, branch, SHA
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn push_event_display_includes_worktree_branch_and_sha() {
    // given
    let registry = CommitProvenanceRegistry::new();
    let event = registry.record_push(
        "sha99",
        "feat/ui",
        "/wt/display-test",
        None,
        lin(&["sha99"]),
        None,
    );

    // when
    let s = event.to_string();

    // then
    assert!(s.contains("/wt/display-test"), "display must include worktree: {s}");
    assert!(s.contains("feat/ui"), "display must include branch: {s}");
    assert!(s.contains("sha99"), "display must include SHA: {s}");
}

// ──────────────────────────────────────────────────────────────────────────────
// 12. Empty registry returns empty lineage for unknown pair
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn empty_registry_returns_empty_lineage_for_unknown_worktree_branch_pair() {
    // given
    let registry = CommitProvenanceRegistry::new();

    // when
    let result = registry.lineage_for("/wt/ghost", "feat/ghost");

    // then
    assert!(result.is_empty());
    assert_eq!(result.tip(), None);
    assert_eq!(result.len(), 0);
}

// ──────────────────────────────────────────────────────────────────────────────
// 13. event_count tracks total pushes
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn event_count_reflects_total_number_of_recorded_pushes() {
    // given
    let registry = CommitProvenanceRegistry::new();
    assert_eq!(registry.event_count(), 0, "should start empty");

    // when — record 7 pushes across different worktrees and branches
    for i in 0..7u32 {
        registry.record_push(
            format!("sha{i}"),
            format!("feat/branch-{i}"),
            format!("/wt/worker-{i}"),
            None,
            lin(&[]),
            None,
        );
    }

    // then
    assert_eq!(registry.event_count(), 7);
}

// ──────────────────────────────────────────────────────────────────────────────
// 14. Parallel worktrees on same branch have independent lineages
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn parallel_worktrees_on_same_branch_have_independent_lineages() {
    // given — two workers both pushing to the same branch (e.g. race condition
    // that branch-lock would prevent, but provenance still tracks correctly)
    let registry = CommitProvenanceRegistry::new();
    let branch = "feat/parallel";

    registry.record_push("wt_a_sha1", branch, "/wt/a", None, lin(&["wt_a_sha1"]), None);
    registry.record_push("wt_b_sha1", branch, "/wt/b", None, lin(&["wt_b_sha1"]), None);
    registry.record_push(
        "wt_a_sha2",
        branch,
        "/wt/a",
        None,
        lin(&["wt_a_sha1", "wt_a_sha2"]),
        None,
    );

    // when
    let lin_a = registry.lineage_for("/wt/a", branch);
    let lin_b = registry.lineage_for("/wt/b", branch);

    // then — each worktree has its own independent lineage
    assert_eq!(lin_a.shas(), &["wt_a_sha1", "wt_a_sha2"]);
    assert_eq!(lin_b.shas(), &["wt_b_sha1"]);
    assert!(!lin_a.contains("wt_b_sha1"));
    assert!(!lin_b.contains("wt_a_sha1"));
}

// ──────────────────────────────────────────────────────────────────────────────
// 15. superseded_by is omitted from JSON when None
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn superseded_by_is_omitted_from_json_when_not_set() {
    // given
    let registry = CommitProvenanceRegistry::new();
    let event = registry.record_push(
        "cleansha",
        "main",
        "/wt/clean",
        None, // no superseded_by
        lin(&["cleansha"]),
        None,
    );

    // when
    let json = serde_json::to_string(&event).expect("serialize");

    // then — superseded_by key must NOT appear when absent (skip_serializing_if)
    assert!(
        !json.contains("superseded_by"),
        "superseded_by should be absent from JSON when not set, got: {json}"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 16. CommitProvenanceRecord is independently serializable
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn commit_provenance_record_serializes_independently() {
    // given
    let record = CommitProvenanceRecord {
        commit_sha: "abc".to_string(),
        branch: "main".to_string(),
        worktree: "/wt/test".to_string(),
        superseded_by: None,
        lineage: lin(&["abc"]),
        pushed_at: 1_000_000,
        summary: None,
    };

    // when
    let json = serde_json::to_string(&record).expect("serialize");
    let back: CommitProvenanceRecord = serde_json::from_str(&json).expect("deserialize");

    // then
    assert_eq!(record, back);
    assert!(json.contains("commit_sha"));
    assert!(json.contains("branch"));
    assert!(json.contains("worktree"));
    assert!(json.contains("lineage"));
    assert!(json.contains("pushed_at"));
}
