//! Branch-lock detection for parallel swarm workers.
//!
//! When multiple workers are spawned in parallel they must not operate on the
//! same Git branch simultaneously — doing so causes conflicting commits, lost
//! work, and corrupted history.  This module provides an in-memory
//! [`BranchLockRegistry`] that workers consult *before* spawning:
//!
//! 1. Call [`BranchLockRegistry::try_acquire`] with the desired branch name and
//!    a worker identifier.
//! 2. If the branch is free, the lock is granted and the caller receives
//!    [`BranchAcquireOutcome::Acquired`].
//! 3. If the branch is already held, the caller receives
//!    [`BranchAcquireOutcome::Collision`] with a [`BranchCollisionEvent`] that
//!    the orchestrator can inspect or emit as a structured log event before
//!    aborting or queuing the spawn.
//! 4. When the worker finishes (success or failure), call
//!    [`BranchLockRegistry::release`] to free the lock.
//!
//! The registry is wrapped in an `Arc<Mutex<…>>` so it is safe to share across
//! threads.

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
// Public types
// ──────────────────────────────────────────────────────────────────────────────

/// A record of which worker currently holds a branch and when it was acquired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchLockEntry {
    /// The branch name that is locked.
    pub branch: String,
    /// The worker that holds this lock.
    pub holder_worker_id: String,
    /// Unix timestamp (seconds) when the lock was acquired.
    pub acquired_at: u64,
}

/// The structured event emitted when a collision is detected.
///
/// This event is produced *before* spawning the conflicting worker so the
/// orchestrator has a chance to log, delay, or abort the spawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchCollisionEvent {
    /// The branch for which a collision was detected.
    pub branch: String,
    /// The worker that is already holding the branch.
    pub holder_worker_id: String,
    /// The worker that attempted to acquire but was blocked.
    pub challenger_worker_id: String,
    /// When the existing lock was originally acquired.
    pub lock_acquired_at: u64,
    /// When this collision event was generated.
    pub detected_at: u64,
    /// Human-readable message suitable for structured logs or UI display.
    pub message: String,
}

impl BranchCollisionEvent {
    fn new(entry: &BranchLockEntry, challenger: &str) -> Self {
        let detected_at = now_secs();
        Self {
            branch: entry.branch.clone(),
            holder_worker_id: entry.holder_worker_id.clone(),
            challenger_worker_id: challenger.to_string(),
            lock_acquired_at: entry.acquired_at,
            detected_at,
            message: format!(
                "branch collision: worker `{challenger}` attempted to acquire `{}` \
                 but it is already held by `{}` (acquired at t={}) — \
                 spawn blocked until lock is released",
                entry.branch, entry.holder_worker_id, entry.acquired_at,
            ),
        }
    }
}

impl std::fmt::Display for BranchCollisionEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// The result of attempting to acquire a branch lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchAcquireOutcome {
    /// The lock was successfully acquired; the worker may proceed to spawn.
    Acquired(BranchLockEntry),
    /// The branch is already held; the structured collision event is enclosed.
    Collision(BranchCollisionEvent),
}

impl BranchAcquireOutcome {
    /// Returns `true` if the branch was successfully acquired.
    #[must_use]
    pub fn is_acquired(&self) -> bool {
        matches!(self, Self::Acquired(_))
    }

    /// Returns `true` if a collision was detected.
    #[must_use]
    pub fn is_collision(&self) -> bool {
        matches!(self, Self::Collision(_))
    }

    /// Returns the collision event, or `None` if the lock was acquired.
    #[must_use]
    pub fn collision_event(&self) -> Option<&BranchCollisionEvent> {
        match self {
            Self::Collision(event) => Some(event),
            Self::Acquired(_) => None,
        }
    }

    /// Returns the lock entry, or `None` if a collision occurred.
    #[must_use]
    pub fn lock_entry(&self) -> Option<&BranchLockEntry> {
        match self {
            Self::Acquired(entry) => Some(entry),
            Self::Collision(_) => None,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Registry
// ──────────────────────────────────────────────────────────────────────────────

/// Thread-safe in-memory registry of branch locks for parallel swarm workers.
///
/// Each branch can be held by at most one worker at a time.  Workers call
/// [`try_acquire`](Self::try_acquire) before spawning and [`release`](Self::release)
/// when they complete.  Parallel workers that attempt to take the same branch
/// receive a [`BranchCollisionEvent`] and must wait or abort.
#[derive(Debug, Clone, Default)]
pub struct BranchLockRegistry {
    inner: Arc<Mutex<BranchLockRegistryInner>>,
}

#[derive(Debug, Default)]
struct BranchLockRegistryInner {
    locks: BTreeMap<String, BranchLockEntry>,
}

impl BranchLockRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempts to acquire a lock on `branch` for `worker_id`.
    ///
    /// * If the branch is free, records the lock and returns
    ///   [`BranchAcquireOutcome::Acquired`].
    /// * If the branch is already held by any worker (including the same
    ///   `worker_id`), returns [`BranchAcquireOutcome::Collision`] with a
    ///   structured [`BranchCollisionEvent`].
    #[must_use]
    pub fn try_acquire(&self, branch: &str, worker_id: &str) -> BranchAcquireOutcome {
        let mut inner = self.inner.lock().expect("branch lock registry poisoned");
        if let Some(existing) = inner.locks.get(branch) {
            return BranchAcquireOutcome::Collision(BranchCollisionEvent::new(existing, worker_id));
        }
        let entry = BranchLockEntry {
            branch: branch.to_string(),
            holder_worker_id: worker_id.to_string(),
            acquired_at: now_secs(),
        };
        inner.locks.insert(branch.to_string(), entry.clone());
        BranchAcquireOutcome::Acquired(entry)
    }

    /// Releases the lock on `branch` held by `worker_id`.
    ///
    /// Returns `Ok(())` if the lock was held by the given worker and has been
    /// released.  Returns `Err` if:
    /// - the branch is not currently locked, or
    /// - the branch is locked by a *different* worker (safety guard).
    pub fn release(&self, branch: &str, worker_id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().expect("branch lock registry poisoned");
        match inner.locks.get(branch) {
            None => Err(format!(
                "cannot release branch `{branch}`: no lock is currently held"
            )),
            Some(entry) if entry.holder_worker_id != worker_id => Err(format!(
                "cannot release branch `{branch}`: held by `{}`, not `{worker_id}`",
                entry.holder_worker_id
            )),
            Some(_) => {
                inner.locks.remove(branch);
                Ok(())
            }
        }
    }

    /// Returns `true` if `branch` is currently locked by any worker.
    #[must_use]
    pub fn is_locked(&self, branch: &str) -> bool {
        let inner = self.inner.lock().expect("branch lock registry poisoned");
        inner.locks.contains_key(branch)
    }

    /// Returns the current lock entry for `branch`, if any.
    #[must_use]
    pub fn current_holder(&self, branch: &str) -> Option<BranchLockEntry> {
        let inner = self.inner.lock().expect("branch lock registry poisoned");
        inner.locks.get(branch).cloned()
    }

    /// Returns the number of branches currently locked.
    #[must_use]
    pub fn lock_count(&self) -> usize {
        let inner = self.inner.lock().expect("branch lock registry poisoned");
        inner.locks.len()
    }

    /// Returns all currently held locks, sorted by branch name.
    #[must_use]
    pub fn all_locks(&self) -> Vec<BranchLockEntry> {
        let inner = self.inner.lock().expect("branch lock registry poisoned");
        inner.locks.values().cloned().collect()
    }

    /// Returns a serde-serializable snapshot of all currently held locks.
    ///
    /// Use [`restore_from_snapshot`](Self::restore_from_snapshot) to reload
    /// this snapshot after a process restart, e.g. from `.claw/swarm-state.json`.
    #[must_use]
    pub fn snapshot(&self) -> Vec<BranchLockEntry> {
        self.all_locks()
    }

    /// Restores lock state from a previously captured snapshot.
    ///
    /// Any locks currently held in the registry are replaced by the snapshot
    /// entries.  This is intended for startup-time state recovery; during normal
    /// operation, use [`try_acquire`](Self::try_acquire) and
    /// [`release`](Self::release) instead.
    pub fn restore_from_snapshot(&self, snapshot: Vec<BranchLockEntry>) {
        let mut inner = self.inner.lock().expect("branch lock registry poisoned");
        inner.locks.clear();
        for entry in snapshot {
            inner.locks.insert(entry.branch.clone(), entry);
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Unit tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_free_branch_returns_acquired() {
        // given
        let registry = BranchLockRegistry::new();

        // when
        let outcome = registry.try_acquire("feat/new-tool", "worker_01");

        // then
        assert!(outcome.is_acquired());
        let entry = outcome.lock_entry().expect("should have entry");
        assert_eq!(entry.branch, "feat/new-tool");
        assert_eq!(entry.holder_worker_id, "worker_01");
    }

    #[test]
    fn acquire_locked_branch_returns_collision_event() {
        // given
        let registry = BranchLockRegistry::new();
        registry.try_acquire("feat/shared", "worker_01");

        // when
        let outcome = registry.try_acquire("feat/shared", "worker_02");

        // then
        assert!(outcome.is_collision());
        let event = outcome.collision_event().expect("should have collision");
        assert_eq!(event.branch, "feat/shared");
        assert_eq!(event.holder_worker_id, "worker_01");
        assert_eq!(event.challenger_worker_id, "worker_02");
        assert!(event.message.contains("worker_01"));
        assert!(event.message.contains("worker_02"));
        assert!(event.message.contains("feat/shared"));
    }

    #[test]
    fn release_held_branch_frees_it_for_next_worker() {
        // given
        let registry = BranchLockRegistry::new();
        registry.try_acquire("main", "worker_01");

        // when
        registry
            .release("main", "worker_01")
            .expect("release should succeed");

        // then
        assert!(!registry.is_locked("main"));
        let second = registry.try_acquire("main", "worker_02");
        assert!(second.is_acquired());
    }

    #[test]
    fn release_by_wrong_worker_returns_error() {
        // given
        let registry = BranchLockRegistry::new();
        registry.try_acquire("feat/x", "worker_01");

        // when
        let result = registry.release("feat/x", "worker_02");

        // then
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("worker_01"));
        assert!(msg.contains("worker_02"));
    }

    #[test]
    fn release_unlocked_branch_returns_error() {
        // given
        let registry = BranchLockRegistry::new();

        // when
        let result = registry.release("feat/ghost", "worker_01");

        // then
        assert!(result.is_err());
    }

    #[test]
    fn same_worker_acquiring_same_branch_twice_is_a_collision() {
        // given — worker_01 already holds the branch
        let registry = BranchLockRegistry::new();
        registry.try_acquire("feat/dup", "worker_01");

        // when — same worker tries again
        let outcome = registry.try_acquire("feat/dup", "worker_01");

        // then — treated as a collision (idempotent acquire is not supported)
        assert!(outcome.is_collision());
    }

    #[test]
    fn different_branches_are_independent_locks() {
        // given
        let registry = BranchLockRegistry::new();

        // when
        let a = registry.try_acquire("feat/a", "worker_01");
        let b = registry.try_acquire("feat/b", "worker_02");

        // then — both succeed independently
        assert!(a.is_acquired());
        assert!(b.is_acquired());
        assert_eq!(registry.lock_count(), 2);
    }

    #[test]
    fn collision_event_serializes_to_json_without_loss() {
        // given
        let registry = BranchLockRegistry::new();
        registry.try_acquire("feat/ser", "worker_01");
        let outcome = registry.try_acquire("feat/ser", "worker_02");
        let event = outcome.collision_event().expect("collision expected");

        // when
        let json = serde_json::to_string(event).expect("serialize");
        let back: BranchCollisionEvent = serde_json::from_str(&json).expect("deserialize");

        // then
        assert_eq!(event, &back);
        assert!(json.contains("feat/ser"));
        assert!(json.contains("worker_01"));
        assert!(json.contains("worker_02"));
    }

    #[test]
    fn all_locks_returns_every_held_entry() {
        // given
        let registry = BranchLockRegistry::new();
        registry.try_acquire("feat/a", "worker_01");
        registry.try_acquire("feat/b", "worker_02");
        registry.try_acquire("feat/c", "worker_03");

        // when
        let locks = registry.all_locks();

        // then
        assert_eq!(locks.len(), 3);
        let branches: Vec<&str> = locks.iter().map(|l| l.branch.as_str()).collect();
        assert!(branches.contains(&"feat/a"));
        assert!(branches.contains(&"feat/b"));
        assert!(branches.contains(&"feat/c"));
    }
}
