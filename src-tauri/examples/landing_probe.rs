//! Does the name the stack already claims actually serve the page? (M-4)
//!
//! `landing.rs` is unit-tested on the two things a string can be checked for:
//! that a project name cannot become markup, and that the `docker run`
//! arguments carry the labels Traefik needs. Neither says whether **Traefik
//! picks the container up**, which is the whole feature and which depends on
//! things no test in this repository can see — the provider's
//! `exposedByDefault`, the network it watches, the certificate the router
//! serves, and whether the bare suffix resolves on this machine at all.
//!
//! So this starts it for real and asks:
//!
//! ```sh
//! cargo run --example landing_probe
//! ```
//!
//! It renders a page into a scratch directory, runs `stackvo-landing` against
//! the live stack network, requests `https://<suffix>` over HTTPS, and reports
//! the status line and whether the page it got back is the one that was
//! written. **It removes the container and the scratch directory on the way
//! out, whatever happened**, and it never touches the workspace's own
//! `generated/landing`.

use stackvo_desktop_lib::landing;
use std::process::Command;

fn main() {
    let suffix = "stackvo.loc";
    let network = "stackvo-net";
    let scratch =
        std::env::temp_dir().join(format!("stackvo-landing-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    if std::fs::create_dir_all(&scratch).is_err() {
        println!("could not make a scratch directory");
        return;
    }

    let entries = vec![
        landing::Entry {
            name: "shop".into(),
            url: format!("https://shop.{suffix}"),
            note: None,
            running: true,
        },
        landing::Entry {
            name: "blog".into(),
            url: format!("https://blog.{suffix}"),
            note: Some("This project's manifest has errors.".into()),
            running: false,
        },
    ];
    let html = landing::render_html(suffix, "2026-08-16T00:00:00Z", &entries, &[]);
    if std::fs::write(scratch.join("index.html"), &html).is_err() {
        println!("could not write the page");
        return;
    }
    println!("  page rendered, {} bytes", html.len());

    // Whatever is there from a previous run, gone before this one starts.
    let _ = docker(&["rm", "-f", "stackvo-landing"]);

    let args = landing::run_args(&scratch.display().to_string(), suffix, network);
    match docker(&args.iter().map(String::as_str).collect::<Vec<_>>()) {
        Some(id) => println!("  container started, {}", &id[..12.min(id.len())]),
        None => {
            println!("  the container did not start — is the stack up?");
            let _ = std::fs::remove_dir_all(&scratch);
            return;
        }
    }

    // Traefik's docker provider polls; the first request can beat it.
    let mut answer = None;
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if let Some(body) = fetch(&format!("https://{suffix}/")) {
            answer = Some(body);
            break;
        }
    }

    let _ = docker(&["rm", "-f", "stackvo-landing"]);
    let _ = std::fs::remove_dir_all(&scratch);

    match answer {
        Some(body) => {
            let same = body.contains("<h1>stackvo.loc</h1>")
                && body.contains(&format!("https://shop.{suffix}"))
                && body.contains("dot down");
            println!(
                "  {} https://{suffix} answered with {} bytes — {}",
                if same { "ok  " } else { "FAIL" },
                body.len(),
                if same {
                    "the page that was written"
                } else {
                    "something else"
                }
            );
            println!();
            if same {
                println!("the name the stack already claimed now serves the list.");
            } else {
                println!("something answered, but it was not this page.");
            }
        }
        None => {
            println!("  FAIL nothing answered on https://{suffix}");
            println!();
            println!("the router never came up. Nothing was left behind.");
        }
    }
}

fn docker(args: &[&str]) -> Option<String> {
    let out = Command::new("docker").args(args).output().ok()?;
    if !out.status.success() {
        let text = String::from_utf8_lossy(&out.stderr);
        let text = text.trim();
        if !text.is_empty() && !text.contains("No such container") {
            eprintln!("       docker: {text}");
        }
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The local CA is trusted by the system store, so no flag is needed to skip
/// verification — and none is passed, deliberately. A page served under a
/// certificate the machine does not trust is not the feature working.
fn fetch(url: &str) -> Option<String> {
    let out = Command::new("curl")
        .args(["-sS", "--max-time", "5", url])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let body = String::from_utf8_lossy(&out.stdout).to_string();
    if body.is_empty() {
        None
    } else {
        Some(body)
    }
}
