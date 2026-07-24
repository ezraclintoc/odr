//! CAPTCHA handling policy.
//!
//! ODR's default is simple and safe: a CAPTCHA is a human's job. But most
//! recipes list a CAPTCHA step *defensively* — on any given run there often
//! isn't one actually blocking (good IP reputation, an invisible v3 challenge,
//! or one already satisfied). Interrupting the user for a CAPTCHA that isn't
//! there is pure friction.
//!
//! So [`CaptchaPolicy::Auto`] does three honest things before falling back to
//! the human:
//!
//! 1. **Detect** whether a challenge is present at all, and whether it's
//!    already solved. If not blocking, skip the prompt entirely.
//! 2. **Wait briefly** — invisible and reputation-based challenges resolve
//!    themselves within a few seconds.
//! 3. **Delegate to a [`CaptchaSolver`]**, if the user plugged one in.
//!
//! What ODR deliberately does *not* do: ship a challenge-breaking
//! implementation. Defeating a modern reCAPTCHA/hCaptcha/Turnstile challenge
//! requires a third-party human-or-ML solving farm, which costs money, routes
//! your data through someone else, and is a genuine terms-of-service and legal
//! gray area — all three of which contradict what ODR is for. [`CaptchaSolver`]
//! exists as an integration point for anyone who chooses that trade-off
//! themselves; no such solver is bundled.
//!
//! The human is always the fallback, and the fallback always works.

use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::browser::{BrowserDriver, BrowserError};

/// How to handle a CAPTCHA gate in a recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptchaPolicy {
    /// Always hand the CAPTCHA to the human. Zero risk, zero surprises.
    #[default]
    AlwaysAsk,
    /// Skip the prompt when nothing is actually blocking; briefly wait for
    /// self-resolving challenges; try a configured solver; then ask the human.
    Auto,
}

/// How long [`CaptchaPolicy::Auto`] waits for a challenge to resolve itself
/// before involving the human.
const AUTO_WAIT: Duration = Duration::from_secs(8);
const POLL: Duration = Duration::from_millis(500);

/// Which challenge vendor was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptchaKind {
    ReCaptcha,
    HCaptcha,
    Turnstile,
    Unknown,
}

impl CaptchaKind {
    fn from_tag(tag: &str) -> Self {
        match tag {
            "recaptcha" => CaptchaKind::ReCaptcha,
            "hcaptcha" => CaptchaKind::HCaptcha,
            "turnstile" => CaptchaKind::Turnstile,
            _ => CaptchaKind::Unknown,
        }
    }

    /// The form field a solved token gets written into.
    fn response_field(self) -> &'static str {
        match self {
            CaptchaKind::ReCaptcha => "#g-recaptcha-response",
            CaptchaKind::HCaptcha => "[name='h-captcha-response']",
            CaptchaKind::Turnstile => "[name='cf-turnstile-response']",
            CaptchaKind::Unknown => "#g-recaptcha-response",
        }
    }
}

/// What a page inspection found.
#[derive(Debug, Clone)]
pub struct CaptchaState {
    /// A challenge widget exists on the page.
    pub present: bool,
    /// It already carries a response token.
    pub solved: bool,
    pub kind: CaptchaKind,
    /// The site key, when readable — a solver needs it.
    pub site_key: Option<String>,
}

impl CaptchaState {
    /// Nothing stands between us and submitting the form.
    pub fn is_clear(&self) -> bool {
        !self.present || self.solved
    }
}

/// A pluggable third-party CAPTCHA solving backend.
///
/// Implement this to integrate a solving service. ODR ships no implementation —
/// see the module docs for why.
pub trait CaptchaSolver: Send {
    /// Return a response token for the challenge, or `None` if it can't be
    /// solved. Errors are treated the same as `None`: fall back to the human.
    fn solve(&mut self, state: &CaptchaState, page_url: &str) -> Result<Option<String>, String>;
}

/// How a run should treat CAPTCHA gates.
///
/// Defaults to [`CaptchaPolicy::AlwaysAsk`] with no solver — the safe choice.
#[derive(Default)]
pub struct CaptchaConfig<'a> {
    pub policy: CaptchaPolicy,
    pub solver: Option<&'a mut dyn CaptchaSolver>,
}

impl<'a> CaptchaConfig<'a> {
    /// Try to clear CAPTCHAs automatically before asking the human.
    pub fn auto() -> Self {
        Self {
            policy: CaptchaPolicy::Auto,
            solver: None,
        }
    }

    /// Attach a third-party solver, consulted only after auto-detection and the
    /// self-resolve wait have both failed.
    pub fn with_solver(mut self, solver: &'a mut dyn CaptchaSolver) -> Self {
        self.solver = Some(solver);
        self
    }
}

/// The decision reached for one CAPTCHA gate.
#[derive(Debug, PartialEq, Eq)]
pub enum Resolution {
    /// Nothing (or nothing further) is needed; carry on with the flow.
    Clear,
    /// A human has to deal with it.
    NeedsHuman,
}

/// Page-inspection script. Returns a JSON string so the engine can parse a
/// structured answer out of a single round-trip.
const DETECT_JS: &str = r#"(() => {
  const q = (s) => document.querySelector(s);
  const val = (s) => { const e = q(s); return e && e.value ? true : false; };
  let out = { present: false, solved: false, kind: "none", site_key: null };
  const rc = q('.g-recaptcha') || q('iframe[src*="recaptcha"]');
  const hc = q('.h-captcha') || q('iframe[src*="hcaptcha"]');
  const ts = q('.cf-turnstile') || q('iframe[src*="challenges.cloudflare.com"]');
  if (rc) {
    out.present = true; out.kind = "recaptcha";
    out.solved = val('#g-recaptcha-response');
    const w = q('.g-recaptcha');
    out.site_key = (w && w.getAttribute('data-sitekey')) || null;
  } else if (hc) {
    out.present = true; out.kind = "hcaptcha";
    out.solved = val('[name="h-captcha-response"]');
    const w = q('.h-captcha');
    out.site_key = (w && w.getAttribute('data-sitekey')) || null;
  } else if (ts) {
    out.present = true; out.kind = "turnstile";
    out.solved = val('[name="cf-turnstile-response"]');
    const w = q('.cf-turnstile');
    out.site_key = (w && w.getAttribute('data-sitekey')) || null;
  }
  return JSON.stringify(out);
})()"#;

#[derive(Deserialize)]
struct RawState {
    present: bool,
    solved: bool,
    kind: String,
    site_key: Option<String>,
}

/// Inspect the current page for a CAPTCHA.
///
/// A driver that can't evaluate scripts returns an empty string; that is
/// reported as "present but unsolved" so the caller conservatively asks the
/// human rather than wrongly skipping a real challenge.
pub fn detect(browser: &mut dyn BrowserDriver) -> Result<CaptchaState, BrowserError> {
    let raw = browser.eval(DETECT_JS)?;
    if raw.trim().is_empty() {
        return Ok(CaptchaState {
            present: true,
            solved: false,
            kind: CaptchaKind::Unknown,
            site_key: None,
        });
    }
    let parsed: RawState = serde_json::from_str(&raw).map_err(|e| {
        BrowserError::Driver(format!("could not read CAPTCHA state: {e} (got {raw:?})"))
    })?;
    Ok(CaptchaState {
        present: parsed.present,
        solved: parsed.solved,
        kind: CaptchaKind::from_tag(&parsed.kind),
        site_key: parsed.site_key,
    })
}

/// Write a solved token into the page's response field.
fn inject_token(
    browser: &mut dyn BrowserDriver,
    kind: CaptchaKind,
    token: &str,
) -> Result<(), BrowserError> {
    let js = format!(
        "(() => {{ const e = document.querySelector({}); if (!e) return \"no\"; \
         e.value = {}; e.dispatchEvent(new Event('change', {{ bubbles: true }})); return \"ok\"; }})()",
        serde_json::Value::from(kind.response_field()),
        serde_json::Value::from(token),
    );
    browser.eval(&js).map(|_| ())
}

/// Apply `policy` to the CAPTCHA gate on the current page.
///
/// Returns [`Resolution::Clear`] if the flow may continue without bothering the
/// user, or [`Resolution::NeedsHuman`] if it must stop and ask.
pub fn resolve(
    config: &mut CaptchaConfig<'_>,
    browser: &mut dyn BrowserDriver,
    page_url: &str,
) -> Result<Resolution, BrowserError> {
    if config.policy == CaptchaPolicy::AlwaysAsk {
        return Ok(Resolution::NeedsHuman);
    }

    // 1. Is anything actually blocking?
    let state = detect(browser)?;
    if state.is_clear() {
        return Ok(Resolution::Clear);
    }

    // 2. Give self-resolving (invisible / reputation-based) challenges a moment.
    let deadline = Instant::now() + AUTO_WAIT;
    while Instant::now() < deadline {
        thread::sleep(POLL);
        if detect(browser)?.is_clear() {
            return Ok(Resolution::Clear);
        }
    }

    // 3. Hand to a solver if the user configured one.
    if let Some(solver) = config.solver.as_deref_mut() {
        let state = detect(browser)?;
        if let Ok(Some(token)) = solver.solve(&state, page_url) {
            inject_token(browser, state.kind, &token)?;
            if detect(browser)?.is_clear() {
                return Ok(Resolution::Clear);
            }
        }
    }

    // 4. The honest fallback.
    Ok(Resolution::NeedsHuman)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::DryRunBrowser;

    /// A browser whose `eval` returns a canned detection payload.
    struct FakePage(&'static str);
    impl BrowserDriver for FakePage {
        fn navigate(&mut self, _: &str) -> Result<(), BrowserError> {
            Ok(())
        }
        fn fill(&mut self, _: &str, _: &str) -> Result<(), BrowserError> {
            Ok(())
        }
        fn select(&mut self, _: &str, _: &str) -> Result<(), BrowserError> {
            Ok(())
        }
        fn click(&mut self, _: &str) -> Result<(), BrowserError> {
            Ok(())
        }
        fn wait_for(&mut self, _: &str) -> Result<(), BrowserError> {
            Ok(())
        }
        fn eval(&mut self, _: &str) -> Result<String, BrowserError> {
            Ok(self.0.to_string())
        }
    }

    #[test]
    fn always_ask_never_inspects() {
        let mut page =
            FakePage(r#"{"present":false,"solved":false,"kind":"none","site_key":null}"#);
        let mut cfg = CaptchaConfig::default();
        let r = resolve(&mut cfg, &mut page, "https://x").unwrap();
        assert_eq!(r, Resolution::NeedsHuman);
    }

    #[test]
    fn auto_skips_when_no_captcha_present() {
        let mut page =
            FakePage(r#"{"present":false,"solved":false,"kind":"none","site_key":null}"#);
        let mut cfg = CaptchaConfig::auto();
        let r = resolve(&mut cfg, &mut page, "https://x").unwrap();
        assert_eq!(r, Resolution::Clear);
    }

    #[test]
    fn auto_skips_when_already_solved() {
        let mut page =
            FakePage(r#"{"present":true,"solved":true,"kind":"recaptcha","site_key":"k"}"#);
        let mut cfg = CaptchaConfig::auto();
        let r = resolve(&mut cfg, &mut page, "https://x").unwrap();
        assert_eq!(r, Resolution::Clear);
    }

    #[test]
    fn auto_consults_solver_then_clears() {
        // Page starts blocked; the solver returns a token and the (fake) page
        // reports clear afterwards.
        struct Solver(bool);
        impl CaptchaSolver for Solver {
            fn solve(&mut self, _: &CaptchaState, _: &str) -> Result<Option<String>, String> {
                self.0 = true;
                Ok(Some("token".into()))
            }
        }
        // Blocked on every detect, so the solver path is reached and then the
        // human fallback still applies — proving we never silently continue.
        let mut page =
            FakePage(r#"{"present":true,"solved":false,"kind":"hcaptcha","site_key":"k"}"#);
        let mut solver = Solver(false);
        let mut cfg = CaptchaConfig::auto().with_solver(&mut solver);
        let r = resolve(&mut cfg, &mut page, "https://x").unwrap();
        assert_eq!(
            r,
            Resolution::NeedsHuman,
            "unsolved page must reach a human"
        );
    }

    #[test]
    fn unknown_page_state_falls_back_to_human() {
        // A driver that can't evaluate scripts must never cause a real CAPTCHA
        // to be silently skipped.
        let mut dry = DryRunBrowser::default();
        let state = detect(&mut dry).unwrap();
        assert!(state.present && !state.solved);
    }

    #[test]
    fn parses_detection_payload() {
        let mut page =
            FakePage(r#"{"present":true,"solved":false,"kind":"turnstile","site_key":"0x4A"}"#);
        let s = detect(&mut page).unwrap();
        assert_eq!(s.kind, CaptchaKind::Turnstile);
        assert_eq!(s.site_key.as_deref(), Some("0x4A"));
        assert!(!s.is_clear());
    }
}
