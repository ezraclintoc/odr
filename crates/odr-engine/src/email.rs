//! Generating legal deletion-request emails.
//!
//! ODR owns the legal prose (not the recipes) so every request stays
//! consistent and can be updated in one place as law evolves. The engine builds
//! the message; it does **not** send it. The user sends from their own address
//! — that keeps the request first-party (brokers slow-queue "authorized agent"
//! traffic) and keeps ODR out of the business of holding mail credentials.

use odr_recipes::{EmailFlow, EmailTemplate};

use crate::profile::Profile;
use crate::template::{self, RenderError};

/// A fully-rendered email ready for the user to send.
#[derive(Debug, Clone)]
pub struct GeneratedEmail {
    pub to: String,
    pub subject: String,
    pub body: String,
}

impl GeneratedEmail {
    /// A `mailto:` URL that opens the user's mail client pre-filled.
    pub fn mailto(&self) -> String {
        format!(
            "mailto:{}?subject={}&body={}",
            self.to,
            urlencode(&self.subject),
            urlencode(&self.body),
        )
    }
}

/// Build the deletion email described by `flow`, filled from `profile`.
pub fn generate(flow: &EmailFlow, profile: &Profile) -> Result<GeneratedEmail, RenderError> {
    let subject = template::render(&flow.subject, profile)?;
    let mut body = body_for(flow.template, profile)?;
    if let Some(extra) = &flow.extra_body {
        body.push_str("\n\n");
        body.push_str(&template::render(extra, profile)?);
    }
    Ok(GeneratedEmail {
        to: flow.to.clone(),
        subject,
        body,
    })
}

fn body_for(template_kind: EmailTemplate, profile: &Profile) -> Result<String, RenderError> {
    let name = profile.full_name();
    let email = &profile.email;
    let addr = profile
        .current_address()
        .map(|a| format!("{}, {}, {} {}", a.street, a.city, a.state, a.zip))
        .unwrap_or_else(|| "[address on request]".to_string());

    let body = match template_kind {
        EmailTemplate::CcpaDeletion => format!(
            "To whom it may concern,\n\n\
             Under the California Consumer Privacy Act (Cal. Civ. Code § 1798.100 et seq.), \
             I request that you delete all personal information you hold about me and stop \
             selling or sharing it. This is a request from the consumer directly, not an \
             authorized agent.\n\n\
             Name: {name}\n\
             Email: {email}\n\
             Address: {addr}\n\n\
             Please confirm completion within 45 days as required by law, and identify any \
             sources from which you obtained my information.\n\n\
             Regards,\n{name}",
        ),
        EmailTemplate::GdprErasure => format!(
            "To whom it may concern,\n\n\
             Under Article 17 of the General Data Protection Regulation (Right to Erasure), \
             I request that you erase all personal data you hold concerning me, and under \
             Article 21 that you cease any processing for direct marketing.\n\n\
             Name: {name}\n\
             Email: {email}\n\
             Address: {addr}\n\n\
             Please confirm completion within one month as required by Article 12(3).\n\n\
             Regards,\n{name}",
        ),
        EmailTemplate::UsStatePrivacy => {
            let (state, law) = state_privacy_law(profile);
            format!(
                "To whom it may concern,\n\n\
                 As a resident of {state}, I invoke my rights under {law} and request that \
                 you delete all personal information you hold about me and stop selling or \
                 sharing it.\n\n\
                 Name: {name}\n\
                 Email: {email}\n\
                 Address: {addr}\n\n\
                 Please confirm completion within the statutory deadline.\n\n\
                 Regards,\n{name}",
            )
        }
    };
    Ok(body)
}

/// Map the profile's current state to its consumer-privacy statute. Falls back
/// to a generic phrasing for states without a comprehensive law.
fn state_privacy_law(profile: &Profile) -> (String, String) {
    let state = profile
        .current_address()
        .map(|a| a.state.to_uppercase())
        .unwrap_or_default();
    let law = match state.as_str() {
        "CA" => "the California Consumer Privacy Act",
        "VA" => "the Virginia Consumer Data Protection Act",
        "CO" => "the Colorado Privacy Act",
        "CT" => "the Connecticut Data Privacy Act",
        "TX" => "the Texas Data Privacy and Security Act",
        "OR" => "the Oregon Consumer Privacy Act",
        _ => "applicable state consumer-privacy law",
    };
    let state_name = if state.is_empty() {
        "my state".to_string()
    } else {
        state
    };
    (state_name, law.to_string())
}

/// Minimal percent-encoding for `mailto:` query components.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
