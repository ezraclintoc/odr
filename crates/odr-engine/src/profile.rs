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
