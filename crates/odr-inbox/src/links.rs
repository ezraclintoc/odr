//! Finding a broker's confirmation link inside an email body.
//!
//! Kept free of IMAP and networking so it can be tested exhaustively — this is
//! the part that decides which URL to *open*, and opening the wrong one (an
//! unsubscribe-from-everything link, a tracking pixel, or a link to an
//! attacker's domain in a spoofed mail) would be the damaging failure.

/// Words that mark a link as the confirmation action rather than boilerplate.
const CONFIRM_HINTS: [&str; 8] = [
    "confirm",
    "verify",
    "validation",
    "optout",
    "opt-out",
    "opt_out",
    "removal",
    "remove",
];

/// Links we must never click, even on the right domain.
const AVOID_HINTS: [&str; 6] = [
    "unsubscribe",
    "privacy-policy",
    "privacypolicy",
    "terms",
    "/help",
    "support",
];

/// Pull every http(s) URL out of a blob of text.
///
/// Deliberately simple: scan for a scheme and take characters until something
/// that can't be in a URL. Handles both plain-text and HTML bodies (where the
/// URL sits inside `href="..."`).
pub fn extract_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let rest = &text[i..];
        let Some(start) = rest.find("http") else {
            break;
        };
        let abs = i + start;
        let tail = &text[abs..];
        if !(tail.starts_with("http://") || tail.starts_with("https://")) {
            i = abs + 4;
            continue;
        }
        let end = tail
            .find(|c: char| {
                c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')' | ']' | '\\')
            })
            .unwrap_or(tail.len());
        let mut url = &tail[..end];
        // Trailing punctuation from prose ("visit https://x/confirm.").
        url = url.trim_end_matches(['.', ',', ';', ':', '!', '?']);
        if url.len() > "https://".len() {
            urls.push(decode_entities(url));
        }
        i = abs + end.max(1);
    }
    urls
}

/// HTML-encoded ampersands are common in `href` attributes and would otherwise
/// corrupt query strings.
fn decode_entities(url: &str) -> String {
    url.replace("&amp;", "&").replace("&#38;", "&")
}

/// Does this URL's host belong to `domain` (or a subdomain of it)?
pub fn host_matches(url: &str, domain: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .split('@')
        .next_back()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let domain = domain.trim_start_matches("www.").to_ascii_lowercase();
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// Pick the confirmation link for `domain` out of an email body.
///
/// Only links on the broker's own domain are ever considered, so a spoofed or
/// quoted third-party URL can't redirect us. Among those, links that look like
/// a confirmation action win; obvious non-actions (unsubscribe, policy pages)
/// are rejected outright.
pub fn find_confirmation_link(body: &str, domain: &str) -> Option<String> {
    let candidates: Vec<String> = extract_urls(body)
        .into_iter()
        .filter(|u| host_matches(u, domain))
        .filter(|u| {
            let low = u.to_ascii_lowercase();
            !AVOID_HINTS.iter().any(|bad| low.contains(bad))
        })
        .collect();

    // Prefer an explicit confirmation-looking link.
    if let Some(best) = candidates.iter().find(|u| {
        let low = u.to_ascii_lowercase();
        CONFIRM_HINTS.iter().any(|hint| low.contains(hint))
    }) {
        return Some(best.clone());
    }
    // Otherwise a link carrying an opaque token is the usual shape.
    candidates
        .iter()
        .find(|u| u.contains('?') && u.len() > 40)
        .cloned()
}

/// The registrable domain of a broker homepage, for matching links against.
pub fn domain_of(homepage: &str) -> String {
    homepage
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(homepage)
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_plain_and_html_urls() {
        let body = "Click https://spokeo.com/optout/confirm?t=abc to finish.\n\
                    <a href=\"https://spokeo.com/other?x=1&amp;y=2\">link</a>";
        let urls = extract_urls(body);
        assert!(urls.contains(&"https://spokeo.com/optout/confirm?t=abc".to_string()));
        assert!(
            urls.contains(&"https://spokeo.com/other?x=1&y=2".to_string()),
            "should decode &amp; in hrefs: {urls:?}"
        );
    }

    #[test]
    fn strips_trailing_prose_punctuation() {
        let urls = extract_urls("go to https://x.com/confirm?t=1.");
        assert_eq!(urls, vec!["https://x.com/confirm?t=1"]);
    }

    #[test]
    fn host_matching_handles_subdomains_not_lookalikes() {
        assert!(host_matches("https://spokeo.com/a", "spokeo.com"));
        assert!(host_matches("https://mail.spokeo.com/a", "spokeo.com"));
        assert!(host_matches("https://www.spokeo.com/a", "www.spokeo.com"));
        // The attack this guards against: a lookalike domain.
        assert!(!host_matches("https://spokeo.com.evil.tld/a", "spokeo.com"));
        assert!(!host_matches("https://notspokeo.com/a", "spokeo.com"));
        assert!(!host_matches(
            "https://evil.tld/?u=spokeo.com",
            "spokeo.com"
        ));
    }

    #[test]
    fn picks_the_confirmation_link() {
        let body = "Hi,\n\
             Unsubscribe: https://spokeo.com/unsubscribe?u=9\n\
             Confirm your opt-out: https://spokeo.com/optout/confirm?token=xyz\n\
             Privacy policy: https://spokeo.com/privacy-policy\n";
        assert_eq!(
            find_confirmation_link(body, "spokeo.com").as_deref(),
            Some("https://spokeo.com/optout/confirm?token=xyz")
        );
    }

    #[test]
    fn never_follows_a_foreign_domain() {
        // A spoofed mail carrying an attacker link must be ignored entirely.
        let body = "Confirm here: https://evil.example/confirm?token=pwn";
        assert_eq!(find_confirmation_link(body, "spokeo.com"), None);
    }

    #[test]
    fn refuses_unsubscribe_only_mail() {
        let body = "Unsubscribe: https://spokeo.com/unsubscribe?u=9";
        assert_eq!(find_confirmation_link(body, "spokeo.com"), None);
    }

    #[test]
    fn falls_back_to_a_tokenised_link() {
        let body = "Finish up: https://spokeo.com/x/9f2b?k=8e1d0c7a6b5f4e3d2c1b0a99";
        assert!(find_confirmation_link(body, "spokeo.com").is_some());
    }

    #[test]
    fn derives_domain_from_homepage() {
        assert_eq!(domain_of("https://www.spokeo.com"), "spokeo.com");
        assert_eq!(domain_of("https://nuwber.com/"), "nuwber.com");
    }
}
