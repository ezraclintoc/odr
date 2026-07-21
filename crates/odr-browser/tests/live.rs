//! Live smoke test for the CDP browser driver.
//!
//! Ignored by default because it needs a real Chrome/Chromium. Run it with a
//! browser available (the Nix dev shell provides one via `$CHROME`):
//!
//! ```sh
//! cargo test -p odr-browser -- --ignored
//! ```

use odr_browser::LocalBrowser;
use odr_engine::BrowserDriver;

/// A minimal self-contained page: type into an input, click a button, and the
/// button's handler copies the input's value into the document title so we can
/// read it back and prove the driver actually drove the page.
const PAGE: &str = "data:text/html,\
<input id='name'>\
<button id='go' onclick=\"document.title=document.getElementById('name').value\">go</button>";

#[test]
#[ignore = "requires a local Chrome/Chromium"]
fn drives_a_real_page() {
    let mut browser = LocalBrowser::launch_headless().expect("launch headless chromium");

    browser.navigate(PAGE).expect("navigate");
    browser.fill("#name", "ada-was-here").expect("fill");
    browser.click("#go").expect("click");

    let title = browser.eval_string("document.title").expect("read title");
    assert_eq!(
        title, "ada-was-here",
        "the driver should have filled + clicked"
    );
}
