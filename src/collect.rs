//! Owns the OS handles and samples all metrics each tick.
use crate::metric::Metric;
use sysinfo::{Disks, Networks, ProcessRefreshKind, ProcessesToUpdate, System};

/// One row in the process table: pid, name, CPU%, resident memory bytes.
#[derive(Clone)]
pub struct ProcRow {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub mem: u64,
}


/// Saturating per-tick delta for cumulative counters.
/// Returns 0 on first sample (prev=None) or on counter reset (cur < prev).
/// Reserved for cumulative counters; sysinfo's `received()`/`usage()` already
/// delta per refresh, so no live caller yet.
#[allow(dead_code)]
pub fn delta(prev: &mut Option<u64>, cur: u64) -> u64 {
    let out = match *prev {
        Some(p) => cur.saturating_sub(p),
        None => 0,
    };
    *prev = Some(cur);
    out
}

const GB: f64 = 1_073_741_824.0;

pub struct Collector {
    sys: System,
    nets: Networks,
    disks: Disks,
}

impl Collector {
    pub fn new() -> Self {
        Self {
            sys: System::new_all(),
            nets: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
        }
    }

    /// Refresh OS state and push one sample into each metric.
    /// Order: [cpu, mem, net, disk].
    pub fn sample(&mut self, metrics: &mut [Metric; 4]) {
        self.sample_cpu(&mut metrics[0]);
        self.sample_mem(&mut metrics[1]);
        self.sample_net(&mut metrics[2]);
        self.sample_disk(&mut metrics[3]);
    }

    /// Refresh processes and return the top rows by CPU usage.
    pub fn sample_procs(&mut self) -> Vec<ProcRow> {
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let ncpu = self.sys.cpus().len().max(1) as f32;
        let mut rows: Vec<ProcRow> = self
            .sys
            .processes()
            .iter()
            .map(|(pid, p)| ProcRow {
                pid: pid.as_u32(),
                name: p.name().to_string_lossy().into_owned(),
                // sysinfo reports cpu summed across cores; normalize to 0–100.
                cpu: p.cpu_usage() / ncpu,
                mem: p.memory(),
            })
            .collect();
        // Sort by PID so rows keep a stable position across ticks (values
        // update in place instead of jumping around as CPU usage changes).
        rows.sort_by_key(|r| r.pid);
        rows
    }

    fn sample_cpu(&mut self, m: &mut Metric) {
        self.sys.refresh_cpu_all();
        let global = self.sys.global_cpu_usage();
        let cores: Vec<f32> = self.sys.cpus().iter().map(|c| c.cpu_usage()).collect();
        let max = cores.iter().cloned().fold(0.0_f32, f32::max);
        let min = cores.iter().cloned().fold(100.0_f32, f32::min);
        m.update(
            global.round() as u64,
            None,
            "CPU",
            vec![
                ("Usage".into(), format!("{global:.0}%")),
                ("Cores".into(), format!("{}", cores.len())),
                ("Busiest".into(), format!("{max:.0}%")),
                ("Idlest".into(), format!("{min:.0}%")),
            ],
        );
    }

    fn sample_mem(&mut self, m: &mut Metric) {
        self.sys.refresh_memory();
        let total = self.sys.total_memory().max(1);
        let used = self.sys.used_memory();
        let pct = used as f64 / total as f64 * 100.0;
        let swap = self.sys.used_swap();

        // Left column: physical/used/cached/swap. Right: app/wired/compressed/pressure.
        let mut stats = vec![
            ("Physical".into(), gb(total)),
            ("Used".into(), gb(used)),
        ];
        let extra = mac_mem();
        stats.push(("Cached Files".into(), extra.map(|e| gb(e.cached)).unwrap_or_else(|| "—".into())));
        stats.push(("Swap Used".into(), gb(swap)));
        stats.push(("App Memory".into(), extra.map(|e| gb(e.app)).unwrap_or_else(|| "—".into())));
        stats.push(("Wired".into(), extra.map(|e| gb(e.wired)).unwrap_or_else(|| "—".into())));
        stats.push(("Compressed".into(), extra.map(|e| gb(e.compressed)).unwrap_or_else(|| "—".into())));
        stats.push(("Pressure".into(), format!("{pct:.0}%")));

        m.update(pct.round() as u64, None, "MEMORY", stats);
    }

    fn sample_net(&mut self, m: &mut Metric) {
        self.nets.refresh(true);
        let (mut rx, mut tx, mut trx, mut ttx) = (0u64, 0u64, 0u64, 0u64);
        for (_, d) in self.nets.iter() {
            rx += d.received();
            tx += d.transmitted();
            trx += d.total_received();
            ttx += d.total_transmitted();
        }
        m.update(
            rx,
            Some(tx),
            "NETWORK",
            vec![
                ("In/sec".into(), format!("{}/s", human(rx))),
                ("Out/sec".into(), format!("{}/s", human(tx))),
                ("Total in".into(), human(trx)),
                ("Total out".into(), human(ttx)),
            ],
        );
    }

    fn sample_disk(&mut self, m: &mut Metric) {
        self.disks.refresh(true);
        let (mut r, mut w, mut used, mut total) = (0u64, 0u64, 0u64, 0u64);
        for d in self.disks.list() {
            let u = d.usage();
            r += u.read_bytes;
            w += u.written_bytes;
            total += d.total_space();
            used += d.total_space().saturating_sub(d.available_space());
        }
        m.update(
            r,
            Some(w),
            "DISK",
            vec![
                ("Read/sec".into(), format!("{}/s", human(r))),
                ("Write/sec".into(), format!("{}/s", human(w))),
                ("Used".into(), format!("{:.0} GB", used as f64 / GB)),
                ("Capacity".into(), format!("{:.0} GB", total as f64 / GB)),
            ],
        );
    }

}

/// Format bytes as gibibytes, e.g. "13.9 GB".
fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / GB)
}

/// macOS memory breakdown derived from `vm_stat`, in bytes.
#[derive(Clone, Copy)]
struct MacMem {
    app: u64,
    wired: u64,
    compressed: u64,
    cached: u64,
}

/// Parse `vm_stat` for the Activity-Monitor-style breakdown. macOS only;
/// returns None on any other platform or if the command/parse fails.
#[cfg(target_os = "macos")]
fn mac_mem() -> Option<MacMem> {
    let out = std::process::Command::new("vm_stat").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);

    // Header: "Mach Virtual Memory Statistics: (page size of 16384 bytes)"
    let page: u64 = text
        .lines()
        .next()
        .and_then(|l| l.split("page size of ").nth(1))
        .and_then(|s| s.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(4096);

    let field = |label: &str| -> u64 {
        text.lines()
            .find_map(|l| l.strip_prefix(label))
            .map(|v| v.trim().trim_end_matches('.').trim())
            .and_then(|n| n.parse::<u64>().ok())
            .unwrap_or(0)
            * page
    };

    let wired = field("Pages wired down:");
    let compressed = field("Pages occupied by compressor:");
    let purgeable = field("Pages purgeable:");
    let file_backed = field("File-backed pages:");
    let anonymous = field("Anonymous pages:");

    Some(MacMem {
        app: anonymous.saturating_sub(purgeable),
        wired,
        compressed,
        cached: file_backed + purgeable,
    })
}

#[cfg(not(target_os = "macos"))]
fn mac_mem() -> Option<MacMem> {
    None
}

/// Human-readable bytes (B/KB/MB/GB).
fn human(bytes: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:.1} {}", v, U[i])
}

#[cfg(test)]
mod tests {
    use super::delta;

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_mem_returns_values() {
        let e = super::mac_mem().expect("vm_stat available on macOS");
        assert!(e.wired > 0 && e.compressed > 0, "wired/compressed should be non-zero");
    }

    #[test]
    fn first_sample_is_zero() {
        let mut prev = None;
        assert_eq!(delta(&mut prev, 100), 0);
    }

    #[test]
    fn computes_difference() {
        let mut prev = None;
        delta(&mut prev, 100);
        assert_eq!(delta(&mut prev, 150), 50);
    }

    #[test]
    fn saturates_on_reset() {
        let mut prev = None;
        delta(&mut prev, 200);
        assert_eq!(delta(&mut prev, 10), 0);
    }
}
