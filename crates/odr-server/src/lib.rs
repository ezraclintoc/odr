//! # odr-server
//!
//! A local web dashboard for ODR. It runs the removal engine on background
//! worker threads and serves a page showing progress statistics plus the live
//! queue of steps that need a human (CAPTCHAs, ID checks, picking your
//! listing). The user opens the page, clears the tasks as they appear, and the
//! blocked workers continue.
//!
//! This is the [`odr_engine::HumanInterface`] contract fulfilled over the web
//! (see [`hub::WebPrompter`]) rather than the terminal — the same engine, a
//! different front door. See `docs/deployment.md` for how this extends to a
//! remote/Docker deployment where the browser itself is viewed over the web.
//!
//! The current build drives a dry-run browser (the live browser driver is not
//! wired up yet), so form submissions are simulated; the human-in-the-loop
//! queue and all state tracking are real.

mod dashboard;
mod http;
mod hub;
mod stats;

pub use hub::{Hub, PendingTask, WebPrompter};
pub use stats::Stats;

use std::sync::{Arc, Mutex};
use std::thread;

use odr_engine::{execute, DryRunBrowser, JsonStore, Outcome, Profile, StateStore};
use odr_recipes::LoadedRecipe;

/// Start the dashboard: spawn a worker per broker, then serve the UI on `addr`.
///
/// Blocks running the HTTP server until the process is stopped. Workers that
/// reach a human step park until the user resolves the task in the browser.
pub fn run(
    addr: &str,
    recipes: Vec<LoadedRecipe>,
    profile: Profile,
    state_path: std::path::PathBuf,
) -> anyhow::Result<()> {
    let hub = Hub::new();
    let recipes = Arc::new(recipes);
    let store = Arc::new(Mutex::new(JsonStore::open(&state_path)?));

    spawn_workers(&hub, &recipes, Arc::new(profile), &store);

    println!("ODR dashboard running at http://{addr}");
    println!("Open it to clear the human-in-the-loop tasks. Ctrl-C to stop.");
    http::serve(addr, hub, recipes, store)
}

/// Spawn one background worker per recipe. Each runs its opt-out and, at a human
/// step, blocks on the shared [`Hub`] until the dashboard resolves it.
fn spawn_workers(
    hub: &Arc<Hub>,
    recipes: &Arc<Vec<LoadedRecipe>>,
    profile: Arc<Profile>,
    store: &Arc<Mutex<JsonStore>>,
) {
    for loaded in recipes.iter().cloned() {
        let hub = Arc::clone(hub);
        let profile = Arc::clone(&profile);
        let store = Arc::clone(store);
        thread::spawn(move || run_one(loaded, &hub, &profile, &store));
    }
}

fn run_one(loaded: LoadedRecipe, hub: &Arc<Hub>, profile: &Profile, store: &Arc<Mutex<JsonStore>>) {
    let mut browser = DryRunBrowser::default();
    let mut prompter = WebPrompter::new(Arc::clone(hub));
    let id = &loaded.recipe.id;

    let outcome = match execute(&loaded.recipe, profile, &mut browser, &mut prompter) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("odr-server: {id} failed: {e}");
            return;
        }
    };

    // Manual brokers can't be automated even in principle, so surface their
    // instructions as a single human task the user acknowledges on completion.
    let outcome = match outcome {
        Outcome::ManualSteps { steps } => {
            let prompt = format!("Manual broker — do these yourself: {}", steps.join(" · "));
            let rx = hub.enqueue(id, &prompt, "Manual");
            match rx.recv() {
                Ok(odr_engine::HumanResponse::Completed) => {
                    Outcome::FormSubmitted { confirmation: None }
                }
                _ => Outcome::SkippedByUser,
            }
        }
        other => other,
    };

    if matches!(outcome, Outcome::SkippedByUser) {
        return;
    }

    let confirm_hours = outcome.confirmation().and_then(|c| c.expires_hours);
    let mut store = store.lock().expect("store lock");
    store.mark_requested(
        id,
        loaded.recipe.recheck_days,
        confirm_hours,
        chrono::Utc::now(),
    );
    if let Err(e) = store.save() {
        eprintln!("odr-server: could not save state for {id}: {e}");
    }
}
