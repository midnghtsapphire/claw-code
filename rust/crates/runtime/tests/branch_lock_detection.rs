//! E7-1: Branch-lock detection integration tests.
//!
//! These tests verify that [`BranchLockRegistry`] correctly detects collisions
//! when parallel swarm workers attempt to acquire the same branch simultaneously,
//! emitting a structured [`BranchCollisionEvent`] *before* the spawn proceeds.
//!
//! Scenarios:
//! 1. **First acquisition succeeds** — a free branch returns `Acquired`.
//! 2. **Collision emits structured event** — the second worker gets a
//!    `BranchCollisionEvent` with holder, challenger, branch, and timestamps.
//! 3. **Event is emitted before spawn** — the collision outcome is available
//!    synchronously from `try_acquire` before any spawn logic runs.
//! 4. **Release frees the branch** — after release the branch is unlocked.
//! 5. **Wrong-worker release is rejected** — a worker may not release a lock it
//!    does not hold.
//! 6. **Independent branches do not collide** — two workers on different branches
//!    both succeed.
//! 7. **N workers on same branch** — only the first acquires; the remaining N-1
//!    all receive collision events naming the *original* holder.
//! 8. **Re-acquire after release** — a new worker can acquire after the holder
//!    releases.
//! 9. **Collision event JSON is stable** — serde round-trip preserves every field.
//! 10. **lock_count reflects registry state** — count goes up on acquire, down on
//!     release.
//! 11. **all_locks snapshot** — reflects exactly the currently held set.
//! 12. **Collision message is human-readable** — `BranchCollisionEvent::message`
//!     names both workers and the branch in a single sentence.

use runtime::{BranchAcquireOutcome, BranchCollisionEvent, BranchLockRegistry};

// ──────────────────────────────────────────────────────────────────────────────
// 1. First acquisition succeeds
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn first_worker_acquires_free_branch_successfully() {
    // given
    let registry = BranchLockRegistry::new();

    // when
    let outcome = registry.try_acquire("feat/new-feature", "worker_01");

    // then
    assert!(
        outcome.is_acquired(),
        "first acquire on a free branch should succeed"
    );
    let entry = outcome.lock_entry().expect("entry should be present");
    assert_eq!(entry.branch, "feat/new-feature");
    assert_eq!(entry.holder_worker_id, "worker_01");
    assert!(entry.acquired_at > 0, "acquired_at timestamp should be set");
}

// ──────────────────────────────────────────────────────────────────────────────
// 2. Collision emits structured event with all required fields
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn second_worker_receives_collision_event_with_complete_fields() {
    // given
    let registry = BranchLockRegistry::new();
    registry.try_acquire("feat/shared", "worker_01");

    // when — second worker tries to acquire the same branch
    let outcome = registry.try_acquire("feat/shared", "worker_02");

    // then
    assert!(
        outcome.is_collision(),
        "second acquire on a held branch must be a collision"
    );
    let event = outcome.collision_event().expect("collision event should be present");
    assert_eq!(event.branch, "feat/shared");
    assert_eq!(event.holder_worker_id, "worker_01");
    assert_eq!(event.challenger_worker_id, "worker_02");
    assert!(event.lock_acquired_at > 0);
    assert!(event.detected_at > 0);
    assert!(!event.message.is_empty());
}

// ──────────────────────────────────────────────────────────────────────────────
// 3. Collision detection is synchronous (available before spawn)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn collision_is_detected_synchronously_before_spawn_proceeds() {
    // given
    let registry = BranchLockRegistry::new();
    registry.try_acquire("feat/busy", "worker_01");

    // when — simulated spawn decision point: check result before doing any work
    let mut spawn_aborted = false;
    let outcome = registry.try_acquire("feat/busy", "worker_02");
    if outcome.is_collision() {
        // the orchestrator would abort the spawn here
        spawn_aborted = true;
    }

    // then — spawn was never allowed to proceed
    assert!(
        spawn_aborted,
        "spawn must be blocked synchronously when collision is detected"
    );
    // the registry still shows only one holder (worker_01)
    let holder = registry
        .current_holder("feat/busy")
        .expect("holder should exist");
    assert_eq!(holder.holder_worker_id, "worker_01");
}

// ──────────────────────────────────────────────────────────────────────────────
// 4. Release frees the branch
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn releasing_a_branch_makes_it_available_again() {
    // given
    let registry = BranchLockRegistry::new();
    registry.try_acquire("feat/releasable", "worker_01");
    assert!(registry.is_locked("feat/releasable"));

    // when
    registry
        .release("feat/releasable", "worker_01")
        .expect("release should succeed");

    // then
    assert!(!registry.is_locked("feat/releasable"));
    assert!(registry.current_holder("feat/releasable").is_none());
}

// ──────────────────────────────────────────────────────────────────────────────
// 5. Wrong-worker release is rejected
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn worker_cannot_release_branch_it_does_not_hold() {
    // given
    let registry = BranchLockRegistry::new();
    registry.try_acquire("feat/guarded", "worker_01");

    // when — worker_02 tries to release worker_01's lock
    let result = registry.release("feat/guarded", "worker_02");

    // then
    assert!(result.is_err(), "wrong-worker release must be rejected");
    let msg = result.unwrap_err();
    assert!(msg.contains("worker_01"), "error should name the holder");
    assert!(msg.contains("worker_02"), "error should name the challenger");
    // lock is still held by worker_01
    assert!(registry.is_locked("feat/guarded"));
}

// ──────────────────────────────────────────────────────────────────────────────
// 6. Independent branches do not collide
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn two_workers_on_different_branches_both_acquire_successfully() {
    // given
    let registry = BranchLockRegistry::new();

    // when
    let a = registry.try_acquire("feat/alpha", "worker_01");
    let b = registry.try_acquire("feat/beta", "worker_02");

    // then — no collision
    assert!(a.is_acquired(), "worker_01 should acquire feat/alpha");
    assert!(b.is_acquired(), "worker_02 should acquire feat/beta");
    assert_eq!(registry.lock_count(), 2);
}

// ──────────────────────────────────────────────────────────────────────────────
// 7. N workers on same branch — only first wins
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn only_first_worker_acquires_when_n_workers_race_for_same_branch() {
    // given
    let registry = BranchLockRegistry::new();
    let branch = "feat/hotly-contested";
    let workers = ["worker_01", "worker_02", "worker_03", "worker_04"];

    // when
    let outcomes: Vec<BranchAcquireOutcome> = workers
        .iter()
        .map(|w| registry.try_acquire(branch, w))
        .collect();

    // then
    let acquired: Vec<_> = outcomes.iter().filter(|o| o.is_acquired()).collect();
    let collisions: Vec<_> = outcomes.iter().filter(|o| o.is_collision()).collect();

    assert_eq!(acquired.len(), 1, "exactly one worker should win the race");
    assert_eq!(
        collisions.len(),
        3,
        "the remaining 3 workers should all get collision events"
    );
    // every collision must name worker_01 as the holder (it won)
    for outcome in &collisions {
        let event = outcome.collision_event().unwrap();
        assert_eq!(
            event.holder_worker_id, "worker_01",
            "all collision events must name the original winner as holder"
        );
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 8. Re-acquire after release
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn branch_can_be_reacquired_by_new_worker_after_release() {
    // given
    let registry = BranchLockRegistry::new();
    registry.try_acquire("feat/sequential", "worker_01");
    registry
        .release("feat/sequential", "worker_01")
        .expect("release");

    // when — worker_02 tries after worker_01 released
    let outcome = registry.try_acquire("feat/sequential", "worker_02");

    // then
    assert!(outcome.is_acquired(), "worker_02 should acquire after worker_01 released");
    let entry = outcome.lock_entry().unwrap();
    assert_eq!(entry.holder_worker_id, "worker_02");
}

// ──────────────────────────────────────────────────────────────────────────────
// 9. Collision event JSON round-trip
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn collision_event_json_round_trips_preserving_all_fields() {
    // given
    let registry = BranchLockRegistry::new();
    registry.try_acquire("feat/serialized", "worker_01");
    let outcome = registry.try_acquire("feat/serialized", "worker_02");
    let event = outcome.collision_event().expect("collision");

    // when
    let json = serde_json::to_string(event).expect("serialize");
    let back: BranchCollisionEvent = serde_json::from_str(&json).expect("deserialize");

    // then
    assert_eq!(event, &back);
    assert!(json.contains("feat/serialized"));
    assert!(json.contains("worker_01"));
    assert!(json.contains("worker_02"));
    assert!(json.contains("holder_worker_id"));
    assert!(json.contains("challenger_worker_id"));
    assert!(json.contains("lock_acquired_at"));
    assert!(json.contains("detected_at"));
}

// ──────────────────────────────────────────────────────────────────────────────
// 10. lock_count reflects registry state
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn lock_count_tracks_acquire_and_release() {
    // given
    let registry = BranchLockRegistry::new();
    assert_eq!(registry.lock_count(), 0);

    // when — acquire three branches
    registry.try_acquire("feat/a", "w1");
    registry.try_acquire("feat/b", "w2");
    registry.try_acquire("feat/c", "w3");
    assert_eq!(registry.lock_count(), 3);

    // release one
    registry.release("feat/b", "w2").expect("release");
    assert_eq!(registry.lock_count(), 2);

    // release the rest
    registry.release("feat/a", "w1").expect("release");
    registry.release("feat/c", "w3").expect("release");
    assert_eq!(registry.lock_count(), 0);
}

// ──────────────────────────────────────────────────────────────────────────────
// 11. all_locks snapshot
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn all_locks_snapshot_contains_exactly_held_branches() {
    // given
    let registry = BranchLockRegistry::new();
    registry.try_acquire("feat/x", "wx");
    registry.try_acquire("feat/y", "wy");
    registry.try_acquire("feat/z", "wz");

    // when
    let locks = registry.all_locks();
    let branches: Vec<String> = locks.iter().map(|l| l.branch.clone()).collect();

    // then
    assert_eq!(locks.len(), 3);
    assert!(branches.contains(&"feat/x".to_string()));
    assert!(branches.contains(&"feat/y".to_string()));
    assert!(branches.contains(&"feat/z".to_string()));

    // after releasing feat/y the snapshot shrinks
    registry.release("feat/y", "wy").expect("release");
    let locks_after = registry.all_locks();
    assert_eq!(locks_after.len(), 2);
    let branches_after: Vec<String> = locks_after.iter().map(|l| l.branch.clone()).collect();
    assert!(!branches_after.contains(&"feat/y".to_string()));
}

// ──────────────────────────────────────────────────────────────────────────────
// 12. Collision message is human-readable
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn collision_message_names_both_workers_and_branch_in_single_sentence() {
    // given
    let registry = BranchLockRegistry::new();
    registry.try_acquire("feat/readable", "worker_alpha");
    let outcome = registry.try_acquire("feat/readable", "worker_beta");
    let event = outcome.collision_event().expect("collision");

    // when
    let msg = event.to_string(); // uses Display impl

    // then
    assert!(
        msg.contains("worker_alpha"),
        "message should name holder, got: {msg}"
    );
    assert!(
        msg.contains("worker_beta"),
        "message should name challenger, got: {msg}"
    );
    assert!(
        msg.contains("feat/readable"),
        "message should name the branch, got: {msg}"
    );
}
