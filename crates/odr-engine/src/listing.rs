//! Finding the user's own record on a broker's search results.
//!
//! "Search for yourself, find your record, paste its URL" is the single most
//! common reason ODR has to interrupt someone — and it's mechanical: the
//! broker's search is a URL pattern, and the user's record is the one matching
//! their name and city. The engine can do it.
//!
//! The one thing it must never do is guess. Opting out the wrong person's
//! listing is a real harm (it removes a stranger's record, and leaves the
//! user's in place), so [`pick`] only returns a match when exactly **one**
//! candidate fits. Zero or several means the human decides — which is
//! precisely when a human should.

use serde::Deserialize;

use crate::browser::{BrowserDriver, BrowserError};

/// One search result: its visible text and the link to the full record.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Candidate {
    pub text: String,
    pub href: String,
}

/// Build the extraction script for the given selectors.
fn extract_js(result_selector: &str, link_selector: &str) -> String {
    format!(
        r#"(() => {{
  const rows = Array.from(document.querySelectorAll({result}));
  const out = rows.map((r) => {{
    let a = r.querySelector({link});
    if (!a && r.tagName === 'A') a = r;
    const text = (r.innerText || r.textContent || '').replace(/\s+/g, ' ').trim();
    return {{ text: text, href: a ? a.href : '' }};
  }}).filter((x) => x.href);
  return JSON.stringify(out.slice(0, 50));
}})()"#,
        result = serde_json::Value::from(result_selector),
        link = serde_json::Value::from(link_selector),
    )
}

/// Scrape the current page's search results.
pub fn extract(
    browser: &mut dyn BrowserDriver,
    result_selector: &str,
    link_selector: &str,
) -> Result<Vec<Candidate>, BrowserError> {
    let raw = browser.eval(&extract_js(result_selector, link_selector))?;
    if raw.trim().is_empty() {
        // A driver that can't read the page yields no candidates, which sends
        // the caller down the ask-the-human path rather than guessing.
        return Ok(Vec::new());
    }
    serde_json::from_str(&raw)
        .map_err(|e| BrowserError::Driver(format!("could not read search results: {e}")))
}

/// Choose the user's record, but only when it is unambiguous.
///
/// A candidate matches when every needle appears in its text (case-insensitive).
/// Returns `Some(href)` only if exactly one candidate matches.
pub fn pick(candidates: &[Candidate], needles: &[String]) -> Option<String> {
    if needles.is_empty() {
        return None;
    }
    let lowered: Vec<String> = needles
        .iter()
        .map(|n| n.trim().to_lowercase())
        .filter(|n| !n.is_empty())
        .collect();
    if lowered.is_empty() {
        return None;
    }

    let mut matches = candidates.iter().filter(|c| {
        let hay = c.text.to_lowercase();
        lowered.iter().all(|n| hay.contains(n.as_str()))
    });

    let first = matches.next()?;
    // More than one match is ambiguous — refuse to choose.
    if matches.next().is_some() {
        return None;
    }
    Some(first.href.clone())
}

/// The URL of the page currently open, used to capture a listing the human
/// navigated to themselves.
pub fn current_url(browser: &mut dyn BrowserDriver) -> Result<String, BrowserError> {
    browser.eval("location.href")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(text: &str, href: &str) -> Candidate {
        Candidate {
            text: text.into(),
            href: href.into(),
        }
    }

    #[test]
    fn picks_the_single_match() {
        let rows = vec![
            c("Bob Smith, Reno NV", "https://x/1"),
            c("Ada Lovelace, London CA, age 36", "https://x/2"),
            c("Carol Jones, Austin TX", "https://x/3"),
        ];
        let needles = vec!["Ada Lovelace".into(), "London".into()];
        assert_eq!(pick(&rows, &needles).as_deref(), Some("https://x/2"));
    }

    #[test]
    fn refuses_when_ambiguous() {
        // Two people share the name and city — a human must disambiguate rather
        // than have ODR opt out a stranger.
        let rows = vec![
            c("Ada Lovelace, London CA, age 36", "https://x/1"),
            c("Ada Lovelace, London CA, age 61", "https://x/2"),
        ];
        let needles = vec!["Ada Lovelace".into(), "London".into()];
        assert_eq!(pick(&rows, &needles), None);
    }

    #[test]
    fn refuses_when_nothing_matches() {
        let rows = vec![c("Bob Smith, Reno NV", "https://x/1")];
        let needles = vec!["Ada Lovelace".into()];
        assert_eq!(pick(&rows, &needles), None);
    }

    #[test]
    fn matching_is_case_insensitive() {
        let rows = vec![c("ADA LOVELACE — LONDON, CA", "https://x/1")];
        let needles = vec!["Ada Lovelace".into(), "london".into()];
        assert_eq!(pick(&rows, &needles).as_deref(), Some("https://x/1"));
    }

    #[test]
    fn no_needles_is_never_a_match() {
        let rows = vec![c("anyone at all", "https://x/1")];
        assert_eq!(pick(&rows, &[]), None);
    }
}
