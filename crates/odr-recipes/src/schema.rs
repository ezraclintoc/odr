//! The ODR recipe schema.
//!
//! A *recipe* is a declarative description of how to opt out of one data
//! broker. Recipes are plain YAML so the community can maintain them the way
//! filter lists are maintained — brokers change their forms constantly, and
//! keeping broker-specific logic out of compiled code is what lets a fix be a
//! one-line data PR instead of an engine release.
//!
//! The engine is a small, stable interpreter over these types; it should never
//! grow per-broker special cases. If a broker needs something the schema can't
//! express, extend the schema here (and the interpreter), not the recipe.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A single broker's opt-out recipe.
///
/// Note: no `deny_unknown_fields` here — it is incompatible with the
/// `#[serde(flatten)]` on `flow`. The flattened `Flow` variants carry their own
/// `deny_unknown_fields`, so typos in method-specific fields are still caught.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Recipe {
    /// Stable, unique, lowercase-kebab identifier (e.g. `spokeo`). Used as the
    /// key in the local state store, so it must never change once published.
    pub id: String,

    /// Human-readable broker name (e.g. `Spokeo`).
    pub name: String,

    /// The broker's public homepage.
    pub homepage: String,

    /// Priority tier, borrowed from the "Big Ass Data Broker Opt-Out List"
    /// philosophy: a handful of high-impact brokers matter far more than a long
    /// tail. Curated-and-working beats big-and-stale.
    #[serde(default)]
    pub tier: Tier,

    /// How often (in days) to re-check this broker for reappearance. Brokers
    /// repopulate every 30–90 days, so removal only works as a recurring cycle.
    #[serde(default = "default_recheck_days")]
    pub recheck_days: u32,

    /// Free-text maintainer notes: quirks, gotchas, last-verified date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    /// The opt-out method and its method-specific configuration. Flattened so a
    /// recipe reads `method: web_form` at the top level alongside its steps.
    #[serde(flatten)]
    pub flow: Flow,
}

/// Priority tiers for triaging which brokers to hit first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    /// The high-impact people-search brokers most worth removing first.
    #[default]
    Essential,
    /// Worthwhile but lower-priority brokers.
    Additional,
    /// Requires uploading a government ID — automate cautiously, warn loudly.
    IdRequired,
    /// Requires a phone call, fax, or postal mail — guide only, never automate.
    OfflineOnly,
}

/// The opt-out method. Internally tagged by `method` so each recipe declares
/// exactly one, carrying only the fields that method needs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum Flow {
    /// Drive a browser through the broker's opt-out form. ODR uses the user's
    /// *own*, visible browser so they can clear CAPTCHAs and ID checks in the
    /// loop — we never try to defeat those.
    WebForm(WebFormFlow),

    /// Send a templated legal deletion request by email (CCPA/GDPR/state law).
    Email(EmailFlow),

    /// A broker that can't be automated: print step-by-step instructions for
    /// the user to follow themselves.
    Manual(ManualFlow),
}

/// A browser-driven opt-out form flow.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WebFormFlow {
    /// The page where the opt-out flow begins.
    pub opt_out_url: String,

    /// Ordered steps the interpreter executes against the page.
    pub steps: Vec<Step>,

    /// What happens after submission (e.g. an email link that must be clicked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<Confirmation>,
}

/// One instruction in a web-form flow. Tagged by `action`.
///
/// Any `String` field may contain `{{placeholders}}` (see [`crate::Placeholder`])
/// that the engine fills from the user's local profile before executing.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Step {
    /// Navigate to a URL.
    Navigate { url: String },

    /// Type `value` into the element matched by a CSS `selector`.
    Fill { selector: String, value: String },

    /// Choose `value` in a `<select>` matched by `selector`.
    Select { selector: String, value: String },

    /// Click the element matched by `selector`.
    Click { selector: String },

    /// Block until an element matching `selector` appears.
    WaitFor { selector: String },

    /// Hand control to the user for something we won't automate — a CAPTCHA, an
    /// ID upload, a "check the box" human step. `prompt` is shown to the user.
    HumanStep { prompt: String },
}

/// A templated email deletion request.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailFlow {
    /// The broker's privacy/opt-out address.
    pub to: String,

    /// Subject line (may contain placeholders).
    pub subject: String,

    /// Which built-in legal body template to use.
    pub template: EmailTemplate,

    /// Optional extra lines appended to the chosen template body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_body: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<Confirmation>,
}

/// Built-in legal frameworks a deletion email can invoke. The engine owns the
/// actual prose so all recipes stay legally consistent and easy to update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmailTemplate {
    /// California Consumer Privacy Act deletion / opt-out request.
    CcpaDeletion,
    /// GDPR Article 17 erasure request.
    GdprErasure,
    /// Generic US state-privacy-law request (auto-cites the user's state law).
    UsStatePrivacy,
}

/// A broker that must be handled by hand.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManualFlow {
    /// Where the user should start, if there is a web page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opt_out_url: Option<String>,

    /// Ordered, human-readable instructions to print for the user.
    pub steps: Vec<String>,
}

/// What the user has to do after a request is submitted for it to take effect.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Confirmation {
    pub kind: ConfirmationKind,

    /// Instruction shown to the user (e.g. "click the link Spokeo emails you").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,

    /// Hours until the confirmation link expires. Broker links commonly expire
    /// in 24–48h, so the engine surfaces this as an urgent deadline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_hours: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationKind {
    /// The broker emails a link the user must click.
    EmailLink,
    /// The broker emails a reply the user must respond to.
    EmailReply,
    /// No confirmation — the request is complete on submission.
    None,
}

fn default_recheck_days() -> u32 {
    60
}
