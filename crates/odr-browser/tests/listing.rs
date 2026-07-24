//! Live test for automatic listing discovery against a real browser.
//!
//! This is the feature that removes the most human interruptions, so it's worth
//! proving against a real DOM rather than a mock: given a search-results page,
//! ODR must pick the user's own record and refuse to guess when ambiguous.
//!
//! Ignored by default — needs a real Chrome/Chromium:
//! `cargo test -p odr-browser -- --ignored`

use odr_browser::LocalBrowser;
use odr_engine::listing;
use odr_engine::BrowserDriver;

/// A search-results page shaped like a real people-search site: several people,
/// one of whom is ours.
const RESULTS: &str = "data:text/html,\
<div class='card-summary'><a class='detail-link' href='https://x.test/1'>Bob Smith</a> Reno, NV</div>\
<div class='card-summary'><a class='detail-link' href='https://x.test/2'>Ada Lovelace</a> London, CA age 36</div>\
<div class='card-summary'><a class='detail-link' href='https://x.test/3'>Ada Lovelace</a> Austin, TX age 61</div>";

#[test]
#[ignore = "requires a local Chrome/Chromium"]
fn finds_the_users_own_listing() {
    let mut browser = LocalBrowser::launch_headless().expect("launch headless chromium");
    browser.navigate(RESULTS).expect("navigate");

    let found = listing::extract(&mut browser, ".card-summary", "a.detail-link").expect("extract");
    assert_eq!(found.len(), 3, "should scrape every result row");

    // Name alone is ambiguous here — two Ada Lovelaces — so ODR must refuse.
    let name_only = vec!["Ada Lovelace".to_string()];
    assert_eq!(
        listing::pick(&found, &name_only),
        None,
        "two people share the name; ODR must not guess"
    );

    // Name + city identifies exactly one record.
    let name_and_city = vec!["Ada Lovelace".to_string(), "London".to_string()];
    assert_eq!(
        listing::pick(&found, &name_and_city).as_deref(),
        Some("https://x.test/2"),
        "should select the matching record's link"
    );

    // Somebody who isn't listed yields nothing.
    let absent = vec!["Grace Hopper".to_string()];
    assert_eq!(listing::pick(&found, &absent), None);
}
