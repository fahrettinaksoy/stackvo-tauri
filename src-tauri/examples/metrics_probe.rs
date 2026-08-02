//! Print what the dashboard would draw, next to what the OS says.
//!
//! Every number on that screen is a percentage of something, and a percentage
//! is exactly the kind of value that looks right while being wrong. This prints
//! the raw inputs so they can be checked against `top`, `vm_stat` and `df`
//! rather than against each other.
//!
//! ```sh
//! cargo run --example metrics_probe
//! ```

use stackvo_desktop_lib::stats::Sampler;

fn gb(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0 / 1024.0
}

fn main() {
    let mut sampler = Sampler::new();

    // The first sample has no previous one: no rates, and the CPU-time window
    // has only just opened.
    let _ = sampler.sample();

    for round in 1..=3 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let s = sampler.sample();

        println!("\n──────── sample {round} ────────");

        print!("cpu      global {:.1}%", s.cpu.percent);
        match s.cpu.breakdown {
            Some(b) => println!(
                "   breakdown busy {:.1}%  (user {:.1} nice {:.1} system {:.1} idle {:.1})",
                100.0 - b.idle,
                b.user,
                b.nice,
                b.system,
                b.idle
            ),
            None => println!("   breakdown: not ready"),
        }
        println!(
            "         cores {} · per-core {:?}",
            s.cpu.core_count,
            s.cpu
                .cores
                .iter()
                .map(|c| format!("{c:.0}"))
                .collect::<Vec<_>>()
        );

        println!(
            "memory   total {:.1} GB · used {:.1} · available {:.1} · used+available {:.1}",
            gb(s.memory.total),
            gb(s.memory.used),
            gb(s.memory.available),
            gb(s.memory.used) + gb(s.memory.available)
        );
        println!(
            "         percent {:.1}%   total-used = {:.1} GB",
            s.memory.percent,
            gb(s.memory.total.saturating_sub(s.memory.used))
        );

        println!(
            "storage  {} · total {:.1} GB · used {:.1} · available {:.1} · percent {:.1}%",
            s.storage.mount_point,
            gb(s.storage.total),
            gb(s.storage.used),
            gb(s.storage.available),
            s.storage.percent
        );

        println!(
            "disk     read {:.1} MB/s · write {:.1} MB/s",
            s.disk.read_rate / 1e6,
            s.disk.write_rate / 1e6
        );
        println!(
            "network  rx {:.1} KB/s · tx {:.1} KB/s",
            s.network.rx_rate / 1e3,
            s.network.tx_rate / 1e3
        );
    }
}
