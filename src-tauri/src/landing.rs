//! One page that lists every site in the workspace (M-4).
//!
//! Every rival in this category has one — it is the first thing their installer
//! shows and the address people bookmark. StackVo has had the *name* for it
//! since the beginning and nothing answering on it: `commands::core_domains`
//! already puts the bare suffix (`stackvo.loc`) in the hosts file, and
//! `certs::required_domains` already issues a certificate for it. Opening it
//! got Traefik's own 404, which is the worst of both — a name the app went out
//! of its way to make resolve, serving nothing.
//!
//! ## Why this is a container and not a file
//!
//! Writing an `index.html` and opening it with `file://` needs no image, no
//! network and no certificate, and it was the first design. It is also not the
//! thing anybody means: a landing page is a URL you bookmark and hand to the
//! new person on the team, and a `file://` path is neither shareable nor the
//! same on two machines. It cannot use the trusted certificate either, so every
//! link on it crosses an origin boundary from nowhere.
//!
//! So it is a sidecar, in the shape [`crate::tunnel`] already established: one
//! `docker run` with Traefik labels, on the stack network, `--rm` so stopping
//! is removal. The generated compose file is deliberately not touched — it is
//! compared byte for byte against the Bash generator's output, and a service
//! that only this app knows about would break that comparison for a page.
//!
//! ## What it does not do
//!
//! It does not proxy, redirect or watch. The page is **rendered when it is
//! started or refreshed**, not on every request: a static file is the only
//! thing that can be served without giving a container in the stack network a
//! reason to read the workspace. Everything on it therefore states the moment
//! it was written, and the page says so rather than looking live.

use crate::error::{Error, Result};
use serde::Serialize;

/// The engine prefixes `stackvo-`, so the container is `stackvo-landing`.
pub const ID: &str = "landing";

/// nginx rather than the `alpine:3` this app already pulls elsewhere: Alpine's
/// busybox is built without the `httpd` applet, which was measured rather than
/// assumed — `httpd: applet not found`.
pub const IMAGE: &str = "nginx:alpine";

/// Where the rendered page is mounted inside the container.
const DOCUMENT_ROOT: &str = "/usr/share/nginx/html";

/// One row on the page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    /// The full `https://…` address, ready to be a link.
    pub url: String,
    /// Only projects have one; a service is just on or off.
    pub note: Option<String>,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub running: bool,
    pub container: String,
    /// The address the page answers on, which is the workspace suffix itself.
    pub url: String,
    /// When the page was last rendered, as an RFC 3339 string; `None` if it has
    /// never been written.
    pub rendered: Option<String>,
    pub projects: usize,
    pub services: usize,
}

/// Where the rendered page lives on the host.
pub fn document_root(root: &std::path::Path) -> std::path::PathBuf {
    root.join("generated").join("landing")
}

/// The `docker run` invocation.
///
/// Arguments rather than an execution, for the same reason the tunnel's are:
/// the first start pulls an image and that belongs in the operation console.
pub fn run_args(host_dir: &str, domain: &str, network: &str) -> Vec<String> {
    let router = "stackvo-landing";
    [
        "run",
        "-d",
        "--rm",
        "--name",
        &format!("stackvo-{ID}"),
        "--network",
        network,
        // Read-only, because nothing in that container has any business writing
        // to a directory inside the workspace.
        "-v",
        &format!("{host_dir}:{DOCUMENT_ROOT}:ro"),
        "--label",
        "traefik.enable=true",
        "--label",
        &format!("traefik.http.routers.{router}.rule=Host(`{domain}`)"),
        "--label",
        &format!("traefik.http.routers.{router}.entrypoints=websecure"),
        "--label",
        &format!("traefik.http.routers.{router}.tls=true"),
        "--label",
        &format!("traefik.http.services.{router}.loadbalancer.server.port=80"),
        IMAGE,
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// HTML-escape. Five characters, because a project directory can be named
/// anything a file system accepts and one of them is `<`.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// The page itself.
///
/// One file, no scripts, no fonts and no requests: it is served to a browser
/// from a container inside the stack network, and anything it fetched would be
/// a second thing that can fail on a page whose entire job is to still work
/// when something else is broken.
pub fn render_html(suffix: &str, when: &str, projects: &[Entry], services: &[Entry]) -> String {
    let mut out = String::new();
    out.push_str(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
    );
    out.push_str(&format!("<title>{}</title>\n", escape(suffix)));
    out.push_str(STYLE);
    out.push_str("</head>\n<body>\n<main>\n");
    out.push_str(&format!("<h1>{}</h1>\n", escape(suffix)));

    section(&mut out, "Projects", projects, "Nothing here yet.");
    section(&mut out, "Services", services, "None are switched on.");

    // Said plainly rather than dressed as a timestamp nobody reads: this page
    // is a snapshot, and a stale snapshot that looks live is worse than one
    // that admits it.
    out.push_str(&format!(
        "<footer>Written {}. This page does not update itself — refresh it from StackVo after starting or stopping something.</footer>\n",
        escape(when)
    ));
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

fn section(out: &mut String, title: &str, entries: &[Entry], empty: &str) {
    out.push_str(&format!("<h2>{title}</h2>\n"));
    if entries.is_empty() {
        out.push_str(&format!("<p class=\"empty\">{empty}</p>\n"));
        return;
    }
    out.push_str("<ul>\n");
    for entry in entries {
        out.push_str(&format!(
            "<li><a href=\"{url}\"><span class=\"dot {state}\"></span><span class=\"name\">{name}</span><span class=\"host\">{host}</span></a>{note}</li>\n",
            url = escape(&entry.url),
            state = if entry.running { "up" } else { "down" },
            name = escape(&entry.name),
            host = escape(entry.url.trim_start_matches("https://")),
            note = entry
                .note
                .as_deref()
                .map(|n| format!("<span class=\"note\">{}</span>", escape(n)))
                .unwrap_or_default(),
        ));
    }
    out.push_str("</ul>\n");
}

const STYLE: &str = r#"<style>
:root { color-scheme: light dark; --fg: #1a1a1a; --dim: #6b6b6b; --bg: #fbfbfb; --card: #ffffff; --line: #e4e4e4; }
@media (prefers-color-scheme: dark) {
  :root { --fg: #e8e8e8; --dim: #9a9a9a; --bg: #161616; --card: #1f1f1f; --line: #2e2e2e; }
}
* { box-sizing: border-box; }
body { margin: 0; padding: 3rem 1.25rem; background: var(--bg); color: var(--fg);
       font: 15px/1.5 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; }
main { max-width: 44rem; margin: 0 auto; }
h1 { font-size: 1.6rem; margin: 0 0 2rem; font-weight: 600; }
h2 { font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.08em;
     color: var(--dim); margin: 2rem 0 0.75rem; font-weight: 600; }
ul { list-style: none; margin: 0; padding: 0; border: 1px solid var(--line);
     border-radius: 10px; overflow: hidden; background: var(--card); }
li + li { border-top: 1px solid var(--line); }
a { display: flex; align-items: center; gap: 0.75rem; padding: 0.85rem 1rem;
    color: inherit; text-decoration: none; }
a:hover { background: rgba(128,128,128,0.08); }
.dot { width: 8px; height: 8px; border-radius: 50%; flex: 0 0 auto; }
.dot.up { background: #2e9e4f; }
.dot.down { background: #b8b8b8; }
.name { font-weight: 500; }
.host { color: var(--dim); font-size: 0.85rem; margin-left: auto; }
.note { display: block; padding: 0 1rem 0.85rem 2.6rem; color: var(--dim); font-size: 0.85rem; }
.empty { color: var(--dim); }
footer { margin-top: 2.5rem; color: var(--dim); font-size: 0.8rem; }
</style>
"#;

/// Write the page, creating the directory if it is not there.
pub fn write(root: &std::path::Path, html: &str) -> Result<std::path::PathBuf> {
    let dir = document_root(root);
    std::fs::create_dir_all(&dir).map_err(|e| Error::io(format!("making {}", dir.display()), e))?;
    let path = dir.join("index.html");
    crate::atomic::write(&path, html)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, running: bool) -> Entry {
        Entry {
            name: name.to_string(),
            url: format!("https://{name}.loc"),
            note: None,
            running,
        }
    }

    /// A project directory can be called anything the file system accepts, and
    /// the name reaches this page as text. `<img onerror>` in a directory name
    /// is a stretch; `R&D` is not, and both break the same way.
    #[test]
    fn a_name_cannot_become_markup() {
        let hostile = Entry {
            name: "<script>alert(1)</script>".to_string(),
            url: "https://x.loc\" onload=\"alert(1)".to_string(),
            note: Some("R&D".to_string()),
            running: true,
        };
        let html = render_html("stackvo.loc", "now", &[hostile], &[]);
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("onload=\"alert"));
        assert!(html.contains("R&amp;D"));
    }

    /// An empty workspace still renders a page. The alternative — refusing to
    /// write one — is a bookmark that 404s on the day somebody sets up.
    #[test]
    fn an_empty_workspace_still_gets_a_page() {
        let html = render_html("stackvo.loc", "now", &[], &[]);
        assert!(html.contains("<h1>stackvo.loc</h1>"));
        assert!(html.contains("Nothing here yet."));
        assert!(html.contains("None are switched on."));
    }

    /// Nothing on the page is fetched. A landing page that needs the network to
    /// render is a landing page that is blank exactly when something is wrong.
    #[test]
    fn the_page_asks_for_nothing_else() {
        let html = render_html("stackvo.loc", "now", &[entry("shop", true)], &[]);
        for forbidden in ["<script", "<link", "@import", "src=", "//fonts", "http://"] {
            assert!(!html.contains(forbidden), "the page pulls in {forbidden}");
        }
        // Its own links are the exception, and they are https.
        assert!(html.contains("https://shop.loc"));
    }

    /// The running state is on every row, because "why is this one 502ing" is
    /// the question the page exists to answer without a second window.
    #[test]
    fn every_row_says_whether_it_is_up() {
        let html = render_html(
            "stackvo.loc",
            "now",
            &[entry("up", true), entry("down", false)],
            &[],
        );
        assert!(html.contains("dot up"));
        assert!(html.contains("dot down"));
    }

    /// The labels are what puts this on the name the stack already claims.
    /// Traefik's provider runs with `exposedByDefault: false`, so a missing
    /// `traefik.enable` label is a container that starts and answers nothing.
    #[test]
    fn the_run_arguments_route_the_bare_suffix() {
        let args = run_args("/w/generated/landing", "stackvo.loc", "stackvo-net");
        let joined = args.join(" ");
        assert!(joined.contains("traefik.enable=true"));
        assert!(joined.contains("rule=Host(`stackvo.loc`)"));
        assert!(joined.contains("entrypoints=websecure"));
        assert!(joined.contains("loadbalancer.server.port=80"));
        assert!(joined.contains("--network stackvo-net"));
        // Read-only: nothing in that container writes into the workspace.
        assert!(joined.contains("/w/generated/landing:/usr/share/nginx/html:ro"));
        assert!(joined.contains("--rm"), "stopping has to be removal");
    }
}
