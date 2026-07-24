//! # odr-recipes
//!
//! The recipe schema for ODR (Open Data Removal): declarative, community-
//! maintainable descriptions of how to opt out of individual data brokers,
//! plus loading and validation.
//!
//! This crate deliberately contains **no** broker-specific logic and **no** IO
//! against brokers. It defines the data model ([`Recipe`]) and turns a
//! directory of YAML files into validated [`LoadedRecipe`]s. The
//! `odr-engine` crate interprets them.

pub mod loader;
pub mod placeholder;
pub mod schema;

pub use loader::{load_dir, load_file, validate, LoadError, LoadedRecipe};
pub use placeholder::Placeholder;
pub use schema::{
    Confirmation, ConfirmationKind, EmailFlow, EmailTemplate, FindListing, Flow, ManualFlow,
    Recipe, Step, Tier, WebFormFlow,
};

/// The JSON Schema for [`Recipe`], as a pretty-printed string.
///
/// Emitted by `odr recipes schema` and checked into the repo so editors can
/// offer autocomplete and validation on recipe YAML, and so CI can diff it.
#[cfg(feature = "schema-gen")]
pub fn json_schema() -> String {
    let schema = schemars::schema_for!(Recipe);
    serde_json::to_string_pretty(&schema).expect("schema serializes")
}
