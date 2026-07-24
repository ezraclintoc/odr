//! Live CAPTCHA-detection test against a real browser.
//!
//! Proves the `--captcha auto` path against an actual DOM: a page with no
//! challenge is detected as clear, and a page carrying an unsolved reCAPTCHA
//! widget is detected as blocking (so the engine falls back to the human).
//!
//! Ignored by default — needs a real Chrome/Chromium:
//! `cargo test -p odr-browser -- --ignored`

use odr_browser::LocalBrowser;
use odr_engine::captcha::{self, CaptchaKind};
use odr_engine::BrowserDriver;

const NO_CAPTCHA: &str = "data:text/html,<h1>plain opt-out form</h1><input id='email'>";

/// A reCAPTCHA v2 widget as it appears before the user solves it: the div
/// carries the site key and the response textarea is empty.
const UNSOLVED_RECAPTCHA: &str = "data:text/html,\
<div class='g-recaptcha' data-sitekey='6LtestKey'></div>\
<textarea id='g-recaptcha-response'></textarea>";

/// The same widget after a token has been issued.
const SOLVED_RECAPTCHA: &str = "data:text/html,\
<div class='g-recaptcha' data-sitekey='6LtestKey'></div>\
<textarea id='g-recaptcha-response'>03AGdBq26-token</textarea>";

#[test]
#[ignore = "requires a local Chrome/Chromium"]
fn detects_captcha_state_on_real_pages() {
    let mut browser = LocalBrowser::launch_headless().expect("launch headless chromium");

    browser.navigate(NO_CAPTCHA).expect("navigate");
    let state = captcha::detect(&mut browser).expect("detect");
    assert!(!state.present, "a plain form has no challenge");
    assert!(state.is_clear(), "nothing should block submission");

    browser.navigate(UNSOLVED_RECAPTCHA).expect("navigate");
    let state = captcha::detect(&mut browser).expect("detect");
    assert!(state.present, "the widget should be found");
    assert!(!state.solved, "an empty response field means unsolved");
    assert_eq!(state.kind, CaptchaKind::ReCaptcha);
    assert_eq!(state.site_key.as_deref(), Some("6LtestKey"));
    assert!(!state.is_clear(), "an unsolved challenge must block");

    browser.navigate(SOLVED_RECAPTCHA).expect("navigate");
    let state = captcha::detect(&mut browser).expect("detect");
    assert!(state.present && state.solved, "token present means solved");
    assert!(state.is_clear(), "a solved challenge should not block");
}
