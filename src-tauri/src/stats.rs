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
    /// cumulative, so a single reading cannot produce a percentage.
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
    /// Per-process disk totals from the previous sample, for the rate.
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
                self.breakdown = Some(CpuBreakdown {
                    user: load.user * 100.0,
                    nice: load.nice * 100.0,
                    system: load.system * 100.0,
                    idle: load.idle * 100.0,
                });
            }
        }
        self.cpu_load = start_cpu_measurement();
    }

    /// System-wide disk throughput, summed over every process.
    fn disk(&mut self, elapsed: Option<f64>) -> DiskStats {
        let (mut read, mut write) = (0u64, 0u64);
        for process in self.system.processes().values() {
            let usage = process.disk_usage();
            read += usage.total_read_bytes;
            write += usage.total_written_bytes;
        }

        // Processes come and go, so the running total can fall; saturating_sub
        // turns that into a zero rate rather than an enormous one.
        let (read_rate, write_rate) = match elapsed {
            Some(secs) => (
                read.saturating_sub(self.disk_totals.0) as f64 / secs,
                write.saturating_sub(self.disk_totals.1) as f64 / secs,
            ),
            None => (0.0, 0.0),
        };
        self.disk_totals = (read, write);

        DiskStats {
            read_total: read,
            write_total: write,
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

        for (_name, data) in self.networks.iter() {
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

    #[test]
    fn the_cpu_breakdown_needs_two_samples_and_then_sums_to_a_hundred() {
        let mut sampler = Sampler::new();

        // The counters are cumulative, so the first sample only opens the
        // measurement. Reporting one here would describe the app's own
        // start-up burst rather than the interval the caller asked about.
        assert!(
            sampler.sample().cpu.breakdown.is_none(),
            "the first sample must not report a breakdown"
        );

        std::thread::sleep(std::time::Duration::from_millis(400));
        let b = sampler
            .sample()
            .cpu
            .breakdown
            .expect("a breakdown once two samples exist");

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
}
