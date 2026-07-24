//! The user's local profile: the personal data ODR fills into requests.
//!
//! This is the *only* place PII lives, and it never leaves the user's machine.
//! It is loaded from a local file the user controls and is git-ignored by
//! default. Treat every field here as sensitive.

use serde::{Deserialize, Serialize};

/// A person's details, used to fill broker opt-out requests on their behalf.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub first_name: String,
    #[serde(default)]
    pub middle_name: Option<String>,
    pub last_name: String,

    /// Email address used as the reply-to / confirmation inbox for requests.
    pub email: String,

    #[serde(default)]
    pub phone: Option<String>,

    /// Known addresses. Brokers key on address history, so listing prior
    /// addresses improves match rates. The first is treated as current.
    #[serde(default)]
    pub addresses: Vec<Address>,

    /// Date of birth as `YYYY-MM-DD`. Optional — some brokers match on it, but
    /// the user may prefer not to hand it over.
    #[serde(default)]
    pub date_of_birth: Option<String>,

    /// Optional IMAP access so ODR can click broker confirmation links for you.
    /// Without it, you click them yourself.
    #[serde(default)]
    pub inbox: Option<InboxConfig>,
}

/// How to reach the mailbox that receives broker confirmation emails.
///
/// Brokers email links that expire in 24–48h, and a full run can produce a
/// dozen of them — the single largest remaining source of manual work. Given
/// read access to the inbox, ODR opens those links itself.
///
/// This is a plain data type so the profile stays dependency-free; `odr-inbox`
/// consumes it. Credentials never leave the machine, and using a dedicated
/// address (or an app-specific password) keeps the blast radius small — brokers
/// only ever see this address anyway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxConfig {
    pub imap_host: String,

    #[serde(default = "default_imap_port")]
    pub imap_port: u16,

    pub username: String,

    /// The password directly. Prefer [`Self::password_command`] so a secret
    /// isn't sitting in a plaintext file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    /// A shell command printing the password on stdout, e.g.
    /// `pass show odr/imap` or `secret-tool lookup service odr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_command: Option<String>,

    /// Mailbox to search.
    #[serde(default = "default_mailbox")]
    pub mailbox: String,
}

fn default_imap_port() -> u16 {
    993
}

fn default_mailbox() -> String {
    "INBOX".to_string()
}

/// A postal address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub street: String,
    pub city: String,
    /// Two-letter state/region code (e.g. `CA`). Drives which state-privacy law
    /// a generic request cites.
    pub state: String,
    pub zip: String,
}

impl Profile {
    /// Full name, joining the middle name when present.
    pub fn full_name(&self) -> String {
        match &self.middle_name {
            Some(m) if !m.is_empty() => {
                format!("{} {} {}", self.first_name, m, self.last_name)
            }
            _ => format!("{} {}", self.first_name, self.last_name),
        }
    }

    /// The current (first) address, if any.
    pub fn current_address(&self) -> Option<&Address> {
        self.addresses.first()
    }
}
