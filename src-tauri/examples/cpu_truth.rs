//! Which CPU number is telling the truth.
//!
//! The dashboard draws one control out of two different measurements: the ring
//! comes from `systemstat`'s CPU-time breakdown, the figure in the middle from
//! `sysinfo`'s per-core average. They disagree, so at most one of them is right
//! and the control is wrong either way.
//!
//! This settles it by making the answer known in advance: spin exactly half the
//! cores flat out and see which number lands where it should. It also drives
//! `systemstat` standalone over a clean window, so a wrong answer can be
//! attributed to the library rather than to how the sampler uses it.
//!
//! ```sh
//! cargo run --release --example cpu_truth
//! ```

use stackvo_desktop_lib::stats::Sampler;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use systemstat::Platform;

fn standalone_busy(window: std::time::Duration) -> Option<f32> {
    let measurement = systemstat::System::new().cpu_load_aggregate().ok()?;
    std::thread::sleep(window);
    let load = measurement.done().ok()?;
    Some((1.0 - load.idle) * 100.0)
}

fn main() {
    let mut sampler = Sampler::new();
    let cores = sampler.sample().cpu.core_count;
    let busy = cores / 2;

    println!("{cores} cores, spinning {busy}\n");

    println!("── idle ──");
    for round in 1..=2 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let s = sampler.sample();
        println!(
            "  {round}: sysinfo {:>5.1}%   systemstat(sampler) {:>5.1}%   systemstat(standalone) {:>5.1}%",
            s.cpu.percent,
            s.cpu.breakdown.map(|b| 100.0 - b.idle).unwrap_or(f32::NAN),
            standalone_busy(std::time::Duration::from_secs(1)).unwrap_or(f32::NAN),
        );
    }

    let stop = Arc::new(AtomicBool::new(false));
    let workers: Vec<_> = (0..busy)
        .map(|_| {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut x: u64 = 0;
                while !stop.load(Ordering::Relaxed) {
                    x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
                }
                x
            })
        })
        .collect();

    println!("\n── {busy}/{cores} cores pinned ──");
    std::thread::sleep(std::time::Duration::from_secs(2));
    let _ = sampler.sample();

    for round in 1..=3 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let s = sampler.sample();
        println!(
            "  {round}: sysinfo {:>5.1}%   systemstat(sampler) {:>5.1}%   systemstat(standalone) {:>5.1}%",
            s.cpu.percent,
            s.cpu.breakdown.map(|b| 100.0 - b.idle).unwrap_or(f32::NAN),
            standalone_busy(std::time::Duration::from_secs(1)).unwrap_or(f32::NAN),
        );
    }

    stop.store(true, Ordering::Relaxed);
    for w in workers {
        let _ = w.join();
    }
}
