//! # odr-browser
//!
//! The live browser driver: a real Chrome/Chromium controlled over the Chrome
//! DevTools Protocol (via [`chromiumoxide`]), implementing the engine's sync
//! [`odr_engine::BrowserDriver`] trait.
//!
//! ODR keeps the browser **visible** and hands CAPTCHAs / ID checks back to the
//! user (that gating lives in the engine, not here). The browser runs on the
//! user's own machine so requests stay first-party.
//!
//! `chromiumoxide` is async and tokio-based; the `BrowserDriver` trait is sync.
//! [`LocalBrowser`] bridges the two by owning a multi-threaded Tokio runtime and
//! `block_on`-ing each operation, so callers (the CLI, the engine) never see
//! async. The CDP event handler runs as a spawned task on that runtime.

use std::time::{Duration, Instant};

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::error::CdpError;
use chromiumoxide::Page;
use futures::StreamExt;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use odr_engine::{BrowserDriver, BrowserError};

/// How long [`BrowserDriver::wait_for`] polls before giving up.
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(300);

/// A real Chrome/Chromium driven over CDP.
///
/// Field order matters for drop: the page, browser, and handler are torn down
/// before the runtime that they ran on.
pub struct LocalBrowser {
    page: Page,
    _browser: Browser,
    _handler: JoinHandle<()>,
    rt: Runtime,
}

impl LocalBrowser {
    /// Launch a fresh **visible** Chrome on this machine.
    pub fn launch() -> Result<Self, BrowserError> {
        Self::from_config(base_config(true)?)
    }

    /// Launch a headless Chrome — for tests and non-interactive checks.
    pub fn launch_headless() -> Result<Self, BrowserError> {
        Self::from_config(base_config(false)?)
    }

    /// Attach to an already-running Chrome exposing a DevTools endpoint, e.g.
    /// one the user started with `--remote-debugging-port=9222`. ODR never
    /// closes a browser it attached to.
    pub fn connect(endpoint: &str) -> Result<Self, BrowserError> {
        let rt = runtime()?;
        let endpoint = endpoint.to_string();
        let (browser, page, handler) = rt
            .block_on(async move {
                let (browser, mut handler) = Browser::connect(endpoint).await?;
                // Keep draining CDP events for the browser's lifetime. We do NOT
                // break on individual errors — some are benign (e.g. events during
                // a data: navigation) and breaking would kill the handler and
                // cancel every later request ("oneshot canceled").
                let task = tokio::spawn(async move { while handler.next().await.is_some() {} });
                let page = browser.new_page("about:blank").await?;
                Ok::<_, CdpError>((browser, page, task))
            })
            .map_err(cdp)?;
        Ok(Self {
            page,
            _browser: browser,
            _handler: handler,
            rt,
        })
    }

    /// Evaluate a JavaScript expression and return its string result. Useful for
    /// detecting confirmation states on a page after a submission.
    pub fn eval_string(&mut self, script: &str) -> Result<String, BrowserError> {
        let page = self.page.clone();
        let script = script.to_string();
        self.rt.block_on(async move {
            let result = page.evaluate(script).await.map_err(cdp)?;
            result
                .into_value::<String>()
                .map_err(|e| BrowserError::Driver(e.to_string()))
        })
    }

    fn from_config(config: BrowserConfig) -> Result<Self, BrowserError> {
        let rt = runtime()?;
        let (browser, page, handler) = rt
            .block_on(async move {
                let (browser, mut handler) = Browser::launch(config).await?;
                // Keep draining CDP events for the browser's lifetime. We do NOT
                // break on individual errors — some are benign (e.g. events during
                // a data: navigation) and breaking would kill the handler and
                // cancel every later request ("oneshot canceled").
                let task = tokio::spawn(async move { while handler.next().await.is_some() {} });
                let page = browser.new_page("about:blank").await?;
                Ok::<_, CdpError>((browser, page, task))
            })
            .map_err(cdp)?;
        Ok(Self {
            page,
            _browser: browser,
            _handler: handler,
            rt,
        })
    }
}

impl BrowserDriver for LocalBrowser {
    fn navigate(&mut self, url: &str) -> Result<(), BrowserError> {
        let page = self.page.clone();
        let url = url.to_string();
        self.rt
            .block_on(async move {
                page.goto(url).await?;
                page.wait_for_navigation().await?;
                Ok::<_, CdpError>(())
            })
            .map_err(cdp)
    }

    fn fill(&mut self, selector: &str, value: &str) -> Result<(), BrowserError> {
        let page = self.page.clone();
        let (selector, value) = (selector.to_string(), value.to_string());
        self.rt
            .block_on(async move {
                let el = page.find_element(&selector).await?;
                el.click().await?.type_str(&value).await?;
                Ok::<_, CdpError>(())
            })
            .map_err(cdp)
    }

    fn select(&mut self, selector: &str, value: &str) -> Result<(), BrowserError> {
        // Set the <select>'s value and fire a change event, the way a user
        // choosing an option would. JSON-encoding keeps the strings injection-safe.
        let page = self.page.clone();
        let js = format!(
            "() => {{ const e = document.querySelector({}); if (!e) throw new Error('no element'); \
             e.value = {}; e.dispatchEvent(new Event('change', {{ bubbles: true }})); }}",
            serde_json::Value::from(selector),
            serde_json::Value::from(value),
        );
        self.rt
            .block_on(async move { page.evaluate(js).await.map(|_| ()) })
            .map_err(cdp)
    }

    fn click(&mut self, selector: &str) -> Result<(), BrowserError> {
        let page = self.page.clone();
        let selector = selector.to_string();
        self.rt
            .block_on(async move {
                page.find_element(&selector).await?.click().await?;
                Ok::<_, CdpError>(())
            })
            .map_err(cdp)
    }

    fn eval(&mut self, script: &str) -> Result<String, BrowserError> {
        self.eval_string(script)
    }

    fn wait_for(&mut self, selector: &str) -> Result<(), BrowserError> {
        let page = self.page.clone();
        let selector = selector.to_string();
        self.rt.block_on(async move {
            let deadline = Instant::now() + WAIT_TIMEOUT;
            loop {
                if page.find_element(&selector).await.is_ok() {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(BrowserError::Timeout(selector));
                }
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        })
    }
}

/// A multi-threaded runtime so the CDP handler task keeps running while a
/// `block_on` operation is in flight.
fn runtime() -> Result<Runtime, BrowserError> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| BrowserError::Driver(format!("tokio runtime: {e}")))
}

/// Base browser config. `visible` toggles a real window vs. headless. Honors a
/// `CHROME` env var pointing at a Chromium binary (e.g. from the Nix dev shell).
fn base_config(visible: bool) -> Result<BrowserConfig, BrowserError> {
    let mut builder = BrowserConfig::builder();
    if visible {
        builder = builder.with_head();
    }
    // Sandbox often can't be used in CI/Nix build sandboxes; harmless locally.
    // `--disable-dev-shm-usage` avoids Chromium crashing where /dev/shm is tiny
    // (containers, CI); `--disable-gpu` avoids a GPU probe in headless envs.
    builder = builder
        .no_sandbox()
        .arg("--disable-dev-shm-usage")
        .arg("--disable-gpu");
    if let Ok(path) = std::env::var("CHROME") {
        builder = builder.chrome_executable(path);
    }
    builder
        .build()
        .map_err(|e| BrowserError::Driver(format!("browser config: {e}")))
}

/// Map a CDP error onto the engine's browser error type. (A `From` impl isn't
/// allowed here — both types are foreign to this crate — so it's a plain fn.)
fn cdp(e: CdpError) -> BrowserError {
    BrowserError::Driver(e.to_string())
}
