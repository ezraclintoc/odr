# ODR Conventions

The shared agreement for how we build ODR — so contributors (and AI assistants)
stay on the same page. Keep it short; change it by PR when reality changes.

## Principles

1. **Local-first, always.** A user's personal data never leaves their machine.
   No telemetry, no "cloud sync", no ODR server. If a feature needs a server,
   it's the wrong feature.
2. **Requests are first-party.** ODR acts *as the user*, from their browser and
   email — never as a third-party agent. This is both an ethical stance and a
   practical one (brokers slow-walk agents).
3. **Human-in-the-loop for gates.** We never defeat CAPTCHAs or forge ID. When a
   broker demands one, ODR hands control to the user (`human_step`). This keeps
   the tool legal and honest.
4. **Broker knowledge lives in recipes, not code.** The engine is a stable
   interpreter. If a broker changes its form, the fix is a one-line YAML PR. Any
   time you're tempted to add a broker-specific branch in Rust, extend the
   schema instead.
5. **Honest about limits.** Removals don't stick forever; some brokers can't be
   automated. We say so plainly rather than overpromising.

## Repository layout

```
crates/odr-recipes   schema, loading, validation (no IO against brokers)
crates/odr-engine    interpreter, profile, state, email, browser + human seams
crates/odr-browser   live browser driver (chromiumoxide/CDP); `live` feature
crates/odr-server    local web dashboard + human-in-the-loop task queue
crates/odr-cli       the `odr` binary (thin; logic belongs in the crates)
recipes/             one YAML file per broker (community-maintained data)
docs/                research and design notes
```

Keep logic in the library crates so a future GUI reuses it; the CLI only parses
args and prints.

## Rust conventions

- **Edition 2021**, toolchain pinned in `rust-toolchain.toml`.
- **Formatting:** `cargo fmt` (default rustfmt). CI rejects unformatted code.
- **Lints:** `cargo clippy --workspace --all-targets` must be warning-free.
- **Errors:** libraries return typed errors (`thiserror`); the CLI uses
  `anyhow` with `.context()` for human-readable failure chains. No `unwrap()` /
  `expect()` on paths that can fail at runtime (tests and provable invariants
  excepted).
- **No new heavy dependencies without discussion.** The core stays lean; keep
  optional/large deps behind features (see `schema-gen`).
- **Every public item gets a doc comment** explaining *why*, not just *what*.
- **PII stays in `profile.rs`.** Don't thread personal data through types that
  don't need it.

## Recipe authoring

A recipe describes how to opt out of exactly one broker.

- **`id`** is lowercase-kebab, stable forever (it keys local state). Never rename.
- **No personal data** in a recipe — only `{{placeholders}}` (see
  `odr-recipes/src/placeholder.rs` for the list).
- Pick the narrowest **`method`**: `web_form` > `email` > `manual`. Use `manual`
  only when a broker genuinely can't be automated (phone/fax/ID upload).
- **Use `human_step`** for any CAPTCHA or ID gate — never try to automate past it.
- Set **`confirmation`** with `expires_hours` when the broker emails a link;
  those expire fast and the engine surfaces the deadline.
- Add a **`notes:`** line with the date you last verified the flow.
- Validate with `cargo run -p odr-cli -- recipes check` before opening a PR.
- Seed source for new brokers: the "Big Ass Data Broker Opt-Out List".

## Commits & branches

- **Conventional Commits**: `type(scope): summary`.
  Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `ci`, `recipe`.
  Scope is a crate or `recipes` (e.g. `feat(engine):`, `recipe(spokeo):`).
- Keep commits focused and the tree green (`cargo test && cargo clippy`).
- The commit body ends with the standard co-author trailer when authored by an
  AI assistant.
- One logical change per PR; recipe additions can batch by tier.

## Releases

- **SemVer.** Pre-1.0, breaking changes bump the minor.
- Tagging `vX.Y.Z` triggers the release workflow (multi-platform binaries).
- `main` builds a rolling `nightly` prerelease every night.
- Conventional-commit history drives the changelog.
