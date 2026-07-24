//! # odr-engine
//!
//! The execution core of ODR (Open Data Removal). Given a validated
//! [`odr_recipes::Recipe`] and a local [`Profile`], the engine:
//!
//! - fills the recipe's `{{placeholders}}` from the profile ([`template`]),
//! - drives the user's browser through web-form opt-outs ([`browser`],
//!   [`executor`]),
//! - generates legal deletion emails for email-based brokers ([`email`]),
//! - and tracks per-broker state so removals recur ([`state`]).
//!
//! It holds all the logic that is *common* across brokers and none that is
//! specific to any one broker — that belongs in recipes.

pub mod browser;
pub mod captcha;
pub mod email;
pub mod executor;
pub mod interaction;
pub mod listing;
pub mod profile;
pub mod state;
pub mod template;

pub use browser::{BrowserDriver, BrowserError, DryRunBrowser};
pub use captcha::{CaptchaConfig, CaptchaKind, CaptchaPolicy, CaptchaSolver, CaptchaState};
pub use email::{generate as generate_email, GeneratedEmail};
pub use executor::{execute, execute_with, ExecError, Outcome};
pub use interaction::{
    AutoApprove, ConsolePrompter, HumanInterface, HumanResponse, HumanTask, HumanTaskKind,
    InteractionError,
};
pub use profile::{Address, Profile};
pub use state::{BrokerRecord, JsonStore, StateStore, Status};
pub use template::{render, RenderError};
