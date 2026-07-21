//! Progress statistics for the dashboard, derived from recipes + local state.

use odr_engine::{JsonStore, StateStore, Status};
use odr_recipes::LoadedRecipe;
use serde::Serialize;

/// A point-in-time summary across all known brokers.
#[derive(Debug, Default, Serialize)]
pub struct Stats {
    pub total_brokers: usize,
    pub not_started: usize,
    pub requested: usize,
    pub awaiting_confirmation: usize,
    pub confirmed: usize,
    pub reappeared: usize,
    pub failed: usize,
    /// Brokers whose re-check date has arrived.
    pub recheck_due_now: usize,
    /// Tasks currently waiting on a human.
    pub pending_human_tasks: usize,
    /// Human tasks completed this session.
    pub completed_tasks: u64,
    /// Human tasks skipped this session.
    pub skipped_tasks: u64,
}

/// Compute stats from the recipe set and the current state store.
pub fn compute(recipes: &[LoadedRecipe], store: &JsonStore, pending_human: usize) -> Stats {
    let mut s = Stats {
        total_brokers: recipes.len(),
        pending_human_tasks: pending_human,
        ..Stats::default()
    };
    let now = chrono::Utc::now();
    for r in recipes {
        let rec = store.get(&r.recipe.id);
        match rec.status {
            Status::NotStarted => s.not_started += 1,
            Status::Requested => s.requested += 1,
            Status::AwaitingConfirmation => s.awaiting_confirmation += 1,
            Status::Confirmed => s.confirmed += 1,
            Status::Reappeared => s.reappeared += 1,
            Status::Failed => s.failed += 1,
        }
        if rec.is_recheck_due(now) {
            s.recheck_due_now += 1;
        }
    }
    s
}
