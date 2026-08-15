//! Does the responder answer a *real* resolver, not just its own encoder?
//!
//! E-1. `dns.rs` has eighteen unit tests and every one of them builds the query
//! with the same code that reads it back. That proves the module is
//! self-consistent and proves nothing about whether `dig`, `getaddrinfo` or a
//! browser would accept a word of it — which is the only question that matters,
//! because a DNS reply that is subtly wrong is not rejected loudly. It is
//! ignored, and the name simply does not resolve.
//!
//! So this binds the responder, asks it with the system's own `dig`, and prints
//! what came back. Run it with:
//!
//! ```sh
//! cargo run --example dns_probe
//! ```

use stackvo_desktop_lib::dns;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn main() {
    let socket = match dns::bind() {
        Ok(socket) => socket,
        Err(e) => {
            println!("could not bind: {}", e.message);
            println!("something else is on 127.0.0.1:{}", dns::PORT);
            return;
        }
    };

    let stop = Arc::new(AtomicBool::new(false));
    let worker = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || dns::serve(socket, "stackvo.loc".to_string(), stop))
    };

    println!("responder on 127.0.0.1:{}, serving .loc\n", dns::PORT);

    // Names chosen to cover the three answers the responder has: a plain name,
    // a name several labels deep (the wildcard E-2 wanted), and a name outside
    // the suffix, which must be refused rather than answered with anything.
    let cases: [(&str, &str, &str); 5] = [
        ("shop.loc", "A", "127.0.0.1"),
        ("a.b.deep.loc", "A", "127.0.0.1 (wildcard)"),
        ("shop.loc", "AAAA", "::1"),
        ("shop.loc", "MX", "no answer, NOERROR"),
        ("google.com", "A", "REFUSED"),
    ];

    let mut failures = 0;
    for (name, kind, expected) in cases {
        let output = Command::new("dig")
            .args([
                "+time=2",
                "+tries=1",
                "@127.0.0.1",
                "-p",
                &dns::PORT.to_string(),
                name,
                kind,
            ])
            .output();

        let Ok(output) = output else {
            println!("dig is not on this machine — nothing was measured");
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = worker.join();
            return;
        };

        let text = String::from_utf8_lossy(&output.stdout);
        let status = text
            .lines()
            .find(|line| line.contains("status:"))
            .and_then(|line| {
                line.split("status: ")
                    .nth(1)
                    .and_then(|rest| rest.split(',').next())
            })
            .unwrap_or("?")
            .to_string();

        let answer = text
            .lines()
            .skip_while(|line| !line.starts_with(";; ANSWER SECTION"))
            .nth(1)
            .map(|line| line.split_whitespace().last().unwrap_or("").to_string())
            .unwrap_or_default();

        // `dig` printing a status at all is the whole point: it means the bytes
        // parsed as a DNS message by something this code did not write.
        let ok = status != "?";
        if !ok {
            failures += 1;
        }
        println!(
            "  {:<14} {:<5} status={:<9} answer={:<10} expected {}",
            name,
            kind,
            status,
            if answer.is_empty() { "—" } else { &answer },
            expected
        );
    }

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = worker.join();

    println!();
    if failures == 0 {
        println!("every reply parsed as DNS by dig.");
    } else {
        println!("{failures} of 5 replies were not readable as DNS — the table above is the evidence, not the summary.");
    }
}
