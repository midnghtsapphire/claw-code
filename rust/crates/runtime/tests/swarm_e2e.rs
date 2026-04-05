//! E8-3: End-to-end swarm simulation tests.
//!
//! These tests simulate a real parallel-worker swarm scenario:
//! two workers compete for the same branch, one is blocked by the
//! branch-lock registry, and both workers' push activity is tracked
//! by the commit-provenance registry.  A third set of tests exercises the
//! full save/restore cycle via [`SwarmStateStore`].
//!
//! Scenarios:
//!
//! 1. **Two workers, one branch — first wins, second gets collision**
//! 2. **Collision event carries correct worker identities**
//! 3. **Winning worker's push is recorded in provenance registry**
//! 4. **Blocked worker does not appear in provenance**
//! 5. **After release, second worker can acquire and push**
//! 6. **Three workers race — only first acquires, others collide**
//! 7. **Independent branches are unaffected by parallel locks**
//! 8. **Full save/restore cycle preserves lock + provenance**
//! 9. **Provenance lineage survives save/restore round-trip**
//! 10. **Git push output parsed and recorded end-to-end**
//! 11. **Superseded-by is preserved through state round-trip**
//! 12. **Restore into empty registries yields correct counts**

use std::path::Path;

use runtime::swarm_state::SwarmStateStore;
use runtime::{
    parse_git_push_output, record_push_from_git_output, BranchLockRegistry, CommitLineage,
    CommitProvenanceRegistry,
};

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn lin(shas: &[&str]) -> CommitLineage {
    CommitLineage::from_shas(shas.iter().copied())
}

fn tmp_state_path(suffix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("swarm_e2e_{suffix}.json"))
}

// ──────────────────────────────────────────────────────────────────────────────
// 1. First worker wins the branch lock
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn first_worker_acquires_branch_lock_second_gets_collision() {
    // given
    let locks = BranchLockRegistry::new();

    // when
    let outcome_a = locks.try_acquire("feat/shared", "worker_01");
    let outcome_b = locks.try_acquire("feat/shared", "worker_02");

    // then
    assert!(outcome_a.is_acquired(), "first worker must acquire");
    assert!(outcome_b.is_collision(), "second worker must collide");
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. Collision event carries correct worker identities
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn collision_event_carries_correct_holder_and_challenger() {
    // given
    let locks = BranchLockRegistry::new();
    locks.try_acquire("feat/collision", "worker_A");

    // when
    let outcome = locks.try_acquire("feat/collision", "worker_B");
    let event = outcome
        .collision_event()
        .expect("should have collision event");

    // then
    assert_eq!(event.holder_worker_id, "worker_A");
    assert_eq!(event.challenger_worker_id, "worker_B");
    assert_eq!(event.branch, "feat/collision");
    assert!(
        event.message.contains("worker_A"),
        "message must name holder"
    );
    assert!(
        event.message.contains("worker_B"),
        "message must name challenger"
    );
    assert!(
        event.message.contains("feat/collision"),
        "message must name branch"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// 3. Winning worker's push is recorded in provenance registry
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn winning_worker_push_is_recorded_in_provenance() {
    // given
    let locks = BranchLockRegistry::new();
    let prov = CommitProvenanceRegistry::new();
    let branch = "feat/work";
    let wt = "/wt/worker_01";

    // worker_01 wins the lock
    let outcome = locks.try_acquire(branch, "worker_01");
    assert!(outcome.is_acquired());

    // when — simulate push after successful spawn
    let mut lineage = CommitLineage::new();
    lineage.push_sha("sha_first");
    prov.record_push("sha_first", branch, wt, None, lineage, None);

    // then
    assert_eq!(prov.event_count(), 1);
    let events = prov.events_for_worktree(wt);
    assert_eq!(events[0].provenance.branch, branch);
    assert_eq!(events[0].provenance.commit_sha, "sha_first");
}

// ──────────────────────────────────────────────────────────────────────────────
// 4. Blocked worker does not appear in provenance
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn blocked_worker_has_no_provenance_entries() {
    // given
    let locks = BranchLockRegistry::new();
    let prov = CommitProvenanceRegistry::new();
    let branch = "feat/blocked";

    locks.try_acquire(branch, "worker_01");
    let blocked = locks.try_acquire(branch, "worker_02");
    assert!(blocked.is_collision());

    // when — the blocked worker does NOT proceed and records no push
    // (this is the orchestrator's responsibility; provenance should be empty)

    // then
    assert_eq!(prov.events_for_worktree("/wt/worker_02").len(), 0);
    assert_eq!(prov.event_count(), 0);
}

// ──────────────────────────────────────────────────────────────────────────────
// 5. After release, second worker can acquire and push
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn second_worker_can_acquire_after_first_releases() {
    // given
    let locks = BranchLockRegistry::new();
    let prov = CommitProvenanceRegistry::new();
    let branch = "feat/sequential";

    locks.try_acquire(branch, "worker_01");
    prov.record_push("sha1", branch, "/wt/w1", None, lin(&["sha1"]), None);
    locks.release(branch, "worker_01").expect("release");

    // when
    let outcome = locks.try_acquire(branch, "worker_02");

    // then
    assert!(
        outcome.is_acquired(),
        "worker_02 must acquire after worker_01 releases"
    );
    prov.record_push("sha2", branch, "/wt/w2", None, lin(&["sha2"]), None);
    assert_eq!(prov.event_count(), 2);
    let latest = prov.latest_per_branch();
    assert_eq!(latest.get(branch).map(String::as_str), Some("sha2"));
}

// ──────────────────────────────────────────────────────────────────────────────
// 6. Three workers race — only first acquires, all others collide
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn three_workers_race_only_first_acquires() {
    // given
    let locks = BranchLockRegistry::new();
    let branch = "feat/race";

    // when
    let outcomes: Vec<_> = (1..=3)
        .map(|i| locks.try_acquire(branch, &format!("worker_{i:02}")))
        .collect();

    // then
    let acquired: Vec<_> = outcomes.iter().filter(|o| o.is_acquired()).collect();
    let collisions: Vec<_> = outcomes.iter().filter(|o| o.is_collision()).collect();
    assert_eq!(acquired.len(), 1, "exactly one worker acquires");
    assert_eq!(collisions.len(), 2, "the other two collide");
    assert_eq!(locks.lock_count(), 1);
}

// ──────────────────────────────────────────────────────────────────────────────
// 7. Independent branches are unaffected by parallel locks
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn workers_on_different_branches_do_not_interfere() {
    // given
    let locks = BranchLockRegistry::new();
    let prov = CommitProvenanceRegistry::new();

    // when
    let o1 = locks.try_acquire("feat/a", "worker_01");
    let o2 = locks.try_acquire("feat/b", "worker_02");
    let o3 = locks.try_acquire("feat/c", "worker_03");

    prov.record_push("sha_a", "feat/a", "/wt/w1", None, lin(&["sha_a"]), None);
    prov.record_push("sha_b", "feat/b", "/wt/w2", None, lin(&["sha_b"]), None);
    prov.record_push("sha_c", "feat/c", "/wt/w3", None, lin(&["sha_c"]), None);

    // then
    assert!(o1.is_acquired());
    assert!(o2.is_acquired());
    assert!(o3.is_acquired());
    assert_eq!(locks.lock_count(), 3);
    assert_eq!(prov.event_count(), 3);
    assert_eq!(prov.latest_per_branch().len(), 3);
}

// ──────────────────────────────────────────────────────────────────────────────
// 8. Full save/restore cycle preserves lock + provenance
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn save_and_restore_preserves_locks_and_provenance() {
    // given
    let state_path = tmp_state_path("8");
    let store = SwarmStateStore::new(Path::new(&state_path));
    let locks = BranchLockRegistry::new();
    let prov = CommitProvenanceRegistry::new();

    locks.try_acquire("feat/x", "worker_01");
    locks.try_acquire("feat/y", "worker_02");
    prov.record_push("sha_x", "feat/x", "/wt/w1", None, lin(&["sha_x"]), None);
    prov.record_push("sha_y", "feat/y", "/wt/w2", None, lin(&["sha_y"]), None);

    // when
    store.save(&locks, &prov).expect("save");
    let locks2 = BranchLockRegistry::new();
    let prov2 = CommitProvenanceRegistry::new();
    store.load(&locks2, &prov2).expect("load");

    // then — locks restored
    assert_eq!(locks2.lock_count(), 2);
    assert!(locks2.is_locked("feat/x"));
    assert!(locks2.is_locked("feat/y"));

    // then — provenance restored
    assert_eq!(prov2.event_count(), 2);
    assert_eq!(
        prov2.latest_per_branch().get("feat/x").map(String::as_str),
        Some("sha_x")
    );

    let _ = store.clear();
}

// ──────────────────────────────────────────────────────────────────────────────
// 9. Provenance lineage survives save/restore round-trip
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn provenance_lineage_survives_save_restore_round_trip() {
    // given
    let state_path = tmp_state_path("9");
    let store = SwarmStateStore::new(Path::new(&state_path));
    let locks = BranchLockRegistry::new();
    let prov = CommitProvenanceRegistry::new();
    let wt = "/wt/chained";
    let branch = "feat/chain";

    let mut acc = CommitLineage::new();
    for sha in &["a1", "a2", "a3"] {
        acc.push_sha(*sha);
        prov.record_push(*sha, branch, wt, None, acc.clone(), None);
    }

    // when
    store.save(&locks, &prov).expect("save");
    let locks2 = BranchLockRegistry::new();
    let prov2 = CommitProvenanceRegistry::new();
    store.load(&locks2, &prov2).expect("load");

    // then
    let lineage = prov2.lineage_for(wt, branch);
    assert_eq!(lineage.shas(), &["a1", "a2", "a3"]);
    assert_eq!(lineage.tip(), Some("a3"));

    let _ = store.clear();
}

// ──────────────────────────────────────────────────────────────────────────────
// 10. Git push output parsed and recorded end-to-end
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn git_push_output_is_parsed_and_recorded_in_registry() {
    // given
    let prov = CommitProvenanceRegistry::new();
    let wt = "/wt/git-push-test";
    // Typical `git push` stderr output format.
    let stderr = "   abc123..def456  feat/git-test -> origin/feat/git-test";

    // when
    let events = record_push_from_git_output(&prov, stderr, wt, Some("fix: test"));

    // then
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].provenance.commit_sha, "def456");
    assert_eq!(events[0].provenance.branch, "feat/git-test");
    assert_eq!(events[0].provenance.worktree, wt);
    assert!(events[0].provenance.superseded_by.is_none());
    assert_eq!(events[0].provenance.lineage.tip(), Some("def456"));
    assert_eq!(prov.event_count(), 1);
}

// ──────────────────────────────────────────────────────────────────────────────
// 11. Superseded-by from forced push is preserved through state round-trip
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn forced_push_superseded_by_is_preserved_through_round_trip() {
    // given
    let state_path = tmp_state_path("11");
    let store = SwarmStateStore::new(Path::new(&state_path));
    let locks = BranchLockRegistry::new();
    let prov = CommitProvenanceRegistry::new();

    // Simulate a forced push (+ flag) in git output.
    let stderr = "+ old_sha..new_sha  feat/forced -> origin/feat/forced";
    let updates = parse_git_push_output(stderr);
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].superseded_sha.as_deref(), Some("old_sha"));

    prov.record_push(
        &updates[0].commit_sha,
        &updates[0].branch,
        "/wt/forced",
        updates[0].superseded_sha.clone(),
        lin(&["new_sha"]),
        None,
    );

    // when
    store.save(&locks, &prov).expect("save");
    let locks2 = BranchLockRegistry::new();
    let prov2 = CommitProvenanceRegistry::new();
    store.load(&locks2, &prov2).expect("load");

    // then
    let events = prov2.all_events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].provenance.superseded_by.as_deref(),
        Some("old_sha")
    );
    assert_eq!(events[0].provenance.commit_sha, "new_sha");

    let _ = store.clear();
}

// ──────────────────────────────────────────────────────────────────────────────
// 12. Restore into empty registries yields correct counts
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn restore_into_empty_registries_yields_expected_counts() {
    // given — build up some state, save it, then restore into fresh registries
    let state_path = tmp_state_path("12");
    let store = SwarmStateStore::new(Path::new(&state_path));
    let locks = BranchLockRegistry::new();
    let prov = CommitProvenanceRegistry::new();

    for i in 0..5u32 {
        let branch = format!("feat/branch-{i}");
        let wt = format!("/wt/worker-{i}");
        let sha = format!("sha{i:08x}");
        let _ = locks.try_acquire(&branch, &format!("worker_{i:02}"));
        prov.record_push(&sha, &branch, &wt, None, lin(&[&sha]), None);
    }

    store.save(&locks, &prov).expect("save");

    // when
    let locks_fresh = BranchLockRegistry::new();
    let prov_fresh = CommitProvenanceRegistry::new();
    let snap = store.load(&locks_fresh, &prov_fresh).expect("load");

    // then
    assert_eq!(locks_fresh.lock_count(), 5);
    assert_eq!(prov_fresh.event_count(), 5);
    assert_eq!(snap.locks.len(), 5);
    assert_eq!(snap.events.len(), 5);
    assert_eq!(prov_fresh.latest_per_branch().len(), 5);

    let _ = store.clear();
}
