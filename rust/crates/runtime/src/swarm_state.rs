//! Swarm state persistence for the claw-code orchestrator.
//!
//! This module provides [`SwarmStateStore`], which serialises and deserialises
//! the combined in-memory state of:
//!
//! * [`BranchLockRegistry`] — which branches are currently locked by which workers
//! * [`CommitProvenanceRegistry`] — the push-event history for each worktree/branch
//!
//! State is written to a single JSON file (default: `.claw/swarm-state.json`) so
//! that a swarm orchestrator can restart without losing provenance history or
//! inadvertently re-acquiring locks that are still held.
//!
//! # Usage
//!
//! ```rust,no_run
//! use runtime::{BranchLockRegistry, CommitProvenanceRegistry};
//! use runtime::swarm_state::SwarmStateStore;
//! use std::path::Path;
//!
//! let lock_reg   = BranchLockRegistry::new();
//! let prov_reg   = CommitProvenanceRegistry::new();
//! let store      = SwarmStateStore::new(Path::new(".claw/swarm-state.json"));
//!
//! // After each push / lock change:
//! store.save(&lock_reg, &prov_reg).expect("save swarm state");
//!
//! // On restart:
//! store.load(&lock_reg, &prov_reg).expect("restore swarm state");
//! ```
//!
//! The state file is a JSON object with two top-level keys:
//! ```json
//! {
//!   "locks":  [ <BranchLockEntry>, … ],
//!   "events": [ <PushEvent>, … ]
//! }
//! ```
//!
//! # Atomicity
//! Writes are atomic at the OS level: the new content is written to a sibling
//! `.tmp` file and then renamed over the target, so a crash during a write
//! cannot corrupt the previous state.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::branch_lock::{BranchLockEntry, BranchLockRegistry};
use crate::commit_provenance::{CommitProvenanceRegistry, PushEvent};

// ──────────────────────────────────────────────────────────────────────────────
// On-disk format
// ──────────────────────────────────────────────────────────────────────────────

/// The JSON-serialisable envelope stored in the state file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SwarmStateSnapshot {
    /// All currently held branch locks.
    pub locks: Vec<BranchLockEntry>,
    /// All push events recorded during this swarm run, in insertion order.
    pub events: Vec<PushEvent>,
}

impl SwarmStateSnapshot {
    /// Returns `true` if both `locks` and `events` are empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.locks.is_empty() && self.events.is_empty()
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Store
// ──────────────────────────────────────────────────────────────────────────────

/// Persists and restores swarm state (branch locks + commit provenance) to/from
/// a JSON file.
///
/// The store itself is stateless: all mutable data lives in the registries
/// passed to [`save`](Self::save) and [`load`](Self::load).
#[derive(Debug, Clone)]
pub struct SwarmStateStore {
    path: PathBuf,
}

impl SwarmStateStore {
    /// Creates a new store backed by the given path.
    ///
    /// The file (and its parent directory) will be created on the first
    /// [`save`](Self::save) call if they do not already exist.
    #[must_use]
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }

    /// Returns the path of the backing state file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Captures a snapshot from the registries and writes it to disk atomically.
    ///
    /// Creates the parent directory if it does not exist.
    ///
    /// # Errors
    /// Returns an [`io::Error`] if the directory cannot be created, the file
    /// cannot be written, or the atomic rename fails.
    pub fn save(
        &self,
        locks: &BranchLockRegistry,
        provenance: &CommitProvenanceRegistry,
    ) -> io::Result<()> {
        let snapshot = SwarmStateSnapshot {
            locks: locks.snapshot(),
            events: provenance.all_events(),
        };

        let json = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Ensure parent directory exists.
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write atomically via a temp file.
        let tmp_path = self.path.with_extension("tmp");
        fs::write(&tmp_path, &json)?;
        fs::rename(&tmp_path, &self.path)?;

        Ok(())
    }

    /// Reads the state file and restores lock + provenance registries.
    ///
    /// If the file does not exist, this is a no-op (treated as an empty state).
    ///
    /// # Errors
    /// Returns an [`io::Error`] if the file exists but cannot be read or
    /// deserialised.
    pub fn load(
        &self,
        locks: &BranchLockRegistry,
        provenance: &CommitProvenanceRegistry,
    ) -> io::Result<SwarmStateSnapshot> {
        if !self.path.exists() {
            return Ok(SwarmStateSnapshot::default());
        }

        let json = fs::read_to_string(&self.path)?;
        let snapshot: SwarmStateSnapshot = serde_json::from_str(&json)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        locks.restore_from_snapshot(snapshot.locks.clone());

        // Replay events into the provenance registry, preserving insertion order.
        // We use record_push so that seq numbers are re-assigned monotonically.
        for event in &snapshot.events {
            let p = &event.provenance;
            provenance.record_push(
                &p.commit_sha,
                &p.branch,
                &p.worktree,
                p.superseded_by.clone(),
                p.lineage.clone(),
                p.summary.clone(),
            );
        }

        Ok(snapshot)
    }

    /// Returns `true` if the state file exists on disk.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Deletes the state file if it exists.
    ///
    /// # Errors
    /// Returns an [`io::Error`] if the file cannot be deleted.
    pub fn clear(&self) -> io::Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Unit tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_state_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "swarm_state_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        dir.join("swarm-state.json")
    }

    fn lin(shas: &[&str]) -> crate::commit_provenance::CommitLineage {
        crate::commit_provenance::CommitLineage::from_shas(shas.iter().copied())
    }

    #[test]
    fn save_and_load_restores_lock_state() {
        // given
        let path = tmp_state_path();
        let store = SwarmStateStore::new(&path);
        let locks = BranchLockRegistry::new();
        let prov = CommitProvenanceRegistry::new();
        locks.try_acquire("feat/x", "worker_01");
        locks.try_acquire("feat/y", "worker_02");

        // when
        store.save(&locks, &prov).expect("save");
        let locks2 = BranchLockRegistry::new();
        let prov2 = CommitProvenanceRegistry::new();
        store.load(&locks2, &prov2).expect("load");

        // then
        assert_eq!(locks2.lock_count(), 2);
        assert!(locks2.is_locked("feat/x"));
        assert!(locks2.is_locked("feat/y"));
        assert_eq!(
            locks2.current_holder("feat/x").unwrap().holder_worker_id,
            "worker_01"
        );

        let _ = store.clear();
    }

    #[test]
    fn save_and_load_restores_provenance_events() {
        // given
        let path = tmp_state_path();
        let store = SwarmStateStore::new(&path);
        let locks = BranchLockRegistry::new();
        let prov = CommitProvenanceRegistry::new();

        let mut lineage = lin(&[]);
        for sha in &["abc", "def", "ghi"] {
            lineage.push_sha(*sha);
            prov.record_push(*sha, "main", "/wt/a", None, lineage.clone(), None);
        }

        // when
        store.save(&locks, &prov).expect("save");
        let locks2 = BranchLockRegistry::new();
        let prov2 = CommitProvenanceRegistry::new();
        store.load(&locks2, &prov2).expect("load");

        // then
        assert_eq!(prov2.event_count(), 3);
        let lineage2 = prov2.lineage_for("/wt/a", "main");
        assert_eq!(lineage2.shas(), &["abc", "def", "ghi"]);

        let _ = store.clear();
    }

    #[test]
    fn load_is_noop_when_file_does_not_exist() {
        // given
        let path = tmp_state_path();
        let store = SwarmStateStore::new(&path);
        let locks = BranchLockRegistry::new();
        let prov = CommitProvenanceRegistry::new();

        // when
        let snap = store.load(&locks, &prov).expect("load");

        // then
        assert!(snap.is_empty());
        assert_eq!(locks.lock_count(), 0);
        assert_eq!(prov.event_count(), 0);
    }

    #[test]
    fn state_file_is_valid_json() {
        // given
        let path = tmp_state_path();
        let store = SwarmStateStore::new(&path);
        let locks = BranchLockRegistry::new();
        let prov = CommitProvenanceRegistry::new();
        locks.try_acquire("main", "worker_01");
        prov.record_push("sha1", "main", "/wt/x", None, lin(&["sha1"]), None);

        // when
        store.save(&locks, &prov).expect("save");
        let contents = std::fs::read_to_string(&path).expect("read");
        let parsed: serde_json::Value = serde_json::from_str(&contents).expect("parse JSON");

        // then
        assert!(parsed.get("locks").is_some(), "JSON must have 'locks' key");
        assert!(
            parsed.get("events").is_some(),
            "JSON must have 'events' key"
        );

        let _ = store.clear();
    }

    #[test]
    fn exists_returns_false_before_save_and_true_after() {
        // given
        let path = tmp_state_path();
        let store = SwarmStateStore::new(&path);
        let locks = BranchLockRegistry::new();
        let prov = CommitProvenanceRegistry::new();

        // then (before save)
        assert!(!store.exists());

        // when
        store.save(&locks, &prov).expect("save");

        // then (after save)
        assert!(store.exists());

        let _ = store.clear();
    }

    #[test]
    fn clear_removes_state_file() {
        // given
        let path = tmp_state_path();
        let store = SwarmStateStore::new(&path);
        let locks = BranchLockRegistry::new();
        let prov = CommitProvenanceRegistry::new();
        store.save(&locks, &prov).expect("save");
        assert!(store.exists());

        // when
        store.clear().expect("clear");

        // then
        assert!(!store.exists());
    }

    #[test]
    fn round_trip_preserves_superseded_by() {
        // given
        let path = tmp_state_path();
        let store = SwarmStateStore::new(&path);
        let locks = BranchLockRegistry::new();
        let prov = CommitProvenanceRegistry::new();
        prov.record_push(
            "new_sha",
            "main",
            "/wt/a",
            Some("old_sha".to_string()),
            lin(&["old_sha", "new_sha"]),
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

        let _ = store.clear();
    }

    #[test]
    fn snapshot_is_empty_by_default() {
        let snap = SwarmStateSnapshot::default();
        assert!(snap.is_empty());
    }
}
