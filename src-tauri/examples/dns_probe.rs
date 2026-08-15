//! Does the responder answer a *real* resolver, not just its own encoder?
//!
//! E-1. `dns.rs` has its own unit tests and every one of them builds the query
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
//!
//! ## What the first version of this file missed
//!
//! It looked for a `status:` line and called anything that had one a pass, and
//! every case passed. Above the status line `dig` was also printing
//!
//! ```text
//! ;; Warning: Message parser reports malformed message packet.
//! ```
//!
//! for every REFUSED and every NODATA — the header said "one question" over a
//! body that carried none. A lenient tool read it anyway; a stub resolver drops
//! what it cannot match against the query it sent, and a dropped reply is not a
//! fast failure, it is a five-second timeout. So the warning is now a failure
//! here, `+tcp` is measured because a resolver picks its own transport, and
//! `+edns` is measured because that is what a modern one actually sends.

use stackvo_desktop_lib::dns;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// One `dig` invocation and what it has to say.
struct Case {
    name: &'static str,
    kind: &'static str,
    /// Extra `dig` flags — the transport and EDNS cases.
    flags: &'static [&'static str],
    expect_status: &'static str,
    expect_answer: &'static str,
}

fn main() {
    let udp = match dns::bind() {
        Ok(socket) => socket,
        Err(e) => {
            println!("could not bind: {}", e.message);
            println!("something else is on 127.0.0.1:{}", dns::PORT);
            return;
        }
    };
    let tcp = dns::bind_tcp().ok();
    if tcp.is_none() {
        println!("warning: tcp/{} could not be bound", dns::PORT);
    }

    let stop = Arc::new(AtomicBool::new(false));
    let mut workers = vec![{
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || dns::serve(udp, "stackvo.loc".to_string(), stop))
    }];
    if let Some(listener) = tcp {
        let stop = Arc::clone(&stop);
        workers.push(std::thread::spawn(move || {
            dns::serve_tcp(listener, "stackvo.loc".to_string(), stop)
        }));
    }

    println!("responder on 127.0.0.1:{}, serving .loc\n", dns::PORT);

    // The three answers the responder has — an address, no data, a refusal —
    // each asked the three ways something might ask: plain UDP, EDNS (what a
    // modern stub sends), and TCP (what a retry arrives on).
    let cases = [
        Case {
            name: "shop.loc",
            kind: "A",
            flags: &["+noedns"],
            expect_status: "NOERROR",
            expect_answer: "127.0.0.1",
        },
        Case {
            name: "a.b.deep.loc",
            kind: "A",
            flags: &["+noedns"],
            expect_status: "NOERROR",
            expect_answer: "127.0.0.1",
        },
        Case {
            name: "shop.loc",
            kind: "AAAA",
            flags: &["+noedns"],
            expect_status: "NOERROR",
            expect_answer: "::1",
        },
        // Type 65. Every Chrome and Safari page load asks this before it asks
        // for an address, so this NODATA is on the hot path, not an oddity.
        Case {
            name: "shop.loc",
            kind: "TYPE65",
            flags: &["+noedns"],
            expect_status: "NOERROR",
            expect_answer: "—",
        },
        Case {
            name: "shop.loc",
            kind: "MX",
            flags: &["+noedns"],
            expect_status: "NOERROR",
            expect_answer: "—",
        },
        Case {
            name: "google.com",
            kind: "A",
            flags: &["+noedns"],
            expect_status: "REFUSED",
            expect_answer: "—",
        },
        Case {
            name: "shop.loc",
            kind: "A",
            flags: &["+edns=0"],
            expect_status: "NOERROR",
            expect_answer: "127.0.0.1",
        },
        Case {
            name: "google.com",
            kind: "A",
            flags: &["+edns=0"],
            expect_status: "REFUSED",
            expect_answer: "—",
        },
        Case {
            name: "shop.loc",
            kind: "A",
            flags: &["+tcp", "+noedns"],
            expect_status: "NOERROR",
            expect_answer: "127.0.0.1",
        },
        Case {
            name: "deep.nested.shop.loc",
            kind: "A",
            flags: &["+tcp", "+edns=0"],
            expect_status: "NOERROR",
            expect_answer: "127.0.0.1",
        },
    ];

    let mut failures = 0;
    for case in &cases {
        let mut args = vec![
            "+time=2".to_string(),
            "+tries=1".to_string(),
            "@127.0.0.1".to_string(),
            "-p".to_string(),
            dns::PORT.to_string(),
        ];
        args.extend(case.flags.iter().map(|f| f.to_string()));
        args.push(case.name.to_string());
        args.push(case.kind.to_string());

        let Ok(output) = Command::new("dig").args(&args).output() else {
            println!("dig is not on this machine — nothing was measured");
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
            for worker in workers {
                let _ = worker.join();
            }
            return;
        };

        let text = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
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

        // The line that used to be printed and ignored. A malformed reply is a
        // reply a stub resolver drops, which reads to a user as a name that
        // takes five seconds to fail.
        let malformed = text.contains("malformed") || stderr.contains("malformed");

        let answer = if answer.is_empty() {
            "—".to_string()
        } else {
            answer
        };
        let ok = status == case.expect_status && answer == case.expect_answer && !malformed;
        if !ok {
            failures += 1;
        }

        println!(
            "  {} {:<20} {:<5} {:<12} status={:<9} answer={:<10} expected {} / {}",
            if ok { "ok  " } else { "FAIL" },
            case.name,
            case.kind,
            case.flags.join(" "),
            status,
            answer,
            case.expect_status,
            case.expect_answer,
        );
        if malformed {
            println!("       dig calls this reply malformed");
        }
    }

    // The app's own self-test, run against the same live responder. This is
    // what the DNS pane's "Test it" button shows, and the interesting line is
    // the third: the first two ask a socket this process owns, and the third
    // asks the machine. They disagree on any machine where the resolver file
    // has not been written — which is most of them, and is exactly the
    // distinction a status built out of "the file exists" cannot draw.
    println!("\nthe self-test, as the pane runs it:");
    let check = dns::check("stackvo.loc");
    println!("  name    {}", check.name);
    for (label, probe) in [
        ("udp   ", &check.udp),
        ("tcp   ", &check.tcp),
        ("system", &check.system),
        ("public", &check.public),
    ] {
        println!(
            "  {label}  {}  {}",
            if probe.ok { "ok  " } else { "no  " },
            probe.detail
        );
    }
    println!(
        "  verdict {}",
        if check.ok {
            "this machine resolves the suffix"
        } else {
            "the responder and the machine do not agree yet"
        }
    );

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    for worker in workers {
        let _ = worker.join();
    }

    println!();
    if failures == 0 {
        println!(
            "{} of {} replies are what dig expected.",
            cases.len(),
            cases.len()
        );
    } else {
        println!(
            "{failures} of {} replies were wrong — the table above is the evidence, not the summary.",
            cases.len()
        );
    }
}
