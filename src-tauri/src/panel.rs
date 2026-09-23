//! Localhost server for the iTerm2 toolbelt panel.
//!
//! iTerm2's Python API can register a *web view* tool in the toolbelt (the
//! sidebar that holds Session Status); it's just a URL. The companion script in
//! `integrations/iterm2/` registers `http://127.0.0.1:<port>/panel`, and this
//! module serves that page (the bundled `panel.html` + its assets) and one JSON
//! endpoint, `/api/snapshot`, with the same data the widget renders.
//!
//! The snapshot comes from the shared [`Hub`] memo the widget's own polling
//! fills, so the panel costs no extra usage-API requests. Only if the widget's
//! poller has evidently stopped (nothing attempted for longer than its maximum
//! backoff) does the panel fetch for itself.
//!
//! Hardening: bound to 127.0.0.1 only; GET only; the `Host` header must be this
//! loopback address (so a DNS-rebinding page can't read the endpoint); no CORS
//! headers outside debug builds, so other web pages can't read responses; and
//! the snapshot never carries the OAuth token (the usage structs don't hold it).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Manager};

use crate::hub::{self, Hub};
use crate::{env, realusage, usage};

/// Fixed so the iTerm2 script can find it; override with `CLAUDE_USAGE_PANEL_PORT`.
pub const DEFAULT_PORT: u16 = 47821;

/// Fetch limits for the panel itself only when nothing has tried for this long
/// — longer than the widget's 5-minute maximum backoff, so a healthy widget
/// poller is never doubled up.
const PANEL_REFETCH_MS: i64 = 330_000;
/// Re-scan local logs for the panel when the widget's last scan is older than this.
const PANEL_RESCAN_MS: i64 = 60_000;
/// Vite's dev server, allowed to read the API in debug builds so the panel can
/// be developed with hot reload at `http://localhost:1420/panel.html?api=…`.
const DEV_ORIGIN: &str = "http://localhost:1420";

pub fn port() -> u16 {
    std::env::var("CLAUDE_USAGE_PANEL_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// Start serving in the background. A bind failure (port taken, e.g. by a
/// second copy of the app) is logged and otherwise ignored — the widget works
/// without the panel.
pub fn start(app: AppHandle) {
    let port = port();
    let spawned = std::thread::Builder::new().name("panel-server".into()).spawn(move || {
        let listener = match TcpListener::bind(("127.0.0.1", port)) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[panel] can't listen on 127.0.0.1:{port}: {e}");
                return;
            }
        };
        for stream in listener.incoming().flatten() {
            let app = app.clone();
            let _ = std::thread::Builder::new()
                .name("panel-conn".into())
                .spawn(move || handle(stream, &app, port));
        }
    });
    if let Err(e) = spawned {
        eprintln!("[panel] can't start server thread: {e}");
    }
}

struct Request {
    method: String,
    path: String,
    host: String,
    origin: String,
}

fn read_request(stream: &TcpStream) -> Option<Request> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(stream.take(16 * 1024));
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?;
    let path = target.split(['?', '#']).next().unwrap_or("/").to_string();

    let (mut host, mut origin) = (String::new(), String::new());
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).ok()? == 0 {
            break;
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "host" => host = v.trim().to_string(),
                "origin" => origin = v.trim().to_string(),
                _ => {}
            }
        }
    }
    Some(Request { method, path, host, origin })
}

fn respond(mut stream: &TcpStream, status: &str, content_type: &str, body: &[u8], cors: bool) {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n",
        body.len()
    );
    if cors {
        head.push_str(&format!("Access-Control-Allow-Origin: {DEV_ORIGIN}\r\nVary: Origin\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn host_allowed(host: &str, port: u16) -> bool {
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

fn handle(stream: TcpStream, app: &AppHandle, port: u16) {
    let Some(req) = read_request(&stream) else { return };
    if !host_allowed(&req.host, port) {
        return respond(&stream, "403 Forbidden", "text/plain", b"forbidden", false);
    }
    if req.method != "GET" {
        return respond(&stream, "405 Method Not Allowed", "text/plain", b"GET only", false);
    }
    let cors = cfg!(debug_assertions) && req.origin == DEV_ORIGIN;

    match req.path.as_str() {
        "/api/snapshot" => {
            let body = serde_json::to_vec(&snapshot(app)).unwrap_or_default();
            respond(&stream, "200 OK", "application/json", &body, cors)
        }
        "/" | "/panel" | "/panel.html" => serve_asset(&stream, app, "panel.html"),
        p if p.starts_with("/assets/") && !p.contains("..") => serve_asset(&stream, app, p),
        _ => respond(&stream, "404 Not Found", "text/plain", b"not found", false),
    }
}

/// Serve a bundled frontend file. Release builds embed `dist/`; debug builds
/// read `dist/` from disk, so run `npm run build` once after changing the panel.
fn serve_asset(stream: &TcpStream, app: &AppHandle, path: &str) {
    match app.asset_resolver().get(path.to_string()) {
        Some(a) => respond(stream, "200 OK", &a.mime_type, &a.bytes, false),
        None => respond(
            stream,
            "404 Not Found",
            "text/plain",
            b"panel not built - run `npm run build`",
            false,
        ),
    }
}

/// Everything the panel renders: the selected environment's live limits and
/// local aggregation (raw — the panel shares the widget's pace math).
fn snapshot(app: &AppHandle) -> serde_json::Value {
    let hub = app.state::<Hub>();
    let env_id = hub.env();
    let now = chrono::Utc::now().timestamp_millis();
    let _serial = hub.refresh_lock.lock();

    let real = match hub.real(&env_id) {
        Some((attempted, v)) if now - attempted < PANEL_REFETCH_MS => v,
        _ => {
            let v = tauri::async_runtime::block_on(realusage::fetch(&env_id));
            hub.store_real(&env_id, &v, chrono::Utc::now().timestamp_millis());
            hub.real(&env_id).map(|(_, v)| v).unwrap_or(v)
        }
    };

    let usage = match hub.usage(&env_id) {
        Some((at, u)) if now - at < PANEL_RESCAN_MS => u,
        _ => {
            let (session_start, week_start) = hub::window_starts(&real);
            let u = usage::collect_at(env::projects_dir_for(&env_id), session_start, week_start);
            hub.store_usage(&env_id, &u, chrono::Utc::now().timestamp_millis());
            u
        }
    };

    let account = env::account_for(&env_id);
    json!({
        "envId": env_id,
        "accountLabel": if account.org_name.is_empty() { account.email } else { account.org_name },
        "real": real,
        "usage": usage,
        "generatedAt": now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_must_be_this_loopback_port() {
        assert!(host_allowed("127.0.0.1:47821", 47821));
        assert!(host_allowed("localhost:47821", 47821));
        assert!(!host_allowed("127.0.0.1:1420", 47821));
        assert!(!host_allowed("evil.example:47821", 47821));
        assert!(!host_allowed("", 47821));
    }
}
