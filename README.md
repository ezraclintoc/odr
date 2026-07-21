# ODR — Open Data Removal

**Free, open-source, local-first removal of your personal data from data brokers.**

Commercial services (DeleteMe, Incogni, Optery…) charge $20–400/year to opt you
out of data brokers, and to do it they upload your name, addresses, and date of
birth to their own servers — then act as a third-party "authorized agent" that
brokers deliberately slow-walk. ODR does the same job for free, from your own
machine, as *you*:

- **Local-first.** Your personal data lives in one file on your computer and
  never leaves it. There is no ODR server.
- **First-party requests.** Opt-outs come from your browser, your IP, and your
  email — so brokers can't route you to the slow "agent" queue (2–14 days vs.
  45–90 for agents).
- **Human-in-the-loop.** ODR drives *your* visible browser and hands control
  back to you for CAPTCHAs and ID checks. In Consumer Reports' 2024 study,
  user-submitted opt-outs were the single most effective method measured (70%),
  beating every paid service.
- **Recurring by design.** Brokers repopulate every 30–90 days, so removal only
  works as a cycle. ODR tracks per-broker state and tells you what's due.
- **Community recipes.** Each broker's opt-out is a small YAML file anyone can
  fix when a form changes — no code release required.

> ⚠️ **Status: early scaffold.** The schema, engine, state tracking, email
> generation, and CLI work end-to-end in dry-run. The live browser driver is
> not wired up yet — see [ROADMAP](#roadmap). Contributions welcome.

## How it works

```
        profile.yaml (your PII, local, git-ignored)
                │
   recipes/*.yaml ──▶  odr-engine  ──▶  browser (web form)  ──▶  state.json
   (community data)     interpreter      mailto  (email req)      (recurrence)
                                         guide   (manual)
```

Three crates:

| Crate | Responsibility |
|-------|----------------|
| [`odr-recipes`](crates/odr-recipes) | The declarative broker-opt-out schema, loading, and validation. No broker-specific logic. |
| [`odr-engine`](crates/odr-engine) | Interprets recipes: fills your profile into requests, drives the browser, generates deletion emails, tracks state. |
| [`odr-browser`](crates/odr-browser) | Live browser driver — a real Chrome driven over CDP (chromiumoxide), behind the engine's `BrowserDriver` trait. |
| [`odr-server`](crates/odr-server) | Local web dashboard — progress stats and the live human-in-the-loop task queue (`odr serve`). |
| [`odr-cli`](crates/odr-cli) | The `odr` command. A thin shell over the engine so a GUI can reuse everything. |

## Quick start

```bash
# 1. Build
cargo build --release

# 2. Create your local profile (git-ignored, never leaves your machine)
cp profile.example.yaml profile.yaml && $EDITOR profile.yaml

# 3. See which brokers ODR knows
./target/release/odr brokers

# 4. Preview exactly what a removal would do — no browser, no email sent
./target/release/odr plan spokeo

# 5. Record a removal (dry-run for now) and see what's tracked
./target/release/odr remove acxiom --dry-run
./target/release/odr status

# 6. Or launch the web dashboard: it runs your removals and shows every step
#    that needs you (CAPTCHAs, ID checks) as a live task queue to clear.
./target/release/odr serve   # then open http://127.0.0.1:7373
```

## The dashboard

`odr serve` starts a local web app that runs your removals on background workers
and shows:

- **Progress** — a bar and stat tiles for confirmed / in-progress / not-started
  brokers and what's due for a re-check.
- **Tasks that need you** — a live queue of human-in-the-loop steps. Each broker
  worker parks when it hits a CAPTCHA, ID check, or "find your listing" step;
  you clear them from one page with **Done** / **Skip**, and the worker
  continues. No per-broker tab-juggling.

It's the same engine as the CLI, reached through the web instead of the terminal
(the [`HumanInterface`](crates/odr-engine/src/interaction.rs) seam). The same
seam is what lets ODR run headless on a server later — see
[docs/deployment.md](docs/deployment.md).

## Commands

| Command | What it does |
|---------|--------------|
| `odr brokers [--tier <t>]` | List known brokers and their priority tier. |
| `odr plan <broker>` | Show exactly what a removal would do — browser steps, or the full email, or manual instructions. Touches nothing. |
| `odr remove <broker> [--dry-run] [--attach <url>]` | Run the opt-out and record it. Web-form brokers drive a real Chrome (`--attach http://127.0.0.1:9222` to use a Chrome you started with `--remote-debugging-port=9222`); `--dry-run` previews without a browser. |
| `odr status` | Per-broker status and what's due for a re-check. |
| `odr serve [--addr host:port]` | Launch the web dashboard (progress + human-in-the-loop task queue). |
| `odr recipes check` | Validate every recipe. |
| `odr recipes schema` | Emit the recipe JSON Schema (for editor autocomplete / CI). |

## Recipes

A recipe describes how to opt out of one broker. It contains no personal data —
only `{{placeholders}}` the engine fills from your local profile. Three methods:
`web_form`, `email`, `manual`. See [`recipes/`](recipes) for real examples and
[CONVENTIONS.md](CONVENTIONS.md) for the authoring guide.

```yaml
id: spokeo
name: Spokeo
homepage: https://www.spokeo.com
tier: essential
method: web_form
opt_out_url: https://www.spokeo.com/optout
steps:
  - action: fill
    selector: "input[name='email']"
    value: "{{email}}"
  - action: human_step
    prompt: "Solve the CAPTCHA, then continue."
  - action: click
    selector: "button[type='submit']"
confirmation:
  kind: email_link
  expires_hours: 24
```

## Roadmap

- [x] Recipe schema + validation + JSON Schema generation
- [x] Profile, template substitution, per-broker state tracking
- [x] Email deletion-request generation (CCPA / GDPR / state law)
- [x] CLI: `brokers`, `plan`, `remove --dry-run`, `status`, `recipes`
- [x] Web dashboard (`odr serve`): progress stats + live human-in-the-loop queue
- [x] Live browser driver (`chromiumoxide` over CDP — launches or attaches to Chrome)
- [ ] Reappearance verification scans (`odr verify`)
- [ ] Confirmation-email inbox helper (links expire in 24–48h)
- [ ] Regulator-complaint generation for non-compliant brokers
- [ ] SQLite state backend (behind the existing `StateStore` trait)
- [ ] **Server / Docker mode** — run ODR on a server and complete the human
  steps from any browser (see [docs/deployment.md](docs/deployment.md)); the
  browser and human-prompt channels are already abstracted for this
- [ ] California DROP integration
- [ ] Grow recipe coverage (seeded from the Big Ass Data Broker Opt-Out List)

## Contributing

Adding a broker recipe is the highest-leverage contribution and needs no Rust.
See [CONVENTIONS.md](CONVENTIONS.md). Run `cargo test`, `cargo clippy`, and
`cargo fmt` before opening a PR — CI enforces all three.

## Background

See [docs/research-competitive-landscape.md](docs/research-competitive-landscape.md)
for the research this design is based on, including the Consumer Reports
effectiveness study and the mechanics of broker opt-outs.

## License

MIT © the ODR contributors. See [LICENSE](LICENSE).

ODR is a tool for exercising your own legal rights over your own data. It does
not defeat CAPTCHAs or impersonate anyone — a human is always in the loop for
those steps.
