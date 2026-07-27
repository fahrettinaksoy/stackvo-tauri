//! Headless end-to-end check of the Phase 1 command surface.
//!
//! Runs every read-only command against the real machine and prints what the
//! UI would render. Useful in two ways: it verifies the port without opening a
//! window, and it is a genuine troubleshooting tool — "what does StackVo
//! actually see?" answered without the GUI in the way.
//!
//!   cargo run --example diagnose
//!   STACKVO_ROOT=/path/to/stackvo cargo run --example diagnose

use stackvo_desktop_lib::{commands, engine, stats::Sampler, workspace};

fn bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if value == 0 {
        return "0 B".into();
    }
    let mut scaled = value as f64;
    let mut unit = 0;
    while scaled >= 1024.0 && unit < UNITS.len() - 1 {
        scaled /= 1024.0;
        unit += 1;
    }
    format!("{scaled:.1} {}", UNITS[unit])
}

fn heading(title: &str) {
    println!("\n\x1b[1m{title}\x1b[0m");
    println!("{}", "─".repeat(title.len()));
}

#[tokio::main]
async fn main() {
    println!("\x1b[1mStackVo Desktop — diagnose\x1b[0m");

    // ---- workspace -------------------------------------------------------
    heading("Workspace");
    let ws = workspace::resolve();
    match (&ws.root, ws.valid) {
        (Some(root), true) => {
            println!("  root      {root}");
            println!("  source    {:?}", ws.source);
            println!(
                "  version   {}",
                ws.stackvo_version.as_deref().unwrap_or("—")
            );
        }
        (Some(root), false) => {
            println!("  \x1b[33mstale\x1b[0m     {root} (no longer a StackVo checkout)");
            return;
        }
        _ => {
            println!("  \x1b[33mnot found\x1b[0m — set STACKVO_ROOT or pick a folder in the app");
            return;
        }
    }
    let root = ws.require_root().expect("validated above");

    // ---- engine ----------------------------------------------------------
    heading("Docker engine");
    let status = engine::status().await;
    if status.reachable {
        println!("  \x1b[32mreachable\x1b[0m");
        println!("  platform  {:?}", status.platform);
        println!("  version   {}", status.version.as_deref().unwrap_or("—"));
        println!(
            "  api       {}",
            status.api_version.as_deref().unwrap_or("—")
        );
        println!("  context   {}", status.context.as_deref().unwrap_or("—"));
        println!(
            "  socket    {}",
            status.socket_path.as_deref().unwrap_or("—")
        );
    } else {
        // Not a failure: the app renders fine in this state, which is the
        // whole point of moving off a containerised UI.
        println!(
            "  \x1b[33mnot running\x1b[0m — {}",
            status.error.as_deref().unwrap_or("?")
        );
        println!(
            "  socket    {}",
            status.socket_path.as_deref().unwrap_or("—")
        );
    }

    // ---- host metrics ----------------------------------------------------
    heading("Host metrics");
    let mut sampler = Sampler::new();
    sampler.sample(); // prime the CPU/network deltas
    std::thread::sleep(std::time::Duration::from_millis(500));
    let s = sampler.sample();

    println!("  host      {}", s.host_name.as_deref().unwrap_or("—"));
    println!("  os        {}", s.os.as_deref().unwrap_or("—"));
    println!(
        "  cpu       {:.1}% over {} cores",
        s.cpu.percent, s.cpu.core_count
    );
    if let Some(load) = s.cpu.load_average {
        println!("  load      {:.2} {:.2} {:.2}", load[0], load[1], load[2]);
    }
    println!(
        "  memory    {} / {} ({:.1}%)",
        bytes(s.memory.used),
        bytes(s.memory.total),
        s.memory.percent
    );
    println!(
        "  storage   {} / {} ({:.1}%) on {}",
        bytes(s.storage.used),
        bytes(s.storage.total),
        s.storage.percent,
        s.storage.mount_point
    );
    println!(
        "  network   ↓ {}/s  ↑ {}/s",
        bytes(s.network.rx_rate as u64),
        bytes(s.network.tx_rate as u64)
    );
    println!(
        "  disk      ↓ {}/s  ↑ {}/s",
        bytes(s.disk.read_rate as u64),
        bytes(s.disk.write_rate as u64)
    );
    match s.cpu.breakdown {
        Some(b) => println!(
            "  cpu split user {:.1}%  nice {:.1}%  system {:.1}%  idle {:.1}%",
            b.user, b.nice, b.system, b.idle
        ),
        // The counters are cumulative; one reading cannot produce a split.
        None => println!("  cpu split (needs a second sample)"),
    }

    // ---- docker inventory ------------------------------------------------
    if status.reachable {
        heading("Docker inventory");
        match engine::system_resources().await {
            Ok(r) => {
                println!(
                    "  images    {} total, {} in use, {} unused, {}",
                    r.images.total,
                    r.images.in_use,
                    r.images.unused,
                    bytes(r.images.size)
                );
                println!(
                    "  volumes   {} total, {} in use, {} unused, {}",
                    r.volumes.total,
                    r.volumes.in_use,
                    r.volumes.unused,
                    bytes(r.volumes.size)
                );
            }
            Err(e) => println!("  \x1b[31m{e}\x1b[0m"),
        }
    }

    // ---- projects --------------------------------------------------------
    heading("Projects");
    match commands::list_projects(&root).await {
        Err(e) => println!("  \x1b[31m{e}\x1b[0m"),
        Ok(projects) => {
            let mut problem_count = 0;
            for p in &projects {
                let state = if p.running {
                    "\x1b[32mrunning\x1b[0m"
                } else if p.built {
                    "\x1b[33mstopped\x1b[0m"
                } else {
                    "\x1b[90mnot built\x1b[0m"
                };
                let flag = if p.manifest_valid {
                    " "
                } else {
                    "\x1b[31m!\x1b[0m"
                };

                println!(
                    "{flag} {:<28} {:<6} {:<24} {}",
                    p.name,
                    p.runtime,
                    p.domain.as_deref().unwrap_or("—"),
                    state
                );

                if p.domain.is_some() && !p.domain_configured {
                    println!(
                        "    \x1b[33m↳ no /etc/hosts entry — unreachable from a browser\x1b[0m"
                    );
                }
                for issue in &p.manifest.errors {
                    problem_count += 1;
                    println!(
                        "    \x1b[31m↳ {} {} — {}\x1b[0m",
                        issue.code, issue.path, issue.message
                    );
                }
                for issue in &p.manifest.warnings {
                    println!(
                        "    \x1b[90m↳ {} {} — {}\x1b[0m",
                        issue.code, issue.path, issue.message
                    );
                }
            }
            println!(
                "\n  {} projects, {} contract errors",
                projects.len(),
                problem_count
            );
        }
    }

    // ---- services --------------------------------------------------------
    heading("Services");
    match commands::list_services(&root).await {
        Err(e) => println!("  \x1b[31m{e}\x1b[0m"),
        Ok(services) => {
            let enabled: Vec<_> = services.iter().filter(|s| s.enabled).collect();
            for s in &enabled {
                let state = if s.running {
                    "\x1b[32mrunning\x1b[0m"
                } else {
                    "\x1b[33mstopped\x1b[0m"
                };
                println!(
                    "  {:<18} {:<10} {}",
                    s.id,
                    s.version.as_deref().unwrap_or("—"),
                    state
                );
                if !s.unmet_dependencies.is_empty() {
                    println!(
                        "    \x1b[33m↳ unmet: {}\x1b[0m",
                        s.unmet_dependencies.join(", ")
                    );
                }
            }
            println!(
                "\n  {} of {} services enabled",
                enabled.len(),
                services.len()
            );
        }
    }

    // ---- container detail ------------------------------------------------
    // Exercises the Phase 2 read paths — inspect, live stats and a log tail —
    // against a real container. The mutating paths (start/stop/build/compose)
    // are deliberately NOT run here: they would restart the user's stack.
    if status.reachable {
        if let Ok(containers) = engine::stackvo_containers().await {
            if let Some((id, _)) = containers.iter().find(|(_, c)| c.running) {
                heading(&format!("Container detail — {id}"));

                match engine::inspect(id).await {
                    Ok(d) => {
                        println!("  state     {}", d.state.as_deref().unwrap_or("—"));
                        println!("  image     {}", d.image.as_deref().unwrap_or("—"));
                        println!("  health    {}", d.health.as_deref().unwrap_or("—"));
                        println!("  networks  {}", d.networks.join(", "));
                        println!("  gateway   {}", d.gateway.as_deref().unwrap_or("—"));
                        println!("  created   {}", d.created.as_deref().unwrap_or("—"));
                        println!("  policy    {}", d.restart_policy.as_deref().unwrap_or("—"));
                        println!("  restarts  {}", d.restart_count);
                        println!(
                            "  img size  {}",
                            d.image_size.map(bytes).unwrap_or_else(|| "—".into())
                        );
                        println!("  env vars  {} (secrets redacted)", d.env.len());
                        let leaked: Vec<_> = d
                            .env
                            .iter()
                            .filter(|e| {
                                e.split_once('=').is_some_and(|(k, v)| {
                                    stackvo_desktop_lib::config::Env::is_secret(k)
                                        && !v.is_empty()
                                        && v != "••••••••"
                                })
                            })
                            .collect();
                        if leaked.is_empty() {
                            println!("  \x1b[32m✓ no secret values crossed the boundary\x1b[0m");
                        } else {
                            println!("  \x1b[31m✗ {} secrets leaked\x1b[0m", leaked.len());
                        }
                    }
                    Err(e) => println!("  \x1b[31m{e}\x1b[0m"),
                }

                match engine::container_stats(id).await {
                    Ok(st) => {
                        println!(
                            "  cpu       {:.1}% over {} cpus   memory {} / {} ({:.1}%)",
                            st.cpu_percent,
                            st.online_cpus,
                            bytes(st.memory_used),
                            bytes(st.memory_limit),
                            st.memory_percent
                        );
                        println!(
                            "  pids      {}   block R {} / W {}",
                            st.pids,
                            bytes(st.block_read),
                            bytes(st.block_write)
                        );
                    }
                    Err(e) => println!("  \x1b[31mstats: {e}\x1b[0m"),
                }

                // follow=false so the stream terminates on its own.
                match engine::logs_stream(id, 3, false) {
                    Ok(stream) => {
                        use futures_util::StreamExt;
                        futures_util::pin_mut!(stream);
                        let mut n = 0;
                        while let Some(line) = stream.next().await {
                            let trimmed: String = line.text.chars().take(90).collect();
                            println!("  log       \x1b[90m{trimmed}\x1b[0m");
                            n += 1;
                            if n >= 3 {
                                break;
                            }
                        }
                        if n == 0 {
                            println!("  log       (container has produced no output)");
                        }
                    }
                    Err(e) => println!("  \x1b[31mlogs: {e}\x1b[0m"),
                }
            }
        }
    }

    // ---- generator port --------------------------------------------------
    // The live differential, using the same code path the app exposes as
    // `generator_verify`. Nothing is written.
    heading("Generator port");
    {
        let report = commands::verify_generator(&root);
        match report {
            Err(e) => println!("  \x1b[31m{e}\x1b[0m"),
            Ok(report) => {
                for file in report["files"].as_array().unwrap_or(&vec![]) {
                    let name = file["file"].as_str().unwrap_or("?");
                    match file["status"].as_str().unwrap_or("?") {
                        "match" => println!("  \x1b[32m✓\x1b[0m {name}"),
                        "missing" => println!("  \x1b[90m·\x1b[0m {name} (not generated yet)"),
                        "differ" => println!(
                            "  \x1b[31m✗\x1b[0m {name} — differs at line {}",
                            file["firstDifferenceLine"]
                        ),
                        _ => println!("  \x1b[31m✗\x1b[0m {name} — {}", file["error"]),
                    }
                }
                println!(
                    "\n  {} match, {} differ — ready to take over: {}",
                    report["matched"], report["differed"], report["readyToTakeOver"]
                );
                for w in report["warnings"].as_array().unwrap_or(&vec![]) {
                    println!("  \x1b[33m⚠ {}\x1b[0m", w.as_str().unwrap_or(""));
                }
            }
        }
    }

    // ---- hosts -----------------------------------------------------------
    // Plans only. Nothing is written and no elevation prompt is raised — this
    // exists to prove the plan preserves the real file before anyone trusts
    // `hosts_apply` with it.
    heading("Hosts file");
    if let Ok(projects) = commands::list_projects(&root).await {
        let missing: Vec<String> = projects
            .iter()
            .filter(|p| p.domain.is_some() && !p.domain_configured)
            .filter_map(|p| p.domain.clone())
            .collect();

        if missing.is_empty() {
            println!("  \x1b[32mevery project domain is mapped\x1b[0m");
        } else {
            println!("  missing   {}", missing.join(", "));

            // Fallible now: a manifest can carry a domain the contract would
            // reject, and planning refuses rather than writing it.
            let plan = match stackvo_desktop_lib::hosts::plan(&missing, &[]) {
                Ok(plan) => plan,
                Err(e) => {
                    println!("  \x1b[31mcannot plan: {}\x1b[0m", e.message);
                    return;
                }
            };
            let before: Vec<&str> = plan.current.lines().collect();
            let after: Vec<&str> = plan.preview.lines().collect();

            let kept = before.iter().filter(|l| after.contains(l)).count();
            // Count the lines we actually list below, so the number and the
            // listing cannot disagree — a blank separator line is present in
            // both files and is neither added nor lost.
            let added = after.iter().filter(|l| !before.contains(l)).count();

            println!(
                "  plan      +{added} line(s), {kept}/{} existing lines kept",
                before.len()
            );

            if kept == before.len() {
                println!("  \x1b[32m✓ no existing line would be lost\x1b[0m");
            } else {
                let lost: Vec<&&str> = before.iter().filter(|l| !after.contains(l)).collect();
                println!("  \x1b[31m✗ would drop: {lost:?}\x1b[0m");
            }

            for line in after.iter().filter(|l| !before.contains(l)) {
                println!("  \x1b[32m+ {line}\x1b[0m");
            }
        }
    }

    // ---- catalog ---------------------------------------------------------
    heading("Catalog");
    match commands::build_catalog(&root) {
        Err(e) => println!("  \x1b[31m{e}\x1b[0m"),
        Ok(catalog) => {
            for r in &catalog.runtimes {
                let mark = if r.available {
                    "\x1b[32m✓\x1b[0m"
                } else {
                    "\x1b[31m✗\x1b[0m"
                };
                println!(
                    "  {mark} {:<8} default {:<8} ({} versions)",
                    r.id,
                    r.default.as_deref().unwrap_or("—"),
                    r.versions.len()
                );
            }
            println!("  servers   {}", catalog.servers.join(", "));
            println!(
                "  php ext   {} offered, cap {} per manifest",
                catalog.php_extensions.len(),
                catalog.max_extensions
            );
            let unavailable = catalog.runtimes.iter().filter(|r| !r.available).count();
            if unavailable > 0 {
                println!(
                    "  \x1b[33m↳ {unavailable} advertised runtimes have no generator (C-02)\x1b[0m"
                );
            }
        }
    }

    println!();
}
