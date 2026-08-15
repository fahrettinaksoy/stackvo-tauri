//! The one link `dns_probe` cannot reach: does the **machine** ask us?
//!
//! E-1. `dns_probe` asks the responder with `dig @127.0.0.1 -p 15353`, which
//! proves the replies are DNS and proves nothing about whether anything on this
//! machine would ever send that query. The step between the two is a file only
//! root can write, so it cannot live in a unit test — and it is exactly the
//! step where "the file was written" and "names resolve" come apart.
//!
//! ```sh
//! cargo run --example dns_machine_probe
//! ```
//!
//! **It asks for a password and puts the machine back.** The responder is
//! started, the resolver file is written through the same elevated path the app
//! uses, the machine is asked three ways, and the file is removed again. A
//! machine that *already* has this set up is left alone entirely — see the
//! first lines of `main`, which refuse rather than take somebody's working
//! configuration away as a side effect of a measurement.
//!
//! ## What it measured here, on macOS 15
//!
//! ```text
//!                     before        after install    after remove
//!  responder (udp)    ok            ok               ok
//!  responder (tcp)    ok            ok               ok
//!  the machine        no            ok               no
//!  example.com        ok            ok               ok
//!  dscacheutil        —             127.0.0.1, ::1   —
//! ```
//!
//! `dig shop.loc` is empty in all three columns, and that is not a failure:
//! `dig` talks to the servers in `resolv.conf` itself and never consults
//! `/etc/resolver`. `dscacheutil` and `getaddrinfo` go through mDNSResponder,
//! which is the path a browser takes — so those are the instruments, and `dig`
//! is only useful here pointed straight at the port.

use stackvo_desktop_lib::dns;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const SUFFIX: &str = "stackvo.loc";

fn system_dig(name: &str) -> String {
    let out = Command::new("dig")
        .args(["+short", "+time=2", "+tries=1", name, "A"])
        .output();
    match out {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Err(e) => format!("dig failed: {e}"),
    }
}

fn dscache(name: &str) -> String {
    let out = Command::new("dscacheutil")
        .args(["-q", "host", "-a", "name", name])
        .output();
    match out {
        Ok(out) => String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| l.starts_with("ip_address") || l.starts_with("ipv6_address"))
            .collect::<Vec<_>>()
            .join(", "),
        Err(e) => format!("dscacheutil failed: {e}"),
    }
}

fn report(stage: &str) {
    let check = dns::check(SUFFIX);
    println!("\n[{stage}]");
    println!("  configured   {}", dns::configured(SUFFIX));
    for (label, probe) in [
        ("udp   ", &check.udp),
        ("tcp   ", &check.tcp),
        ("system", &check.system),
        ("public", &check.public),
    ] {
        println!(
            "  {label}       {}  {}",
            if probe.ok { "ok" } else { "no" },
            probe.detail
        );
    }
    println!("  dig shop.loc      -> {:?}", system_dig("shop.loc"));
    println!("  dig a.b.shop.loc  -> {:?}", system_dig("a.b.shop.loc"));
    println!("  dscacheutil       -> {:?}", dscache("shop.loc"));
    println!("  dig example.com   -> {:?}", system_dig("example.com"));
}

fn main() {
    let plan = dns::plan(SUFFIX).expect("a plan for this machine");
    println!("mechanism {:?}", plan.mechanism);
    println!("file      {:?}", plan.file);
    println!("text      {:?}", plan.text);

    // A machine that is already pointed at a responder has nothing to prove
    // here, and this probe ends by removing what it wrote — which on that
    // machine would mean taking away a working configuration to measure it.
    if dns::configured(SUFFIX) {
        println!(
            "\nthis machine already asks a responder for .{}, so nothing was touched.",
            dns::tld_of(SUFFIX).unwrap_or_default()
        );
        println!(
            "turn it off in Settings → Local DNS first if you want to measure the whole path."
        );
        return;
    }

    let udp = dns::bind().expect("udp");
    let tcp = dns::bind_tcp().expect("tcp");
    let stop = Arc::new(AtomicBool::new(false));
    let workers = vec![
        {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || dns::serve(udp, SUFFIX.to_string(), stop))
        },
        {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || dns::serve_tcp(tcp, SUFFIX.to_string(), stop))
        },
    ];

    report("before");

    println!("\ninstalling — a password panel is about to appear");
    match dns::install(SUFFIX) {
        Ok(()) => println!("install: ok (it verified through the machine's own resolver)"),
        Err(e) => println!("install: FAILED {:?} — {}", e.code, e.message),
    }

    report("after install");

    println!("\nremoving");
    match dns::remove(SUFFIX) {
        Ok(()) => println!("remove: ok"),
        Err(e) => println!("remove: FAILED {:?} — {}", e.code, e.message),
    }
    // The resolver is dropped by the system as soon as the file goes, but the
    // 60-second answers it already handed out may still be cached.
    std::thread::sleep(std::time::Duration::from_millis(1500));

    report("after remove");

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    for worker in workers {
        let _ = worker.join();
    }
}
