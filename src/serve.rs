//! Development server: serves the built site, rebuilds it when a source file
//! changes, and reloads open pages after each rebuild.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use tiny_http::{Header, Response, Server};

use crate::build;

const WATCHED: &[&str] = &["posts", "pages", "templates", "sass", "static", "site.toml"];

/// Polls the server and reloads the page when the site is rebuilt.
const RELOAD_SCRIPT: &str = r#"<script>
// Start polling only after the page has loaded, to not delay its load event.
window.addEventListener("load", () => {
  let version = null;
  setInterval(async () => {
    try {
      const v = await (await fetch("/__version")).text();
      if (version !== null && v !== version) location.reload();
      version = v;
    } catch (e) {}
  }, 1000);
});
</script>"#;

fn rebuild(root: &Path, out: &Path, version: &AtomicU64) {
    match build::build(root, out, Some(RELOAD_SCRIPT)) {
        Ok(()) => println!("site rebuilt"),
        Err(e) => eprintln!("build failed: {e:?}"),
    }
    version.fetch_add(1, Ordering::SeqCst);
}

pub fn serve(root: &Path, out: &Path, port: u16) -> Result<()> {
    let version = Arc::new(AtomicU64::new(0));
    rebuild(root, out, &version);

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;
    for path in WATCHED {
        let path = root.join(path);
        if path.exists() {
            watcher.watch(&path, RecursiveMode::Recursive)?;
        }
    }
    {
        let (root, out, version) = (root.to_path_buf(), out.to_path_buf(), version.clone());
        std::thread::spawn(move || {
            // Opening or reading a file is not a change.
            let is_change =
                |e: &notify::Result<notify::Event>| e.as_ref().is_ok_and(|e| !e.kind.is_access());
            while let Ok(event) = rx.recv() {
                if !is_change(&event) {
                    continue;
                }
                // Wait for a burst of events (e.g. an editor saving) to end.
                while rx.recv_timeout(Duration::from_millis(100)).is_ok() {}
                rebuild(&root, &out, &version);
            }
        });
    }

    let server = Server::http(("127.0.0.1", port)).map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("serving on http://localhost:{port}");
    for request in server.incoming_requests() {
        let url = request
            .url()
            .split(['?', '#'])
            .next()
            .unwrap_or("/")
            .to_string();
        let response = if url == "/__version" {
            Response::from_string(version.load(Ordering::SeqCst).to_string()).boxed()
        } else {
            match resolve(out, &url) {
                Some(path) => file_response(&path, 200),
                None => file_response(&out.join("404.html"), 404),
            }
        };
        let _ = request.respond(response);
    }
    Ok(())
}

fn resolve(out: &Path, url: &str) -> Option<PathBuf> {
    let rel = percent_decode(url.trim_start_matches('/'));
    if rel.split('/').any(|c| c == "..") {
        return None;
    }
    let mut path = out.join(rel);
    if path.is_dir() {
        path.push("index.html");
    }
    path.is_file().then_some(path)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn file_response(path: &Path, status: u16) -> tiny_http::ResponseBox {
    let Ok(data) = std::fs::read(path) else {
        return Response::from_string("not found")
            .with_status_code(404)
            .boxed();
    };
    let content_type = match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css",
        Some("js") => "text/javascript",
        Some("xml") => "application/xml",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    };
    Response::from_data(data)
        .with_status_code(status)
        .with_header(Header::from_bytes("Content-Type", content_type).unwrap())
        .boxed()
}
