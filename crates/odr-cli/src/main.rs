//! `odr` — the Open Data Removal command-line interface.
//!
//! This is a thin shell over `odr-engine` and `odr-recipes`: it parses args,
//! loads the local profile and recipes, and prints results. All the real work
//! lives in the library crates so a future GUI can reuse it.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use odr_engine::{
    execute, AutoApprove, BrowserDriver, ConsolePrompter, DryRunBrowser, HumanInterface, JsonStore,
    Outcome, Profile, StateStore, Status,
};
use odr_recipes::{load_dir, Flow, LoadedRecipe, Tier};

/// Open Data Removal — free, local-first removal of your personal data from
/// data brokers.
#[derive(Parser)]
#[command(name = "odr", version, about)]
struct Cli {
    /// Directory of broker recipe YAML files.
    #[arg(long, default_value = "recipes", global = true)]
    recipes: PathBuf,

    /// Local profile file (your personal data — never leaves this machine).
    #[arg(long, default_value = "profile.yaml", global = true)]
    profile: PathBuf,

    /// Local state file tracking per-broker progress.
    #[arg(long, default_value = "state.json", global = true)]
    state: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the brokers ODR knows how to opt out of.
    Brokers {
        /// Only show one tier (essential, additional, id-required, offline-only).
        #[arg(long)]
        tier: Option<String>,
    },

    /// Show what a removal would do for one broker, without touching a browser.
    Plan {
        /// Broker id (see `odr brokers`).
        broker: String,
    },

    /// Run the opt-out for one broker.
    Remove {
        /// Broker id.
        broker: String,
        /// Print actions instead of driving a real browser / sending email.
        #[arg(long)]
        dry_run: bool,
        /// Attach to an already-running Chrome's DevTools endpoint instead of
        /// launching one (e.g. http://127.0.0.1:9222). Start Chrome with
        /// --remote-debugging-port=9222 first.
        #[arg(long, value_name = "ENDPOINT")]
        attach: Option<String>,
    },

    /// Show per-broker removal status and what's due.
    Status,

    /// Launch the web dashboard: progress, stats, and the queue of steps that
    /// need you (CAPTCHAs, ID checks). Open the printed URL in a browser.
    Serve {
        /// Address to bind (host:port).
        #[arg(long, default_value = "127.0.0.1:7373")]
        addr: String,
    },

    /// Recipe maintenance helpers.
    #[command(subcommand)]
    Recipes(RecipesCmd),
}

#[derive(Subcommand)]
enum RecipesCmd {
    /// Validate every recipe in the recipes directory.
    Check,
    /// Print the recipe JSON Schema (for editor autocomplete / CI).
    Schema,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Brokers { tier } => cmd_brokers(&cli, tier.as_deref()),
        Command::Plan { broker } => cmd_plan(&cli, broker),
        Command::Remove {
            broker,
            dry_run,
            attach,
        } => cmd_remove(&cli, broker, *dry_run, attach.as_deref()),
        Command::Status => cmd_status(&cli),
        Command::Serve { addr } => cmd_serve(&cli, addr),
        Command::Recipes(RecipesCmd::Check) => cmd_recipes_check(&cli),
        Command::Recipes(RecipesCmd::Schema) => cmd_recipes_schema(),
    }
}

fn load_recipes(cli: &Cli) -> Result<Vec<LoadedRecipe>> {
    load_dir(&cli.recipes)
        .with_context(|| format!("loading recipes from {}", cli.recipes.display()))
}

fn find_recipe(cli: &Cli, broker: &str) -> Result<LoadedRecipe> {
    load_recipes(cli)?
        .into_iter()
        .find(|r| r.recipe.id == broker)
        .with_context(|| format!("no recipe with id `{broker}` (try `odr brokers`)"))
}

fn load_profile(cli: &Cli) -> Result<Profile> {
    let text = std::fs::read_to_string(&cli.profile).with_context(|| {
        format!(
            "reading profile {} (copy profile.example.yaml to get started)",
            cli.profile.display()
        )
    })?;
    let profile = serde_yaml_ng::from_str(&text)
        .with_context(|| format!("parsing profile {}", cli.profile.display()))?;
    Ok(profile)
}

fn tier_label(t: Tier) -> &'static str {
    match t {
        Tier::Essential => "essential",
        Tier::Additional => "additional",
        Tier::IdRequired => "id-required",
        Tier::OfflineOnly => "offline-only",
    }
}

fn cmd_brokers(cli: &Cli, tier_filter: Option<&str>) -> Result<()> {
    let mut recipes = load_recipes(cli)?;
    recipes.sort_by(|a, b| a.recipe.id.cmp(&b.recipe.id));
    println!("{:<20} {:<13} NAME", "ID", "TIER");
    for r in &recipes {
        let label = tier_label(r.recipe.tier);
        if tier_filter.is_some_and(|f| f != label) {
            continue;
        }
        println!("{:<20} {:<13} {}", r.recipe.id, label, r.recipe.name);
    }
    Ok(())
}

fn cmd_plan(cli: &Cli, broker: &str) -> Result<()> {
    let loaded = find_recipe(cli, broker)?;
    let profile = load_profile(cli)?;
    // Planning never touches a real browser or a real human: the dry-run
    // browser records the steps and every human gate is auto-approved so we can
    // show the full flow.
    let mut browser = DryRunBrowser::default();
    let mut human = AutoApprove::default();
    let outcome = execute(&loaded.recipe, &profile, &mut browser, &mut human)
        .with_context(|| format!("planning removal for `{broker}`"))?;

    println!("Plan for {} ({})\n", loaded.recipe.name, loaded.recipe.id);
    match &outcome {
        Outcome::FormSubmitted { .. } => {
            println!("Web form — actions ODR would take in your browser:");
            for line in &browser.log {
                println!("  · {line}");
            }
            if !human.seen.is_empty() {
                println!("\nYou'll be asked to (in your browser):");
                for step in &human.seen {
                    println!("  · {step}");
                }
            }
        }
        Outcome::EmailReady { email, .. } => {
            println!("Email to: {}", email.to);
            println!("Subject:  {}\n", email.subject);
            println!("{}", email.body);
        }
        Outcome::ManualSteps { steps } => {
            println!("Manual broker — do these yourself:");
            for (i, s) in steps.iter().enumerate() {
                println!("  {}. {s}", i + 1);
            }
        }
        Outcome::SkippedByUser => unreachable!("planning auto-approves human steps"),
    }
    if let Some(c) = outcome.confirmation() {
        if let Some(note) = &c.note {
            let note = odr_engine::render(note, &profile).unwrap_or_else(|_| note.clone());
            println!("\n⚠ After submitting: {note}");
        }
        if let Some(h) = c.expires_hours {
            println!("  (the confirmation link expires in ~{h}h — act fast)");
        }
    }
    Ok(())
}

fn cmd_remove(cli: &Cli, broker: &str, dry_run: bool, attach: Option<&str>) -> Result<()> {
    let loaded = find_recipe(cli, broker)?;
    let profile = load_profile(cli)?;
    let id = &loaded.recipe.id;

    // Only web-form recipes need a browser; email/manual don't, so we never
    // launch Chrome just to draft an email.
    let is_web_form = matches!(loaded.recipe.flow, Flow::WebForm(_));
    let live = !dry_run && is_web_form;

    // Live web-form runs drive a real browser and prompt on the terminal; dry
    // runs (and non-browser flows) preview with the recording browser.
    let mut live_browser;
    let mut dry_browser = DryRunBrowser::default();
    let browser: &mut dyn BrowserDriver = if live {
        live_browser = make_live_browser(attach)?;
        &mut live_browser
    } else {
        &mut dry_browser
    };

    let mut auto = AutoApprove::default();
    let mut console = ConsolePrompter;
    let human: &mut dyn HumanInterface = if dry_run { &mut auto } else { &mut console };

    if live {
        println!("Opening a browser to opt out of {}…", loaded.recipe.name);
    }
    let outcome = execute(&loaded.recipe, &profile, browser, human)?;

    match &outcome {
        Outcome::SkippedByUser => {
            println!("Skipped {id}.");
            return Ok(());
        }
        Outcome::FormSubmitted { .. } => {
            let how = if dry_run {
                "[dry-run] would submit"
            } else {
                "Submitted"
            };
            println!("{how} the opt-out form for {}.", loaded.recipe.name);
        }
        Outcome::EmailReady { email, .. } => {
            println!(
                "Send this email to opt out of {} — ODR won't send it for you:\n",
                loaded.recipe.name
            );
            println!("To: {}", email.to);
            println!("Subject: {}\n", email.subject);
            println!("{}\n", email.body);
        }
        Outcome::ManualSteps { steps } => {
            println!("{} requires manual steps:", loaded.recipe.name);
            for (i, s) in steps.iter().enumerate() {
                println!("  {}. {s}", i + 1);
            }
        }
    }

    let mut store = JsonStore::open(&cli.state)?;
    let confirm_hours = outcome.confirmation().and_then(|c| c.expires_hours);
    store.mark_requested(
        id,
        loaded.recipe.recheck_days,
        confirm_hours,
        chrono::Utc::now(),
    );
    store.save()?;

    if let Some(c) = outcome.confirmation() {
        if let Some(note) = &c.note {
            let note = odr_engine::render(note, &profile).unwrap_or_else(|_| note.clone());
            println!("\n⚠ Next: {note}");
        }
        if let Some(h) = c.expires_hours {
            println!("  (that link expires in ~{h}h — act fast)");
        }
    }
    Ok(())
}

/// Build the live browser driver, launching a visible Chrome or attaching to an
/// existing one. Only available when built with the `live` feature.
#[cfg(feature = "live")]
fn make_live_browser(attach: Option<&str>) -> Result<odr_browser::LocalBrowser> {
    match attach {
        Some(endpoint) => odr_browser::LocalBrowser::connect(endpoint),
        None => odr_browser::LocalBrowser::launch(),
    }
    .map_err(|e| anyhow::anyhow!("starting browser: {e}"))
}

#[cfg(not(feature = "live"))]
fn make_live_browser(_attach: Option<&str>) -> Result<DryRunBrowser> {
    anyhow::bail!(
        "this build has no live browser driver.\n\
         Rebuild with the `live` feature, or use `--dry-run` to preview."
    )
}

fn cmd_status(cli: &Cli) -> Result<()> {
    let store = JsonStore::open(&cli.state)?;
    let now = chrono::Utc::now();
    let records = store.all();
    if records.is_empty() {
        println!("No removals tracked yet. Run `odr remove <broker>` to start.");
        return Ok(());
    }
    println!("{:<20} {:<22} NEXT", "BROKER", "STATUS");
    for (id, rec) in records {
        let status = match rec.status {
            Status::NotStarted => "not started",
            Status::Requested => "requested",
            Status::AwaitingConfirmation => "awaiting confirmation",
            Status::Confirmed => "confirmed",
            Status::Reappeared => "reappeared!",
            Status::Failed => "failed",
        };
        let next = if rec.is_recheck_due(now) {
            "re-check due now".to_string()
        } else if let Some(due) = rec.recheck_due {
            format!("re-check {}", due.date_naive())
        } else {
            "-".to_string()
        };
        println!("{id:<20} {status:<22} {next}");
    }
    Ok(())
}

fn cmd_serve(cli: &Cli, addr: &str) -> Result<()> {
    let recipes = load_recipes(cli)?;
    let profile = load_profile(cli)?;
    odr_server::run(addr, recipes, profile, cli.state.clone())
        .with_context(|| format!("running dashboard on {addr}"))
}

fn cmd_recipes_check(cli: &Cli) -> Result<()> {
    let recipes = load_recipes(cli)?;
    println!("✓ {} recipe(s) valid", recipes.len());
    Ok(())
}

fn cmd_recipes_schema() -> Result<()> {
    println!("{}", odr_recipes::json_schema());
    Ok(())
}
