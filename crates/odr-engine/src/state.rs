//! Per-broker removal state, persisted locally.
//!
//! Recurrence is the whole game: brokers repopulate every 30–90 days, so ODR
//! has to remember what it did and when to come back. This module owns that
//! memory as a per-broker state machine.
//!
//! The current backend is a single JSON file, chosen so the scaffold builds
//! with no native dependencies. The [`StateStore`] trait exists so the intended
//! production backend (SQLite via `rusqlite`) can drop in without touching
//! callers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Where a broker sits in the removal lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// No request has been made yet.
    #[default]
    NotStarted,
    /// A request was submitted; nothing further is required from the user.
    Requested,
    /// Submitted, but the user still has to click/confirm something.
    AwaitingConfirmation,
    /// The broker confirmed removal (or a re-check found nothing).
    Confirmed,
    /// Data was found again on a re-check — time to resubmit.
    Reappeared,
    /// The attempt failed (broken form, broker refusal, unmet requirement).
    Failed,
}

/// The tracked record for one broker.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BrokerRecord {
    pub status: Status,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_requested: Option<DateTime<Utc>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_at: Option<DateTime<Utc>>,

    /// When this broker should next be re-checked for reappearance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recheck_due: Option<DateTime<Utc>>,

    /// Deadline for a pending confirmation link (they expire fast).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm_by: Option<DateTime<Utc>>,

    /// Last human-readable note (e.g. a failure reason).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl BrokerRecord {
    /// True if this broker is due (or overdue) for a re-check as of `now`.
    pub fn is_recheck_due(&self, now: DateTime<Utc>) -> bool {
        matches!(self.recheck_due, Some(due) if due <= now)
    }
}

/// Abstract persistence for broker records.
pub trait StateStore {
    fn get(&self, broker_id: &str) -> BrokerRecord;
    fn set(&mut self, broker_id: &str, record: BrokerRecord);
    fn all(&self) -> &BTreeMap<String, BrokerRecord>;

    /// Mark a broker requested now, scheduling its re-check `recheck_days` out
    /// and, when relevant, a confirmation deadline `confirm_hours` out.
    fn mark_requested(
        &mut self,
        broker_id: &str,
        recheck_days: u32,
        confirm_hours: Option<u32>,
        now: DateTime<Utc>,
    ) {
        let mut rec = self.get(broker_id);
        rec.last_requested = Some(now);
        rec.recheck_due = Some(now + Duration::days(recheck_days as i64));
        match confirm_hours {
            Some(h) => {
                rec.status = Status::AwaitingConfirmation;
                rec.confirm_by = Some(now + Duration::hours(h as i64));
            }
            None => {
                rec.status = Status::Requested;
                rec.confirm_by = None;
            }
        }
        self.set(broker_id, rec);
    }
}

/// A [`StateStore`] backed by a single JSON file on disk.
#[derive(Debug)]
pub struct JsonStore {
    path: PathBuf,
    records: BTreeMap<String, BrokerRecord>,
}

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("could not read state file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not write state file {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not parse state file {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl JsonStore {
    /// Open the store at `path`, starting empty if the file doesn't exist yet.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StateError> {
        let path = path.as_ref().to_path_buf();
        let records = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|source| StateError::Parse {
                path: path.clone(),
                source,
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(source) => {
                return Err(StateError::Read {
                    path: path.clone(),
                    source,
                })
            }
        };
        Ok(Self { path, records })
    }

    /// Persist the current records to disk.
    pub fn save(&self) -> Result<(), StateError> {
        let text = serde_json::to_string_pretty(&self.records).expect("records serialize");
        std::fs::write(&self.path, text).map_err(|source| StateError::Write {
            path: self.path.clone(),
            source,
        })
    }
}

impl StateStore for JsonStore {
    fn get(&self, broker_id: &str) -> BrokerRecord {
        self.records.get(broker_id).cloned().unwrap_or_default()
    }

    fn set(&mut self, broker_id: &str, record: BrokerRecord) {
        self.records.insert(broker_id.to_string(), record);
    }

    fn all(&self) -> &BTreeMap<String, BrokerRecord> {
        &self.records
    }
}
