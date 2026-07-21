//! The shared task hub: the meeting point between the removal engine (which
//! blocks waiting for a human) and the web dashboard (where the human resolves
//! tasks).
//!
//! An engine worker thread hits a step it can't automate, enqueues a task here,
//! and blocks on a channel. The dashboard lists pending tasks; when the user
//! clicks "Done" or "Skip", the HTTP handler resolves the task, unblocking the
//! worker. This is the [`odr_engine::HumanInterface`] contract fulfilled over
//! the web instead of the terminal.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use odr_engine::{HumanInterface, HumanResponse, HumanTask, InteractionError};
use serde::Serialize;

/// A task awaiting a human, as shown on the dashboard.
#[derive(Debug, Clone, Serialize)]
pub struct PendingTask {
    pub id: u64,
    pub broker_id: String,
    pub prompt: String,
    /// Category label (e.g. `Captcha`, `Verification`) for the UI.
    pub kind: String,
    pub created: DateTime<Utc>,
}

#[derive(Default)]
struct Inner {
    tasks: Vec<PendingTask>,
    resolvers: HashMap<u64, Sender<HumanResponse>>,
    next_id: u64,
    completed: u64,
    skipped: u64,
}

/// Thread-safe registry of pending human tasks and their waiting workers.
#[derive(Default)]
pub struct Hub {
    inner: Mutex<Inner>,
}

impl Hub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Snapshot of the currently-pending tasks, oldest first.
    pub fn pending(&self) -> Vec<PendingTask> {
        self.inner.lock().expect("hub lock").tasks.clone()
    }

    /// Number of tasks currently awaiting a human.
    pub fn pending_count(&self) -> usize {
        self.inner.lock().expect("hub lock").tasks.len()
    }

    /// Count of tasks resolved so far, as `(completed, skipped)`.
    pub fn resolved_counts(&self) -> (u64, u64) {
        let inner = self.inner.lock().expect("hub lock");
        (inner.completed, inner.skipped)
    }

    /// Resolve a task by id, unblocking the worker waiting on it. Returns
    /// `false` if no such task is pending (already resolved, or bad id).
    pub fn resolve(&self, id: u64, response: HumanResponse) -> bool {
        let mut inner = self.inner.lock().expect("hub lock");
        let Some(pos) = inner.tasks.iter().position(|t| t.id == id) else {
            return false;
        };
        inner.tasks.remove(pos);
        if let Some(tx) = inner.resolvers.remove(&id) {
            // If the worker is gone the send just fails; nothing to do.
            let _ = tx.send(response);
        }
        match response {
            HumanResponse::Completed => inner.completed += 1,
            HumanResponse::Skipped => inner.skipped += 1,
        }
        true
    }

    /// Enqueue a task and return the channel the caller blocks on for the
    /// human's response. Shared by [`WebPrompter`] and the manual-broker path.
    pub fn enqueue(&self, broker_id: &str, prompt: &str, kind: &str) -> Receiver<HumanResponse> {
        let (tx, rx) = channel();
        let mut inner = self.inner.lock().expect("hub lock");
        inner.next_id += 1;
        let id = inner.next_id;
        inner.tasks.push(PendingTask {
            id,
            broker_id: broker_id.to_string(),
            prompt: prompt.to_string(),
            kind: kind.to_string(),
            created: Utc::now(),
        });
        inner.resolvers.insert(id, tx);
        rx
    }
}

/// A [`HumanInterface`] that surfaces tasks on the web dashboard instead of the
/// terminal. Cloneable-per-worker via the shared [`Hub`].
pub struct WebPrompter {
    hub: Arc<Hub>,
}

impl WebPrompter {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self { hub }
    }
}

impl HumanInterface for WebPrompter {
    fn request(&mut self, task: &HumanTask) -> Result<HumanResponse, InteractionError> {
        let kind = format!("{:?}", task.kind);
        let rx = self.hub.enqueue(&task.broker_id, &task.prompt, &kind);
        // Block this worker until the dashboard resolves the task.
        rx.recv().map_err(|_| InteractionError::Closed)
    }
}
