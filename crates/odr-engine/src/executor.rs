//! The recipe interpreter: turn a [`Recipe`] into concrete actions.
//!
//! This is the stable core the whole design hinges on. It contains no
//! broker-specific knowledge — every broker quirk lives in a recipe. When a
//! broker changes its form, the fix is a YAML edit, not a change here.

use odr_recipes::{Confirmation, ConfirmationKind, Flow, ManualFlow, Recipe, Step, WebFormFlow};

use crate::browser::{BrowserDriver, BrowserError};
use crate::captcha::{self, CaptchaConfig, Resolution};
use crate::email::{self, GeneratedEmail};
use crate::interaction::{
    HumanInterface, HumanResponse, HumanTask, HumanTaskKind, InteractionError,
};
use crate::listing;
use crate::profile::Profile;
use crate::template::{self, Bindings, RenderError};

/// Anything that can go wrong carrying out a recipe.
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error(transparent)]
    Browser(#[from] BrowserError),
    #[error(transparent)]
    Interaction(#[from] InteractionError),
}

/// The outcome of running a recipe — what the caller should tell the user and
/// how to update state.
#[derive(Debug)]
pub enum Outcome {
    /// A web form was driven to completion.
    FormSubmitted { confirmation: Option<Confirmation> },
    /// An email was generated for the user to send.
    EmailReady {
        email: GeneratedEmail,
        confirmation: Option<Confirmation>,
    },
    /// The broker is manual; here are the instructions to show the user.
    ManualSteps { steps: Vec<String> },
    /// The user skipped this broker at a human step; nothing was submitted.
    SkippedByUser,
}

impl Outcome {
    /// The confirmation requirement attached to this outcome, if any.
    pub fn confirmation(&self) -> Option<&Confirmation> {
        match self {
            Outcome::FormSubmitted { confirmation } | Outcome::EmailReady { confirmation, .. } => {
                confirmation.as_ref()
            }
            Outcome::ManualSteps { .. } | Outcome::SkippedByUser => None,
        }
    }
}

/// Execute `recipe` for `profile`.
///
/// `browser` drives web-form flows; `human` is the control channel used for any
/// step that needs a person (CAPTCHA, ID, picking a listing). Both are traits,
/// so the same call works for a local CLI (real browser + terminal prompts) and
/// a future server deployment (remote-viewable browser + web prompts) without
/// change here.
pub fn execute(
    recipe: &Recipe,
    profile: &Profile,
    browser: &mut dyn BrowserDriver,
    human: &mut dyn HumanInterface,
) -> Result<Outcome, ExecError> {
    execute_with(
        recipe,
        profile,
        browser,
        human,
        &mut CaptchaConfig::default(),
    )
}

/// Like [`execute`], but with explicit control over how CAPTCHA gates are
/// handled (see [`CaptchaConfig`]).
pub fn execute_with(
    recipe: &Recipe,
    profile: &Profile,
    browser: &mut dyn BrowserDriver,
    human: &mut dyn HumanInterface,
    captcha: &mut CaptchaConfig<'_>,
) -> Result<Outcome, ExecError> {
    match &recipe.flow {
        Flow::WebForm(flow) => run_web_form(&recipe.id, flow, profile, browser, human, captcha),
        Flow::Email(flow) => {
            let email = email::generate(flow, profile)?;
            Ok(Outcome::EmailReady {
                email,
                confirmation: flow.confirmation.clone(),
            })
        }
        Flow::Manual(flow) => run_manual(flow, profile),
    }
}

fn run_web_form(
    broker_id: &str,
    flow: &WebFormFlow,
    profile: &Profile,
    browser: &mut dyn BrowserDriver,
    human: &mut dyn HumanInterface,
    captcha: &mut CaptchaConfig<'_>,
) -> Result<Outcome, ExecError> {
    // Values discovered mid-run (e.g. the user's listing URL) that later steps
    // can reference as {{placeholders}}.
    let mut bindings = Bindings::new();

    let page_url = template::render(&flow.opt_out_url, profile)?;
    browser.navigate(&page_url)?;

    for step in &flow.steps {
        match step {
            Step::Navigate { url } => {
                browser.navigate(&template::render_with(url, profile, &bindings)?)?
            }
            Step::Fill { selector, value } => {
                browser.fill(selector, &template::render_with(value, profile, &bindings)?)?
            }
            Step::Select { selector, value } => {
                browser.select(selector, &template::render_with(value, profile, &bindings)?)?
            }
            Step::Click { selector } => browser.click(selector)?,
            Step::WaitFor { selector } => browser.wait_for(selector)?,

            Step::FindListing(find) => {
                let search_url = template::render_with(&find.search_url, profile, &bindings)?;
                browser.navigate(&search_url)?;

                let candidates =
                    listing::extract(browser, &find.result_selector, &find.link_selector)?;
                let needles = find
                    .must_match
                    .iter()
                    .map(|m| template::render_with(m, profile, &bindings))
                    .collect::<Result<Vec<_>, _>>()?;

                match listing::pick(&candidates, &needles) {
                    // Unambiguous hit — no need to involve the user at all.
                    Some(url) => {
                        bindings.insert(find.bind.clone(), url);
                    }
                    // Nothing matched, or several did. Refusing to guess here is
                    // the point: picking wrong would opt out a stranger.
                    None => {
                        let task = HumanTask {
                            broker_id: broker_id.to_string(),
                            prompt: format!(
                                "Couldn't identify your listing automatically ({} candidate(s) \
                                 on {search_url}). Open your own record in the browser, then \
                                 continue.",
                                candidates.len()
                            ),
                            kind: HumanTaskKind::FindListing,
                        };
                        if human.request(&task)? == HumanResponse::Skipped {
                            return Ok(Outcome::SkippedByUser);
                        }
                        // Whatever page they landed on is their listing.
                        let url = listing::current_url(browser)?;
                        bindings.insert(find.bind.clone(), url);
                    }
                }
            }

            Step::HumanStep { prompt } => {
                let kind = classify(prompt);

                // For CAPTCHA gates, the policy may clear the step without ever
                // bothering the user — e.g. nothing is actually blocking, or the
                // challenge resolved itself. Anything else falls through to the
                // human, which always works.
                if kind == HumanTaskKind::Captcha
                    && captcha::resolve(captcha, browser, &page_url)? == Resolution::Clear
                {
                    continue;
                }

                let task = HumanTask {
                    broker_id: broker_id.to_string(),
                    prompt: template::render_with(prompt, profile, &bindings)?,
                    kind,
                };
                if human.request(&task)? == HumanResponse::Skipped {
                    return Ok(Outcome::SkippedByUser);
                }
            }
        }
    }

    Ok(Outcome::FormSubmitted {
        confirmation: flow.confirmation.clone(),
    })
}

/// Best-effort categorization of a human step from its prompt text, so a UI can
/// present it appropriately. Purely cosmetic — the flow behaves the same.
fn classify(prompt: &str) -> HumanTaskKind {
    let p = prompt.to_lowercase();
    if p.contains("captcha") {
        HumanTaskKind::Captcha
    } else if p.contains("id") || p.contains("verify") || p.contains("verification") {
        HumanTaskKind::Verification
    } else if p.contains("listing") || p.contains("search") || p.contains("url") {
        HumanTaskKind::FindListing
    } else {
        HumanTaskKind::Generic
    }
}

fn run_manual(flow: &ManualFlow, profile: &Profile) -> Result<Outcome, ExecError> {
    // Render placeholders in the human-readable steps too, so a manual recipe
    // can say "search for {{full_name}}".
    let steps = flow
        .steps
        .iter()
        .map(|s| template::render(s, profile))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Outcome::ManualSteps { steps })
}

/// Convenience: does this confirmation require the user to act (vs. none)?
pub fn needs_user_confirmation(c: &Confirmation) -> bool {
    !matches!(c.kind, ConfirmationKind::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::DryRunBrowser;
    use crate::interaction::AutoApprove;
    use crate::profile::Address;
    use odr_recipes::load_file;
    use std::io::Write;

    /// A [`HumanInterface`] that skips every task, for testing the skip path.
    struct AlwaysSkip;
    impl HumanInterface for AlwaysSkip {
        fn request(&mut self, _: &HumanTask) -> Result<HumanResponse, InteractionError> {
            Ok(HumanResponse::Skipped)
        }
    }

    fn profile() -> Profile {
        Profile {
            first_name: "Ada".into(),
            middle_name: None,
            last_name: "Lovelace".into(),
            email: "ada@example.com".into(),
            phone: Some("555-0100".into()),
            addresses: vec![Address {
                street: "12 Analytical Way".into(),
                city: "London".into(),
                state: "CA".into(),
                zip: "90001".into(),
            }],
            date_of_birth: None,
        }
    }

    fn recipe_from_yaml(yaml: &str) -> Recipe {
        let mut f = tempfile();
        f.write_all(yaml.as_bytes()).unwrap();
        load_file(f.path()).unwrap()
    }

    // Tiny temp-file helper to avoid a dev-dependency in the scaffold.
    struct TempYaml {
        path: std::path::PathBuf,
    }
    impl TempYaml {
        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }
    impl std::io::Write for TempYaml {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?;
            f.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Drop for TempYaml {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
    fn tempfile() -> TempYaml {
        let mut p = std::env::temp_dir();
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("odr-test-{n}.yaml"));
        TempYaml { path: p }
    }

    #[test]
    fn drives_web_form_in_order() {
        let recipe = recipe_from_yaml(
            "id: example\nname: Example\nhomepage: https://e.com\nmethod: web_form\n\
             opt_out_url: https://e.com/optout\nsteps:\n\
             - action: fill\n  selector: '#name'\n  value: '{{full_name}}'\n\
             - action: click\n  selector: '#submit'\n",
        );
        let mut browser = DryRunBrowser::default();
        let mut human = AutoApprove::default();
        let outcome = execute(&recipe, &profile(), &mut browser, &mut human).unwrap();
        assert!(matches!(outcome, Outcome::FormSubmitted { .. }));
        assert_eq!(
            browser.log,
            vec![
                "navigate https://e.com/optout",
                "fill #name = \"Ada Lovelace\"",
                "click #submit",
            ]
        );
    }

    #[test]
    fn human_skip_aborts_submission() {
        let recipe = recipe_from_yaml(
            "id: example\nname: Example\nhomepage: https://e.com\nmethod: web_form\n\
             opt_out_url: https://e.com/optout\nsteps:\n\
             - action: human_step\n  prompt: 'Solve the CAPTCHA'\n\
             - action: click\n  selector: '#submit'\n",
        );
        let mut browser = DryRunBrowser::default();
        let mut human = AlwaysSkip;
        let outcome = execute(&recipe, &profile(), &mut browser, &mut human).unwrap();
        assert!(matches!(outcome, Outcome::SkippedByUser));
        // The submit click after the skipped human step must not have run.
        assert!(!browser.log.iter().any(|l| l.contains("click")));
    }

    #[test]
    fn generates_email_body() {
        let recipe = recipe_from_yaml(
            "id: acme\nname: Acme\nhomepage: https://acme.com\nmethod: email\n\
             to: privacy@acme.com\nsubject: 'Deletion request for {{full_name}}'\n\
             template: ccpa_deletion\n",
        );
        let mut browser = DryRunBrowser::default();
        let mut human = AutoApprove::default();
        let outcome = execute(&recipe, &profile(), &mut browser, &mut human).unwrap();
        match outcome {
            Outcome::EmailReady { email, .. } => {
                assert_eq!(email.to, "privacy@acme.com");
                assert!(email.subject.contains("Ada Lovelace"));
                assert!(email.body.contains("California Consumer Privacy Act"));
            }
            other => panic!("expected email, got {other:?}"),
        }
    }
}
