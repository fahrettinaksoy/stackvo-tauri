//! Host metrics, read on the host.
//!
//! This module replaces `HostStatsService.js` (293 lines). That file read
//! `/proc/stat`, `/proc/meminfo` and `/proc/net/dev` from *inside a container*,
//! so on Linux it reported the container's constrained view and on macOS — where
//! there is no `/proc` at all — it silently fell through to estimating CPU from
//! `os.loadavg()`. Every number on the dashboard was either scoped wrong or a
//! guess.
//!
//! Running on the host makes the problem disappear rather than solving it.
//!
//! Two numbers the dashboard shows that `sysinfo` alone cannot supply, and how
//! they are obtained honestly rather than approximated:
//!   - **CPU user/nice/system/idle.** `systemstat` reads the platform's own CPU
//!     time counters (mach `host_statistics64` on macOS, `/proc/stat` on Linux,
//!     `GetSystemTimes` on Windows). It needs two samples separated in time,
//!     so the first call after startup reports nothing rather than guessing.
//!   - **Disk read/write throughput.** Summed from per-process disk usage,
//!     which sysinfo does expose everywhere. That is the I/O this machine's
//!     processes actually performed, not a device-level counter — close enough
//!     to be useful and, unlike the old `/proc/diskstats` read from inside a
//!     container, actually about this machine.

use serde::Serialize;
use std::time::Instant;
use sysinfo::{Disks, Networks, System};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuStats {
    /// Total usage across all cores, 0–100.
    pub percent: f32,
    /// Per-core usage, so the UI can show real parallelism.
    pub cores: Vec<f32>,
    pub core_count: usize,
    /// 1/5/15-minute load average. `None` on Windows, which has no equivalent.
    pub load_average: Option<[f64; 3]>,
    /// Where the time went. `None` until two samples exist — the counters are
    /// cumulative, so a single reading cannot produce a percentage — and `None`
    /// for good on a machine where it was measured to disagree with `percent`.
    ///
    /// See `breakdown_is_credible`. It is reported when it can be trusted and
    /// withheld when it cannot, rather than drawn next to a number it
    /// contradicts.
    pub breakdown: Option<CpuBreakdown>,
}

/// CPU time split, as percentages that sum to ~100.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuBreakdown {
    pub user: f32,
    pub nice: f32,
    pub system: f32,
    pub idle: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStats {
    pub total: u64,
    pub used: u64,
    /// `total - used`. The complement of what is in use, and the only figure
    /// that can sit beside `used` in one control without the two contradicting
    /// each other.
    ///
    /// The dashboard used to put `available` there, and on a 24 GB machine that
    /// read: used 18.7, available 12.4 — 31.1 GB of a 24 GB machine. Both
    /// numbers were true and they measure different things, so the donut drawn
    /// from the pair filled to 60% while the label in its middle said 78%.
    pub free: u64,
    /// What a new process could actually get, which on macOS and Linux is more
    /// than `free`: page cache is reclaimable and is counted in `used`. Kept
    /// because it is the more useful number for "can I run this", and reported
    /// separately because it is not part of a total.
    pub available: u64,
    pub percent: f32,
    pub swap_total: u64,
    pub swap_used: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub percent: f32,
    pub mount_point: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskStats {
    /// Cumulative since the sampler started.
    pub read_total: u64,
    pub write_total: u64,
    /// Bytes per second, derived from the gap since the previous sample.
    pub read_rate: f64,
    pub write_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStats {
    /// Cumulative since boot.
    pub rx_total: u64,
    pub tx_total: u64,
    /// Bytes per second, derived from the gap since the previous sample.
    pub rx_rate: f64,
    pub tx_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostStats {
    pub cpu: CpuStats,
    pub memory: MemoryStats,
    pub storage: StorageStats,
    pub network: NetworkStats,
    pub disk: DiskStats,
    pub host_name: Option<String>,
    pub os: Option<String>,
    pub uptime: u64,
    pub timestamp: u64,
}

/// Holds the sampler state. CPU percentages and network rates are both deltas,
/// so the same instance must be reused between calls — a fresh `System` per
/// request reports 0% CPU forever, which is exactly the kind of plausible-but-
/// wrong number this port is meant to eliminate.
pub struct Sampler {
    system: System,
    networks: Networks,
    disks: Disks,
    last_sample: Option<Instant>,
    /// The CPU-time counters are cumulative; a percentage needs the delta
    /// between two readings, which is why this measurement is held open.
    cpu_load: Option<systemstat::DelayedMeasurement<systemstat::CPULoad>>,
    breakdown: Option<CpuBreakdown>,
    /// Set once the CPU-time split has been caught disagreeing with per-core
    /// usage, and never cleared. See `breakdown_is_credible`.
    breakdown_untrusted: bool,
    /// Running per-process disk totals, accumulated from per-refresh deltas.
    disk_totals: (u64, u64),
}

impl Sampler {
    pub fn new() -> Self {
        let mut system = System::new_all();
        // Prime the CPU counters: the first reading after construction is
        // always 0 because there is no previous sample to diff against.
        system.refresh_cpu_usage();

        Self {
            system,
            networks: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            last_sample: None,
            // Deliberately not started here. A measurement opened in the
            // constructor closes microseconds later, over the app's own
            // start-up burst — it would report ~50% system on an idle machine.
            // The first sample() opens it; the second reports it.
            cpu_load: None,
            breakdown: None,
            breakdown_untrusted: false,
            disk_totals: (0, 0),
        }
    }

    pub fn sample(&mut self) -> HostStats {
        let elapsed = self
            .last_sample
            .map(|t| t.elapsed().as_secs_f64())
            .filter(|s| *s > 0.0);
        self.last_sample = Some(Instant::now());

        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system
            .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        self.networks.refresh(true);
        self.disks.refresh(true);
        self.refresh_breakdown();

        let disk = self.disk(elapsed);

        HostStats {
            cpu: self.cpu(),
            memory: self.memory(),
            storage: self.storage(),
            network: self.network(elapsed),
            disk,
            host_name: System::host_name(),
            os: System::long_os_version(),
            uptime: System::uptime(),
            timestamp: now_millis(),
        }
    }

    /// Close the open CPU-time measurement and open the next one.
    ///
    /// Keeps the previous breakdown when the reading is not ready yet, so the
    /// UI shows the last real value rather than flickering to nothing.
    fn refresh_breakdown(&mut self) {
        if let Some(measurement) = self.cpu_load.take() {
            if let Ok(load) = measurement.done() {
                let breakdown = CpuBreakdown {
                    user: load.user * 100.0,
                    nice: load.nice * 100.0,
                    system: load.system * 100.0,
                    idle: load.idle * 100.0,
                };
                // Once, and for the life of the process. The disagreement is
                // a property of the machine's counters, not of the moment: on
                // the Apple Silicon box this was measured on the gap wanders
                // either side of the tolerance, so judging each sample on its
                // own merits made the legend flicker between four rows and two
                // every couple of seconds. A machine whose counters are right
                // never trips this; one whose counters are wrong does not
                // become right later.
                if self.breakdown_untrusted {
                    self.breakdown = None;
                } else if breakdown_is_credible(
                    100.0 - breakdown.idle,
                    self.system.global_cpu_usage(),
                ) {
                    self.breakdown = Some(breakdown);
                } else {
                    tracing::info!(
                        cpu_time_busy = 100.0 - breakdown.idle,
                        per_core_average = self.system.global_cpu_usage(),
                        "the CPU-time split disagrees with per-core usage; not reporting it"
                    );
                    self.breakdown_untrusted = true;
                    self.breakdown = None;
                }
            }
        }
        self.cpu_load = start_cpu_measurement();
    }

    /// System-wide disk throughput, summed over every process.
    ///
    /// From `read_bytes`, which sysinfo defines as the I/O since the last
    /// refresh, rather than from differencing `total_read_bytes` ourselves.
    /// The difference matters when a process exits: its lifetime total leaves
    /// the sum, the running total falls, and the subtraction saturates to zero
    /// — so a tick during which something finished reported no disk activity at
    /// all, however much the rest of the machine had done. A build finishing is
    /// exactly when that happens, and exactly when the number is being watched.
    fn disk(&mut self, elapsed: Option<f64>) -> DiskStats {
        let (mut read_delta, mut write_delta) = (0u64, 0u64);
        for process in self.system.processes().values() {
            let usage = process.disk_usage();
            read_delta += usage.read_bytes;
            write_delta += usage.written_bytes;
        }

        // The running totals stay, because they are what the payload calls
        // totals — but nothing is derived from them any more.
        self.disk_totals.0 = self.disk_totals.0.saturating_add(read_delta);
        self.disk_totals.1 = self.disk_totals.1.saturating_add(write_delta);

        let (read_rate, write_rate) = match elapsed {
            Some(secs) => (read_delta as f64 / secs, write_delta as f64 / secs),
            None => (0.0, 0.0),
        };

        DiskStats {
            read_total: self.disk_totals.0,
            write_total: self.disk_totals.1,
            read_rate,
            write_rate,
        }
    }

    fn cpu(&self) -> CpuStats {
        let cores: Vec<f32> = self.system.cpus().iter().map(|c| c.cpu_usage()).collect();
        let load = System::load_average();

        CpuStats {
            percent: self.system.global_cpu_usage(),
            core_count: cores.len(),
            cores,
            // sysinfo reports zeroes rather than an error on platforms without
            // a load average; treat all-zero as "not available".
            load_average: (load.one > 0.0 || load.five > 0.0 || load.fifteen > 0.0).then_some([
                load.one,
                load.five,
                load.fifteen,
            ]),
            breakdown: self.breakdown,
        }
    }

    fn memory(&self) -> MemoryStats {
        let total = self.system.total_memory();
        let used = self.system.used_memory();

        MemoryStats {
            total,
            used,
            free: total.saturating_sub(used),
            available: self.system.available_memory(),
            percent: percent_of(used, total),
            swap_total: self.system.total_swap(),
            swap_used: self.system.used_swap(),
        }
    }

    /// The disk backing the root filesystem — the one that fills up with Docker
    /// images. Falls back to the largest mounted disk if `/` is not listed.
    fn storage(&self) -> StorageStats {
        let root = self
            .disks
            .list()
            .iter()
            .find(|d| d.mount_point() == std::path::Path::new("/"))
            .or_else(|| self.disks.list().iter().max_by_key(|d| d.total_space()));

        match root {
            Some(disk) => {
                let total = disk.total_space();
                let available = disk.available_space();
                let used = total.saturating_sub(available);
                StorageStats {
                    total,
                    used,
                    available,
                    percent: percent_of(used, total),
                    mount_point: disk.mount_point().display().to_string(),
                }
            }
            None => StorageStats {
                total: 0,
                used: 0,
                available: 0,
                percent: 0.0,
                mount_point: String::new(),
            },
        }
    }

    fn network(&self, elapsed: Option<f64>) -> NetworkStats {
        let mut rx_total = 0u64;
        let mut tx_total = 0u64;
        let mut rx_delta = 0u64;
        let mut tx_delta = 0u64;

        for data in self.networks.values() {
            rx_total += data.total_received();
            tx_total += data.total_transmitted();
            rx_delta += data.received();
            tx_delta += data.transmitted();
        }

        // Without a previous sample there is no rate to report. Zero here means
        // "first sample", not "no traffic" — the UI shows a dash until the
        // second poll lands.
        let (rx_rate, tx_rate) = match elapsed {
            Some(secs) => (rx_delta as f64 / secs, tx_delta as f64 / secs),
            None => (0.0, 0.0),
        };

        NetworkStats {
            rx_total,
            tx_total,
            rx_rate,
            tx_rate,
        }
    }
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

/// Does the CPU-time split agree with the usage figure it sits beside?
///
/// Two independent measurements of the same quantity: `systemstat` reads the
/// platform's CPU-time counters, `sysinfo` averages per-core usage. They should
/// land in the same place, and on Apple Silicon they do not. Measured on an
/// 8-core M-series machine with four cores pinned flat out — where the true
/// figure is known in advance — `sysinfo` reported 63–71% and the CPU-time
/// split reported 95%, saturating; at idle the same split read 12–19% against
/// `sysinfo`'s 24–29%. Not a scale factor, so not something to correct for.
/// (`examples/cpu_truth.rs` is that experiment, kept so the claim can be
/// re-run rather than believed.)
///
/// The likely mechanism is core parking: an idle efficiency core that the
/// scheduler powers down stops accumulating idle ticks, so the idle share of
/// total ticks collapses under load. Whatever the cause, a dashboard that draws
/// a ring from one measurement and prints the other in its middle is wrong
/// twice — the ring disagreed with its own label by 3×.
///
/// So it is checked rather than trusted. Where the two agree the split is real
/// detail worth having (Linux reads `/proc/stat`, which does not have this
/// problem); where they do not, the UI shows the figure it can stand behind and
/// says the split is unavailable.
///
/// A flat margin in percentage points, not a ratio.
///
/// A ratio was tried first and let the case this exists for straight through:
/// 95% against a real 67% is 1.4×, which any tolerance loose enough for two
/// windows that do not line up would have to allow. In points it is 28 apart,
/// which nothing sane calls agreement. The measured disagreements are 28 and
/// 12 points; ordinary window skew on this machine is 1 to 5.
fn breakdown_is_credible(busy: f32, reference: f32) -> bool {
    const TOLERANCE: f32 = 10.0;
    (busy - reference).abs() <= TOLERANCE
}

/// Open a CPU-time measurement. Returns None when the platform refuses, which
/// is reported as an absent breakdown rather than as zeroes.
fn start_cpu_measurement() -> Option<systemstat::DelayedMeasurement<systemstat::CPULoad>> {
    use systemstat::Platform;
    systemstat::System::new().cpu_load_aggregate().ok()
}

fn percent_of(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 / whole as f64 * 100.0) as f32
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_of_handles_a_zero_denominator() {
        assert_eq!(percent_of(0, 0), 0.0);
        assert_eq!(percent_of(50, 100), 50.0);
    }

    #[test]
    fn sampling_reports_real_host_numbers() {
        // The point of the port: these come from the host, not a container's
        // constrained /proc view, and not from a loadavg estimate on macOS.
        let mut sampler = Sampler::new();
        let stats = sampler.sample();

        assert!(stats.memory.total > 0, "host memory must be readable");
        assert!(stats.cpu.core_count > 0, "host CPU count must be readable");
        assert_eq!(stats.cpu.cores.len(), stats.cpu.core_count);
        assert!(stats.storage.total > 0, "root filesystem must be readable");
        assert!(stats.memory.percent >= 0.0 && stats.memory.percent <= 100.0);
    }

    /// Two things, and the second one changed.
    ///
    /// The first sample still cannot report a split — the counters are
    /// cumulative, so one reading describes the app's own start-up burst rather
    /// than the interval anybody asked about.
    ///
    /// The second sample used to be required to produce one. It is not any
    /// more: the split is withheld where it disagrees with the usage figure
    /// beside it, which on Apple Silicon is most of the time, so this asserts
    /// what a reported split must look like rather than that one exists.
    #[test]
    fn a_reported_cpu_breakdown_needs_two_samples_and_sums_to_a_hundred() {
        let mut sampler = Sampler::new();

        assert!(
            sampler.sample().cpu.breakdown.is_none(),
            "the first sample must not report a breakdown"
        );

        std::thread::sleep(std::time::Duration::from_millis(400));
        let sample = sampler.sample();

        let Some(b) = sample.cpu.breakdown else {
            // Withheld, which is a valid outcome and the common one here. The
            // machine this runs on decides which branch is taken, so both have
            // to be acceptable — and `a_breakdown_that_disagrees_with_the_usage_figure_is_withheld`
            // is where the withholding rule itself is pinned down.
            return;
        };

        let total = b.user + b.nice + b.system + b.idle;
        assert!(
            (total - 100.0).abs() < 1.0,
            "breakdown summed to {total}, not ~100"
        );
        for value in [b.user, b.nice, b.system, b.idle] {
            assert!(
                (0.0..=100.0).contains(&value),
                "{value} is not a percentage"
            );
        }

        // And a reported one agrees with the figure it is shown beside — that
        // is the condition it was reported under.
        assert!(
            breakdown_is_credible(100.0 - b.idle, sample.cpu.percent),
            "a breakdown was reported that disagrees with cpu.percent"
        );
    }

    #[test]
    fn disk_throughput_is_measured_not_zeroed() {
        let mut sampler = Sampler::new();
        let first = sampler.sample();
        assert_eq!(
            first.disk.read_rate, 0.0,
            "no previous sample means no rate"
        );

        // Something on this machine reads from disk; the total must be real.
        assert!(
            first.disk.read_total > 0,
            "per-process disk totals should be readable"
        );
    }

    #[test]
    fn first_sample_has_no_network_rate() {
        let mut sampler = Sampler::new();
        let first = sampler.sample();
        assert_eq!(
            first.network.rx_rate, 0.0,
            "no previous sample means no rate"
        );
    }

    /// The memory card draws a ring from `used` and its companion, and prints
    /// `percent` in the middle. Those three have to describe one machine.
    ///
    /// They did not: the companion was `available`, which counts reclaimable
    /// page cache that `used` also counts, so on a 24 GB machine the pair read
    /// 18.7 + 12.4 = 31.1 GB and the ring filled to 60% under a label saying
    /// 78%.
    #[test]
    fn memory_used_and_free_describe_the_same_machine() {
        let mut sampler = Sampler::new();
        let m = sampler.sample().memory;

        assert!(m.total > 0, "no memory reading at all");
        assert_eq!(
            m.used + m.free,
            m.total,
            "used and free must be the whole machine"
        );

        // And the number in the middle of the ring is the ring's own share.
        let drawn = m.used as f32 / m.total as f32 * 100.0;
        assert!(
            (drawn - m.percent).abs() < 0.5,
            "the ring says {drawn:.1}% and the label says {:.1}%",
            m.percent
        );

        // `available` is still reported, and is still allowed to exceed `free`
        // — that is the whole reason it is a separate field.
        assert!(m.available >= m.free || m.available > 0);
    }

    /// The check that decides whether the CPU-time split is shown.
    #[test]
    fn a_breakdown_that_disagrees_with_the_usage_figure_is_withheld() {
        // The measured Apple Silicon disagreement, both directions.
        assert!(
            !breakdown_is_credible(95.0, 67.0),
            "a split saturating at 95% against a real 67% must not be shown"
        );
        assert!(
            !breakdown_is_credible(11.8, 24.2),
            "a split reading half the real figure must not be shown"
        );

        // Ordinary agreement, including the wobble between two measurement
        // windows that do not line up exactly.
        assert!(breakdown_is_credible(30.0, 31.0));
        assert!(breakdown_is_credible(31.0, 30.0));
        assert!(breakdown_is_credible(60.0, 55.0));

        // Near zero a ratio would mean nothing — 1% against 4% is four times
        // as much and also nothing at all — which is why the rule counts
        // points rather than multiples.
        assert!(breakdown_is_credible(1.0, 4.0));
        assert!(breakdown_is_credible(0.0, 0.0));
    }
}
