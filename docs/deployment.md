# Deployment models

ODR is designed to run the same engine in very different places. Two channels
are abstracted so the executor and recipes never care which deployment they're
in:

- **Browser** ([`odr_engine::browser::BrowserDriver`]) — views and drives the
  broker page.
- **Human interface** ([`odr_engine::interaction::HumanInterface`]) — reaches
  whoever completes manual steps (CAPTCHA, ID, picking a listing).

## 1. Local CLI (today)

```
you ── odr (CLI) ── your Chrome (visible window)
        └────────── terminal prompts (ConsolePrompter)
```

Both channels are local: `LocalBrowser` (planned, `chromiumoxide` over CDP)
drives your own Chrome; `ConsolePrompter` prompts on the terminal. Your profile
and state are local files.

## 2. Headless server / Docker (planned)

Run `odr` on a server (e.g. a home server or VPS) and complete the human steps
from any other computer via a browser — nothing to install on the second
machine.

```
                          ┌──────────── Docker container ────────────┐
                          │  odr (server mode)                        │
   your laptop/phone      │    ├── RemoteBrowser ── headful Chrome ───┼── view stream (noVNC / CDP screencast)
   ── https://host:port ──┼────┤                                      │
   (view + "Done" button) │    └── WebPrompter ──── task queue ───────┼── prompt + completion signal
                          └───────────────────────────────────────────┘
                             profile + state live on the server (encrypted at rest)
```

How it maps onto the abstractions:

- **`RemoteBrowser`** implements `BrowserDriver` by attaching to the headful
  Chrome inside the container. That Chrome's view is exposed to the web — either
  by running it under a VNC server fronted by [noVNC], or by relaying a CDP
  screencast to the page. The user *sees and interacts with the same browser*
  ODR is driving, so they can solve a CAPTCHA or click a broker's checkbox.
- **`WebPrompter`** implements `HumanInterface` by pushing each `HumanTask` onto
  a queue served over HTTP. `request()` blocks until the remote user opens the
  URL, completes the step in the shared browser view, and clicks "Done" (or
  "Skip"). No change to the executor: it already blocks on `human.request()`.

Because the two channels are independent, the *view* transport (VNC vs.
screencast) and the *control* transport (web queue) can evolve separately, and
the local CLI keeps using local implementations of each.

### What this needs (not yet built)

- `RemoteBrowser` + a container image bundling Chrome and a view server.
- `WebPrompter` + a small HTTP server (task list, per-task page, completion
  webhook), likely making the engine loop async or running it on a worker
  thread that blocks on a channel fed by the HTTP handlers.
- Authn on the web endpoint (it can see your PII and a live browser session).
- Encryption at rest for the server-side profile.

### Security notes

- The web endpoint exposes a live, logged-in-ish browser and your personal
  data. It must require authentication and run over TLS. Default to binding
  loopback + a reverse proxy / tunnel rather than exposing a port directly.
- Even server-side, requests still originate from the container's browser and
  the user's own email, keeping them first-party.

[noVNC]: https://novnc.com/
