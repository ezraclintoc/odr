//! Placeholders that recipes may embed in `String` fields.
//!
//! A recipe never contains real personal data. Instead it uses `{{tokens}}`
//! that the engine substitutes from the user's local profile at execution
//! time. This keeps recipes shareable and PII-free — a recipe is public data,
//! a profile is private.
//!
//! Keeping the canonical list here lets both the engine (substitution) and
//! recipe validation (catching typo'd tokens) agree on what's valid.

use std::fmt;

/// A placeholder token usable inside recipe strings as `{{name}}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placeholder {
    FirstName,
    MiddleName,
    LastName,
    FullName,
    Email,
    Phone,
    Street,
    City,
    State,
    Zip,
    DateOfBirth,
}

impl Placeholder {
    /// The token as it appears between the braces (e.g. `first_name`).
    pub fn token(self) -> &'static str {
        match self {
            Placeholder::FirstName => "first_name",
            Placeholder::MiddleName => "middle_name",
            Placeholder::LastName => "last_name",
            Placeholder::FullName => "full_name",
            Placeholder::Email => "email",
            Placeholder::Phone => "phone",
            Placeholder::Street => "street",
            Placeholder::City => "city",
            Placeholder::State => "state",
            Placeholder::Zip => "zip",
            Placeholder::DateOfBirth => "date_of_birth",
        }
    }

    /// Every known placeholder, for validation and documentation.
    pub const ALL: [Placeholder; 11] = [
        Placeholder::FirstName,
        Placeholder::MiddleName,
        Placeholder::LastName,
        Placeholder::FullName,
        Placeholder::Email,
        Placeholder::Phone,
        Placeholder::Street,
        Placeholder::City,
        Placeholder::State,
        Placeholder::Zip,
        Placeholder::DateOfBirth,
    ];

    /// Parse a token string back into a [`Placeholder`].
    pub fn from_token(token: &str) -> Option<Placeholder> {
        Placeholder::ALL.into_iter().find(|p| p.token() == token)
    }
}

impl fmt::Display for Placeholder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{{{}}}}}", self.token())
    }
}
