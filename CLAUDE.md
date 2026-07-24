# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this is

ODR (Open Data Removal) — a free, open-source, **local-first** tool to remove
personal data from data brokers. It's the free alternative to DeleteMe/Incogni,
built so the user's PII never leaves their machine.

Read [CONVENTIONS.md](CONVENTIONS.md) before making changes; it holds the
project's principles and rules. The short version of what matters most:

1. **Local-first, always.** No telemetry, no server, no PII leaving the machine.
   If a feature needs a backend, it's the wrong feature.
2. **Requests are first-party.** ODR acts *as the user*, from their browser and
   email — never as a third-party agent (brokers slow-walk agents 45–90 days
   vs. 2–14 for first-party).
3. **Never defeat CAPTCHAs or forge ID.** When a broker gates on one, hand
   control to the human via `HumanInterface`. This is a deliberate design
   stance, not a limitation — see below.
4. **Broker knowledge lives in recipes (YAML), not Rust.** If you're tempted to
   add a broker-specific branch in code, extend the schema instead.

## Commands

```bash
cargo build --workspace          # build everything
cargo test --workspace           # unit/integration tests
cargo clippy --workspace --all-targets   # must be warning-free
cargo fmt --all                  # must be clean; CI enforces

cargo run -p odr-cli -- recipes check    # validate all recipes
cargo run -p odr-cli -- brokers          # list known brokers
cargo run -p odr-cli -- plan <broker>    # preview a removal (no browser)
cargo run -p odr-cli -- serve            # web dashboard on :7373

# Live browser test — needs a real Chromium, ignored by default:
nix-shell -p chromium --run 'export CHROME=$(command -v chromium); \
  cargo test -p odr-browser -- --ignored'

# Lean build without the heavy Chrome deps:
cargo build -p odr-cli --no-default-features
```

A local `profile.yaml` (git-ignored) is required by `plan`/`remove`/`serve`:
`cp profile.example.yaml profile.yaml`.

## Architecture

```
recipes/*.yaml  ──▶  odr-engine  ──▶  odr-browser (web form, real Chrome)
(community data)     interpreter      mailto      (email request)
profile.yaml    ──▶                   guide       (manual broker)
(user PII, local)         │
                          └──▶ state.json (per-broker recurrence tracking)
```

| Crate | Role |
|-------|------|
| `odr-recipes` | Recipe schema, loading, validation. **No** broker-specific logic, no IO against brokers. |
| `odr-engine` | The interpreter + profile, templating, state, email generation. Owns the two abstraction seams below. |
| `odr-browser` | Live `BrowserDriver` over CDP (chromiumoxide). Behind the CLI's default `live` feature. |
| `odr-inbox` | Reads broker confirmation emails over IMAP (`odr confirm`). Behind the default `inbox` feature. |
| `odr-server` | Local web dashboard (`odr serve`) + human-in-the-loop task queue. |
| `odr-cli` | The `odr` binary. Thin — logic belongs in the libraries so a GUI can reuse it. |

### The two seams (the core design idea)

The engine deliberately separates:

- **`BrowserDriver`** (`odr-engine/src/browser.rs`) — *views and drives* the
  page. Impls: `DryRunBrowser` (records actions, used by `plan`/`--dry-run`),
  `odr_browser::LocalBrowser` (real Chrome over CDP).
- **`HumanInterface`** (`odr-engine/src/interaction.rs`) — the *control channel*
  reaching whoever completes manual steps. Impls: `ConsolePrompter` (terminal),
  `AutoApprove` (tests/dry-run), `odr_server::WebPrompter` (web dashboard).

Keeping them independent is what lets the same engine run as a local CLI or,
later, headless on a server with the browser viewed remotely. See
[docs/deployment.md](docs/deployment.md). **Preserve this separation** — don't
let browser code reach for the human, or vice versa.

## Recipes

One YAML file per broker in `recipes/`. Three methods: `web_form`, `email`,
`manual`. Recipes contain **no personal data** — only `{{placeholders}}` the
engine fills from the local profile (valid tokens: `odr-recipes/src/placeholder.rs`).

- `id` is lowercase-kebab and **stable forever** — it keys local state.
- Put fragile steps (find-your-listing, CAPTCHA) behind `human_step`, so the
  automated selectors stay minimal and rarely break.
- Set `confirmation.expires_hours` when the broker emails a link — they expire
  in 24–48h and the engine surfaces the deadline.
- Validate with `cargo run -p odr-cli -- recipes check`.

## Gotchas discovered the hard way

- **`deny_unknown_fields` + `#[serde(flatten)]` are incompatible.** `Recipe`
  flattens `Flow`, so it can't use `deny_unknown_fields`; the inner flow structs
  carry it instead. Don't "fix" this by adding it to `Recipe`.
- **The CDP handler loop must not break on errors.** In `odr-browser`, drain
  with `while handler.next().await.is_some() {}`. Breaking on the first `Err`
  (as the chromiumoxide README does) kills the handler on benign events and
  every later call fails with `"oneshot canceled"`.
- **Chromium needs `--no-sandbox --disable-dev-shm-usage --disable-gpu`** in
  containers/CI or it dies on launch.
- **Nix files are intentionally git-ignored**, so the dev shell is `shell.nix`
  (plain nix-shell), *not* a flake — flakes only see git-tracked files. `.envrc`
  uses `use nix`. Don't convert it to a flake without also tracking the files.
- **`odr-inbox` needs OpenSSL** (the `imap` crate only supports native-tls), so
  build inside `nix-shell` on NixOS. Release binaries use `--features
  vendored-tls` to statically link it; don't make that the default, it costs
  every contributor a multi-minute OpenSSL build.
- **Minimising human interruptions is a product requirement**, not a nice-to-
  have. Before adding a `human_step` to a recipe, check whether `find_listing`,
  the CAPTCHA policy, or `odr confirm` already covers it. The unavoidable floor
  is the `manual`-method brokers (phone/fax/ID).
- The CLI binary is stale after `cargo test`; run `cargo build` before smoke-
  testing `./target/debug/odr`.

## Testing notes

Tests requiring a real browser are `#[ignore]`d (`odr-browser/tests/live.rs`);
CI does not run them. Everything else must pass without network or a browser.

## Context

[docs/research-competitive-landscape.md](docs/research-competitive-landscape.md)
has the research the design rests on. Key facts that justify the architecture:
paid services only kept 35% of data removed after 4 months, while
**user-submitted manual opt-outs scored 70%** — the best result measured. Brokers
repopulate every 30–90 days, which is why recurrence (not one-shot removal) is
the core feature.
