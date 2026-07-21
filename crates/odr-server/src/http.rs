//! The HTTP server: serves the dashboard and the small JSON API it polls.

use std::sync::{Arc, Mutex};

use odr_engine::{HumanResponse, JsonStore};
use odr_recipes::LoadedRecipe;
use tiny_http::{Header, Method, Response, Server};

use crate::dashboard;
use crate::hub::Hub;
use crate::stats;

/// Run the blocking HTTP loop. Returns only on fatal server error.
pub fn serve(
    addr: &str,
    hub: Arc<Hub>,
    recipes: Arc<Vec<LoadedRecipe>>,
    store: Arc<Mutex<JsonStore>>,
) -> anyhow::Result<()> {
    let server = Server::http(addr).map_err(|e| anyhow::anyhow!("failed to bind {addr}: {e}"))?;
    for request in server.incoming_requests() {
        handle(request, &hub, &recipes, &store);
    }
    Ok(())
}

fn handle(
    request: tiny_http::Request,
    hub: &Arc<Hub>,
    recipes: &[LoadedRecipe],
    store: &Arc<Mutex<JsonStore>>,
) {
    let method = request.method().clone();
    let url = request.url().to_string();

    let result = match (&method, url.as_str()) {
        (Method::Get, "/") => respond_html(request, dashboard::PAGE),
        (Method::Get, "/api/tasks") => {
            let body = serde_json::to_string(&hub.pending()).unwrap_or_else(|_| "[]".into());
            respond_json(request, &body)
        }
        (Method::Get, "/api/stats") => {
            let (completed, skipped) = hub.resolved_counts();
            let mut stats = {
                let store = store.lock().expect("store lock");
                stats::compute(recipes, &store, hub.pending_count())
            };
            stats.completed_tasks = completed;
            stats.skipped_tasks = skipped;
            let body = serde_json::to_string(&stats).unwrap_or_else(|_| "{}".into());
            respond_json(request, &body)
        }
        (Method::Post, path) if is_task_action(path, "complete") => {
            resolve_task(request, hub, path, HumanResponse::Completed)
        }
        (Method::Post, path) if is_task_action(path, "skip") => {
            resolve_task(request, hub, path, HumanResponse::Skipped)
        }
        _ => respond_status(request, 404, "not found"),
    };

    if let Err(e) = result {
        eprintln!("odr-server: response error: {e}");
    }
}

/// Match `/api/tasks/{id}/{action}` and return whether it fits.
fn is_task_action(path: &str, action: &str) -> bool {
    parse_task_action(path).is_some_and(|(_, a)| a == action)
}

/// Parse `/api/tasks/{id}/{action}` into `(id, action)`.
fn parse_task_action(path: &str) -> Option<(u64, &str)> {
    let rest = path.strip_prefix("/api/tasks/")?;
    let (id, action) = rest.split_once('/')?;
    Some((id.parse().ok()?, action))
}

fn resolve_task(
    request: tiny_http::Request,
    hub: &Arc<Hub>,
    path: &str,
    response: HumanResponse,
) -> std::io::Result<()> {
    let ok = parse_task_action(path)
        .map(|(id, _)| hub.resolve(id, response))
        .unwrap_or(false);
    if ok {
        respond_json(request, r#"{"ok":true}"#)
    } else {
        respond_status(
            request,
            404,
            r#"{"ok":false,"error":"no such pending task"}"#,
        )
    }
}

fn respond_html(request: tiny_http::Request, html: &str) -> std::io::Result<()> {
    let header = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
        .expect("valid header");
    request.respond(Response::from_string(html).with_header(header))
}

fn respond_json(request: tiny_http::Request, json: &str) -> std::io::Result<()> {
    let header =
        Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).expect("valid header");
    request.respond(Response::from_string(json).with_header(header))
}

fn respond_status(request: tiny_http::Request, code: u16, body: &str) -> std::io::Result<()> {
    request.respond(Response::from_string(body).with_status_code(code))
}
