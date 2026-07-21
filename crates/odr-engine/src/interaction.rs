//! The human-interaction control channel.
//!
//! Some broker steps can't (and shouldn't) be automated — solving a CAPTCHA,
//! uploading an ID, picking your own listing from search results. ODR hands
//! those to a human. *How* the human is reached is deliberately abstracted here
//! so the same engine works in very different deployments:
//!
//! - **Local CLI** — prompt on the terminal, wait for the user to press Enter
//!   ([`ConsolePrompter`]).
//! - **Headless server / Docker** (future) — ODR runs on a server, the browser
//!   runs headful inside the container and is exposed to the web (e.g. VNC via
//!   noVNC, or a CDP screencast). The engine posts each human task to a web
//!   queue and blocks until the user, on some *other* machine, opens a URL,
//!   completes the step in the shared browser view, and clicks "Done". A
//!   `WebPrompter` implementing this trait is all that's needed — no change to
//!   the executor or recipes.
//! - **Tests / dry-run** — auto-approve without a human ([`AutoApprove`]).
//!
//! Two concerns are separated on purpose:
//!
//! 1. *Viewing/driving the broker page* is a property of the concrete
//!    [`crate::browser::BrowserDriver`] (a local window, or a remote-viewable
//!    browser inside a container).
//! 2. *Signaling "I finished this step"* is this control channel.
//!
//! A server deployment swaps both; a local deployment uses local versions of
//! each. Neither the executor nor any recipe knows which.

use std::io::{self, Write};

/// A task that needs a human to act before the flow can continue.
#[derive(Debug, Clone)]
pub struct HumanTask {
    /// Broker this task belongs to (used to label it in a web queue).
    pub broker_id: String,
    /// What the user should do, in plain language.
    pub prompt: String,
    /// The kind of gate, so a UI can present it appropriately.
    pub kind: HumanTaskKind,
}

/// The category of a human step, for presentation and metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanTaskKind {
    /// Solve a CAPTCHA in the browser.
    Captcha,
    /// Upload or present a government ID.
    Verification,
    /// Find and select the user's own listing (e.g. paste its URL).
    FindListing,
    /// Anything else that needs a human.
    Generic,
}

/// What the human did with a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanResponse {
    /// The user completed the step; continue the flow.
    Completed,
    /// The user chose to skip this broker for now.
    Skipped,
}

/// Errors surfacing a human task.
#[derive(Debug, thiserror::Error)]
pub enum InteractionError {
    #[error("interaction channel closed before the user responded")]
    Closed,
    #[error("timed out waiting for the user to complete `{0}`")]
    Timeout(String),
    #[error("interaction error: {0}")]
    Other(String),
}

/// A channel for reaching whoever is completing manual steps.
///
/// Implementations block in [`Self::request`] until the human responds. The
/// engine treats them uniformly, so a local terminal and a remote web queue are
/// interchangeable.
pub trait HumanInterface: Send {
    /// Present `task` and block until the human completes or skips it.
    fn request(&mut self, task: &HumanTask) -> Result<HumanResponse, InteractionError>;
}

/// Prompts on the local terminal and waits for the user to press Enter. Used by
/// the CLI in interactive mode.
#[derive(Debug, Default)]
pub struct ConsolePrompter;

impl HumanInterface for ConsolePrompter {
    fn request(&mut self, task: &HumanTask) -> Result<HumanResponse, InteractionError> {
        println!("\n⏸  [{}] {}", task.broker_id, task.prompt);
        print!("   Press Enter when done (or type 'skip'): ");
        io::stdout()
            .flush()
            .map_err(|e| InteractionError::Other(e.to_string()))?;
        let mut line = String::new();
        io::stdin()
            .read_line(&mut line)
            .map_err(|_| InteractionError::Closed)?;
        if line.trim().eq_ignore_ascii_case("skip") {
            Ok(HumanResponse::Skipped)
        } else {
            Ok(HumanResponse::Completed)
        }
    }
}

/// Auto-completes every task without a human. For `--dry-run` and tests.
#[derive(Debug, Default)]
pub struct AutoApprove {
    /// Record of tasks that were auto-approved, for assertions/inspection.
    pub seen: Vec<String>,
}

impl HumanInterface for AutoApprove {
    fn request(&mut self, task: &HumanTask) -> Result<HumanResponse, InteractionError> {
        self.seen.push(task.prompt.clone());
        Ok(HumanResponse::Completed)
    }
}
