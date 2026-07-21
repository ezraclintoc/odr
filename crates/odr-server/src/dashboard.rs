//! The dashboard page, served as a single self-contained HTML document.
//!
//! It polls `/api/stats` and `/api/tasks` a couple of times a second and
//! renders the progress summary and the live human-in-the-loop queue. Kept
//! dependency-free (no build step, no external assets) so the binary is the
//! whole app.

/// The dashboard HTML. Static; all data arrives via the JSON endpoints.
pub const PAGE: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>ODR — removal dashboard</title>
<style>
  :root {
    --bg: #f7f8f6; --card: #ffffff; --ink: #17201b; --soft: #5c6a62;
    --line: #e3e8e4; --accent: #0e7c5b; --accent-soft: #e9f4ef;
    --warn: #a8721f; --warn-soft: #f6edd9; --danger: #b23b3b;
  }
  @media (prefers-color-scheme: dark) {
    :root {
      --bg: #121613; --card: #1a211d; --ink: #e6ece8; --soft: #9aa79f;
      --line: #29332d; --accent: #46c493; --accent-soft: #17271f;
      --warn: #d3a24c; --warn-soft: #2a2517; --danger: #e06a6a;
    }
  }
  * { box-sizing: border-box; }
  body {
    margin: 0; background: var(--bg); color: var(--ink);
    font: 15px/1.5 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  }
  .wrap { max-width: 960px; margin: 0 auto; padding: 2rem 1.25rem 4rem; }
  header { display: flex; align-items: baseline; justify-content: space-between; gap: 1rem; flex-wrap: wrap; }
  h1 { font-size: 1.4rem; margin: 0; letter-spacing: -0.01em; }
  .mode { font-size: .78rem; color: var(--warn); background: var(--warn-soft);
    padding: .2rem .55rem; border-radius: 999px; font-weight: 600; }
  .sub { color: var(--soft); margin: .35rem 0 1.5rem; font-size: .9rem; }

  .progress { background: var(--card); border: 1px solid var(--line); border-radius: 12px; padding: 1.1rem 1.25rem; margin-bottom: 1.25rem; }
  .bar { height: 12px; border-radius: 999px; background: var(--line); overflow: hidden; display: flex; }
  .bar > span { display: block; height: 100%; }
  .bar .done { background: var(--accent); }
  .bar .prog { background: color-mix(in srgb, var(--accent) 45%, transparent); }
  .bar-legend { display: flex; gap: 1.25rem; margin-top: .6rem; font-size: .82rem; color: var(--soft); flex-wrap: wrap; }
  .dot { display: inline-block; width: .7em; height: .7em; border-radius: 50%; margin-right: .35em; vertical-align: baseline; }

  .tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); gap: .75rem; margin-bottom: 1.75rem; }
  .tile { background: var(--card); border: 1px solid var(--line); border-radius: 12px; padding: .9rem 1rem; }
  .tile .n { font-size: 1.6rem; font-weight: 700; font-variant-numeric: tabular-nums; }
  .tile .l { font-size: .78rem; color: var(--soft); text-transform: uppercase; letter-spacing: .04em; }
  .tile.accent .n { color: var(--accent); }
  .tile.warn .n { color: var(--warn); }

  h2 { font-size: 1.05rem; margin: 0 0 .25rem; }
  .queue-note { color: var(--soft); font-size: .85rem; margin: 0 0 1rem; }
  .task { background: var(--card); border: 1px solid var(--line); border-radius: 12px;
    padding: 1rem 1.15rem; margin-bottom: .75rem; display: flex; gap: 1rem; align-items: flex-start; flex-wrap: wrap; }
  .task .body { flex: 1 1 320px; min-width: 0; }
  .task .broker { font-weight: 650; }
  .task .kind { font-size: .72rem; color: var(--accent); background: var(--accent-soft);
    padding: .12rem .5rem; border-radius: 999px; margin-left: .5rem; font-weight: 600; }
  .task .prompt { color: var(--soft); margin-top: .3rem; }
  .task .age { font-size: .75rem; color: var(--soft); margin-top: .4rem; }
  .task .actions { display: flex; gap: .5rem; align-items: center; }
  button { font: inherit; font-weight: 600; border: 1px solid var(--line); border-radius: 8px;
    padding: .5rem .9rem; cursor: pointer; background: var(--card); color: var(--ink); }
  button.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .empty { text-align: center; color: var(--soft); padding: 2.5rem 1rem; border: 1px dashed var(--line); border-radius: 12px; }
  .empty b { color: var(--ink); }
  footer { color: var(--soft); font-size: .8rem; margin-top: 2rem; }
</style>
</head>
<body>
<div class="wrap">
  <header>
    <h1>ODR removal dashboard</h1>
    <span class="mode" id="mode">dry-run</span>
  </header>
  <p class="sub">Your data-broker removals, running locally. Tasks below need you to act in the browser — solve a CAPTCHA, pick your listing, confirm an ID. Everything else is automatic.</p>

  <div class="progress">
    <div class="bar" aria-label="progress">
      <span class="done" id="bar-done" style="width:0"></span>
      <span class="prog" id="bar-prog" style="width:0"></span>
    </div>
    <div class="bar-legend">
      <span><span class="dot" style="background:var(--accent)"></span><span id="lg-confirmed">0</span> confirmed</span>
      <span><span class="dot" style="background:color-mix(in srgb, var(--accent) 45%, transparent)"></span><span id="lg-progress">0</span> in progress</span>
      <span><span class="dot" style="background:var(--line)"></span><span id="lg-remaining">0</span> not started</span>
    </div>
  </div>

  <div class="tiles" id="tiles"></div>

  <h2>Tasks that need you</h2>
  <p class="queue-note" id="queue-note">Waiting for tasks…</p>
  <div id="queue"></div>

  <footer>Auto-refreshing. Leave this open while removals run · served by <code>odr serve</code></footer>
</div>

<script>
const $ = (id) => document.getElementById(id);

function tile(n, label, cls) {
  return `<div class="tile ${cls||''}"><div class="n">${n}</div><div class="l">${label}</div></div>`;
}

function ago(iso) {
  const s = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 60) return `${Math.floor(s)}s ago`;
  if (s < 3600) return `${Math.floor(s/60)}m ago`;
  return `${Math.floor(s/3600)}h ago`;
}

async function resolve(id, action) {
  await fetch(`/api/tasks/${id}/${action}`, { method: 'POST' });
  await refresh();
}

async function refresh() {
  try {
    const [stats, tasks] = await Promise.all([
      fetch('/api/stats').then(r => r.json()),
      fetch('/api/tasks').then(r => r.json()),
    ]);

    const total = stats.total_brokers || 1;
    const inProgress = stats.requested + stats.awaiting_confirmation;
    $('bar-done').style.width = (100 * stats.confirmed / total) + '%';
    $('bar-prog').style.width = (100 * inProgress / total) + '%';
    $('lg-confirmed').textContent = stats.confirmed;
    $('lg-progress').textContent = inProgress;
    $('lg-remaining').textContent = stats.not_started;

    $('tiles').innerHTML =
      tile(stats.pending_human_tasks, 'need you', 'warn') +
      tile(stats.confirmed, 'confirmed', 'accent') +
      tile(stats.requested + stats.awaiting_confirmation, 'in progress') +
      tile(stats.recheck_due_now, 're-check due') +
      tile(stats.total_brokers, 'brokers');

    if (tasks.length === 0) {
      $('queue-note').textContent = '';
      $('queue').innerHTML = `<div class="empty"><b>Nothing needs you right now.</b><br>Human-in-the-loop tasks will appear here as removals run.</div>`;
    } else {
      $('queue-note').textContent = `${tasks.length} task${tasks.length===1?'':'s'} waiting.`;
      $('queue').innerHTML = tasks.map(t => `
        <div class="task">
          <div class="body">
            <span class="broker">${t.broker_id}</span><span class="kind">${t.kind}</span>
            <div class="prompt">${t.prompt}</div>
            <div class="age">queued ${ago(t.created)}</div>
          </div>
          <div class="actions">
            <button class="primary" onclick="resolve(${t.id},'complete')">Done</button>
            <button onclick="resolve(${t.id},'skip')">Skip</button>
          </div>
        </div>`).join('');
    }
  } catch (e) {
    $('queue-note').textContent = 'Lost connection to odr — is it still running?';
  }
}

refresh();
setInterval(refresh, 1500);
</script>
</body>
</html>
"##;
