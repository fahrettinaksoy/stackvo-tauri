//! How far can the Stripe listener be checked without a Stripe account? (M-11)
//!
//! This is the item on the list whose stated cost was "depends on the Stripe
//! CLI and an account; it cannot be verified on this machine". Half of that is
//! true and half of it was an excuse, and this probe is the line between them.
//!
//! **What it verifies for real:**
//!
//! * the image runs and the CLI is the program inside it;
//! * the arguments `stripe.rs` builds are arguments that CLI accepts;
//! * a wrong key produces a complaint that `find_failure` recognises, so the
//!   app reports "that key was rejected" rather than sitting on a blank pane;
//! * nothing is left behind.
//!
//! **What it cannot verify, and does not pretend to:** that a real event
//! arrives at the application. That needs an account, a key and a payment in
//! test mode, and no assertion written here can stand in for it.
//!
//! ```sh
//! cargo run --example stripe_probe
//! ```
//!
//! The key used is the literal string `sk_test_invalid_probe_key`, which is not
//! a credential and is not valid anywhere. Nothing on this machine is read.

use stackvo_desktop_lib::stripe;
use std::process::Command;

fn main() {
    let container = "stackvo-stripe-probe";
    let _ = docker(&["rm", "-f", container]);

    // The real argument list, from the real function, with the name changed so
    // it cannot collide with a listener somebody is actually using.
    let mut args = stripe::run_args("probe", 80, "/stripe/webhook", &[], "bridge");
    for arg in args.iter_mut() {
        if arg == "stackvo-stripe-probe" {
            *arg = container.to_string();
        }
    }
    println!("  argv: docker {}", args.join(" "));
    // Said out loud, because it is the point of passing it through the
    // environment: the line above is what the operation console shows.
    assert!(
        !args.iter().any(|a| a.starts_with("sk_")),
        "the key reached the argument list"
    );
    println!("  the credential is not in that line — it goes through -e");

    let started = Command::new("docker")
        .args(&args)
        .env("STRIPE_API_KEY", "sk_test_invalid_probe_key")
        .output();

    let ok = matches!(&started, Ok(out) if out.status.success());
    if !ok {
        if let Ok(out) = &started {
            println!(
                "  FAIL the container did not start: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        } else {
            println!("  FAIL docker is not reachable");
        }
        return;
    }
    println!("  container started");

    // The CLI authenticates on start, so its verdict lands in the first second
    // or two. Read for longer than that before concluding anything.
    //
    // This is readable at all because the sidecar is not started with `--rm`.
    // The first version of this probe found that the hard way: an invalid key
    // makes the CLI exit, `--rm` removed the container with its log, and
    // `docker logs` then answered "No such container" — which `find_failure`
    // matched, because it begins with "Error". A probe reporting success on
    // Docker's own error message is worse than no probe, and it is the reason
    // `run_args` no longer passes `--rm`.
    let mut log = String::new();
    for _ in 0..15 {
        std::thread::sleep(std::time::Duration::from_millis(600));
        log = docker(&["logs", container]).unwrap_or_default();
        if stripe::find_failure(&log).is_some() || stripe::find_secret(&log).is_some() {
            break;
        }
    }

    let _ = docker(&["rm", "-f", container]);

    println!();
    match (stripe::find_secret(&log), stripe::find_failure(&log)) {
        (Some(secret), _) => {
            // Only reachable with a working key in the environment, which this
            // probe does not supply — so if it happens, say so plainly.
            println!("  a signing secret was printed and read back: {secret}");
            println!("the listener works end to end on this machine.");
        }
        (None, Some(failure)) => {
            println!("  the CLI rejected the invalid key, and the reader saw it:");
            println!("    {failure}");
            println!();
            println!("the image, the arguments and the log reader are verified.");
            println!("that a real event reaches the application is NOT — that needs an account.");
        }
        (None, None) => {
            println!("  the CLI said neither. What it did say:");
            for line in log.lines().take(8) {
                println!("    {line}");
            }
            println!();
            println!("nothing was verified; find_failure does not recognise this build's wording.");
        }
    }
}

fn docker(args: &[&str]) -> Option<String> {
    let out = Command::new("docker").args(args).output().ok()?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    // The CLI writes to stderr, which is where a complaint about a key lands.
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    if !out.status.success() && text.trim().is_empty() {
        return None;
    }
    Some(text.trim().to_string())
}
