//! The browser automation seam.
//!
//! ODR drives a *visible* browser so a human can clear CAPTCHAs and ID checks
//! themselves — we never try to defeat those. The concrete driver lives behind
//! this trait so the recipe interpreter has no idea which browser it's talking
//! to, and so tests can run against a fake.
//!
//! The browser is only the *view/drive* channel. Signaling that a human
//! finished a manual step is a separate concern — see
//! [`crate::interaction`]. Keeping them apart is what lets ODR run headless on
//! a server with the browser exposed to the web (VNC/CDP screencast) while
//! prompts are delivered to the user over a different channel entirely.
//!
//! Planned drivers:
//! - `LocalBrowser` — `chromiumoxide` over CDP, attaching to the user's Chrome.
//! - `RemoteBrowser` — attaches to a headful Chrome inside a container whose
//!   view is streamed to the web, for the Docker/server deployment.

/// Errors a browser driver can surface while executing a step.
#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("no element matched selector `{0}`")]
    ElementNotFound(String),
    #[error("timed out waiting for `{0}`")]
    Timeout(String),
    #[error("browser driver error: {0}")]
    Driver(String),
}

/// Something that can carry out browser actions on the user's behalf.
///
/// Implementations keep the page viewable by a human (locally or remotely) but
/// need no notion of *how* a human is prompted — that is the job of
/// [`crate::interaction::HumanInterface`].
pub trait BrowserDriver {
    fn navigate(&mut self, url: &str) -> Result<(), BrowserError>;
    fn fill(&mut self, selector: &str, value: &str) -> Result<(), BrowserError>;
    fn select(&mut self, selector: &str, value: &str) -> Result<(), BrowserError>;
    fn click(&mut self, selector: &str) -> Result<(), BrowserError>;
    fn wait_for(&mut self, selector: &str) -> Result<(), BrowserError>;

    /// Evaluate a JavaScript expression and return its string result.
    ///
    /// Used to *inspect* page state the engine can't otherwise see — e.g.
    /// whether a CAPTCHA is actually present and unsolved (see
    /// [`crate::captcha`]). Drivers that can't run scripts return an empty
    /// string, which callers must treat as "unknown", never as "absent".
    fn eval(&mut self, script: &str) -> Result<String, BrowserError>;
}

/// A driver that performs no real automation but records what it was asked to
/// do. Lets the engine and recipes be exercised end-to-end in tests and in a
/// `--dry-run` mode without opening a browser.
#[derive(Debug, Default)]
pub struct DryRunBrowser {
    pub log: Vec<String>,
}

impl BrowserDriver for DryRunBrowser {
    fn navigate(&mut self, url: &str) -> Result<(), BrowserError> {
        self.log.push(format!("navigate {url}"));
        Ok(())
    }
    fn fill(&mut self, selector: &str, value: &str) -> Result<(), BrowserError> {
        self.log.push(format!("fill {selector} = {value:?}"));
        Ok(())
    }
    fn select(&mut self, selector: &str, value: &str) -> Result<(), BrowserError> {
        self.log.push(format!("select {selector} = {value:?}"));
        Ok(())
    }
    fn click(&mut self, selector: &str) -> Result<(), BrowserError> {
        self.log.push(format!("click {selector}"));
        Ok(())
    }
    fn wait_for(&mut self, selector: &str) -> Result<(), BrowserError> {
        self.log.push(format!("wait_for {selector}"));
        Ok(())
    }
    fn eval(&mut self, script: &str) -> Result<String, BrowserError> {
        // No page to inspect. Empty means "unknown", so CAPTCHA auto-detection
        // conservatively falls back to asking the human rather than skipping.
        self.log.push(format!("eval {script}"));
        Ok(String::new())
    }
}
