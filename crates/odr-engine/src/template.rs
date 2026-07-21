//! Filling `{{placeholder}}` tokens in recipe strings from a [`Profile`].

use odr_recipes::Placeholder;

use crate::profile::Profile;

/// An error substituting placeholders.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("unknown placeholder `{{{{{0}}}}}` in recipe")]
    UnknownPlaceholder(String),

    #[error("recipe needs `{{{{{token}}}}}` but the profile has no {token}")]
    MissingValue { token: String },

    #[error("unbalanced `{{{{` / `}}}}` in recipe string")]
    Unbalanced,
}

/// Resolve a single placeholder against the profile. Returns `None` (rather than
/// an error) when the field is simply absent, so callers can decide whether a
/// given token is required for a given broker.
fn resolve(profile: &Profile, ph: Placeholder) -> Option<String> {
    let addr = profile.current_address();
    match ph {
        Placeholder::FirstName => Some(profile.first_name.clone()),
        Placeholder::MiddleName => profile.middle_name.clone().filter(|s| !s.is_empty()),
        Placeholder::LastName => Some(profile.last_name.clone()),
        Placeholder::FullName => Some(profile.full_name()),
        Placeholder::Email => Some(profile.email.clone()),
        Placeholder::Phone => profile.phone.clone(),
        Placeholder::Street => addr.map(|a| a.street.clone()),
        Placeholder::City => addr.map(|a| a.city.clone()),
        Placeholder::State => addr.map(|a| a.state.clone()),
        Placeholder::Zip => addr.map(|a| a.zip.clone()),
        Placeholder::DateOfBirth => profile.date_of_birth.clone(),
    }
}

/// Replace every `{{token}}` in `input` with the matching profile value.
///
/// Fails on unknown tokens (a recipe typo) and on known-but-missing values (the
/// profile lacks data this broker needs) — the latter is actionable feedback
/// to the user, not a silent blank submission.
pub fn render(input: &str, profile: &Profile) -> Result<String, RenderError> {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find("}}").ok_or(RenderError::Unbalanced)?;
        let token = after[..end].trim();

        let ph = Placeholder::from_token(token)
            .ok_or_else(|| RenderError::UnknownPlaceholder(token.to_string()))?;
        let value = resolve(profile, ph).ok_or_else(|| RenderError::MissingValue {
            token: token.to_string(),
        })?;
        out.push_str(&value);

        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Address;

    fn sample() -> Profile {
        Profile {
            first_name: "Ada".into(),
            middle_name: None,
            last_name: "Lovelace".into(),
            email: "ada@example.com".into(),
            phone: None,
            addresses: vec![Address {
                street: "12 Analytical Way".into(),
                city: "London".into(),
                state: "CA".into(),
                zip: "90001".into(),
            }],
            date_of_birth: None,
        }
    }

    #[test]
    fn fills_known_tokens() {
        let out = render("{{first_name}} {{last_name}} in {{city}}", &sample()).unwrap();
        assert_eq!(out, "Ada Lovelace in London");
    }

    #[test]
    fn rejects_unknown_token() {
        let err = render("{{social_security}}", &sample()).unwrap_err();
        assert!(matches!(err, RenderError::UnknownPlaceholder(_)));
    }

    #[test]
    fn reports_missing_value() {
        let err = render("call {{phone}}", &sample()).unwrap_err();
        assert!(matches!(err, RenderError::MissingValue { .. }));
    }
}
