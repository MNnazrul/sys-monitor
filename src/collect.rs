//! Owns the OS handles and samples all five metrics each tick.
use crate::metric::Metric;
use sysinfo::{Disks, Networks, System};

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

pub struct Collector {
    sys: System,
    nets: Networks,
    disks: Disks,
    battery: Option<battery::Manager>,
}

impl Collector {
    pub fn new() -> Self {
        Self {
            sys: System::new_all(),
            nets: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            battery: battery::Manager::new().ok(),
        }
    }

    /// Refresh OS state and push one sample+headline into each metric.
    /// Order: [cpu, mem, net, disk, energy].
    pub fn sample(&mut self, metrics: &mut [Metric; 5]) {
        self.sample_cpu(&mut metrics[0]);
        self.sample_mem(&mut metrics[1]);
        self.sample_net(&mut metrics[2]);
        self.sample_disk(&mut metrics[3]);
        self.sample_energy(&mut metrics[4]);
    }

    fn sample_cpu(&mut self, m: &mut Metric) {
        self.sys.refresh_cpu_all();
        let global = self.sys.global_cpu_usage();
        let cores: Vec<String> = self
            .sys
            .cpus()
            .iter()
            .map(|c| format!("{:.0}", c.cpu_usage()))
            .collect();
        m.push(
            global.round() as u64,
            format!("CPU {:.0}%   cores: {}", global, cores.join(" ")),
        );
    }

    fn sample_mem(&mut self, m: &mut Metric) {
        self.sys.refresh_memory();
        let total = self.sys.total_memory().max(1);
        let used = self.sys.used_memory();
        let pct = used as f64 / total as f64 * 100.0;
        let gb = 1_073_741_824.0;
        m.push(
            pct.round() as u64,
            format!(
                "Mem {:.1} / {:.1} GB ({:.0}%)",
                used as f64 / gb,
                total as f64 / gb,
                pct
            ),
        );
    }

    fn sample_net(&mut self, m: &mut Metric) {
        self.nets.refresh(true);
        let (mut rx, mut tx) = (0u64, 0u64);
        for (_, data) in self.nets.iter() {
            rx += data.received();
            tx += data.transmitted();
        }
        let total = rx + tx;
        m.push(total, format!("↓ {}/s   ↑ {}/s", human(rx), human(tx)));
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
        let gb = 1_073_741_824.0;
        m.push(
            r + w,
            format!(
                "R {}/s  W {}/s   {:.0}/{:.0} GB used",
                human(r),
                human(w),
                used as f64 / gb,
                total as f64 / gb
            ),
        );
    }

    fn sample_energy(&mut self, m: &mut Metric) {
        let Some(mgr) = &self.battery else {
            m.push(0, "no battery detected".into());
            return;
        };
        let batteries = match mgr.batteries() {
            Ok(b) => b,
            Err(_) => {
                m.push(0, "no battery detected".into());
                return;
            }
        };
        // First battery only.
        if let Some(Ok(bat)) = batteries.into_iter().next() {
            use battery::units::{power::watt, ratio::percent, time::minute};
            let watts = bat.energy_rate().get::<watt>();
            let charge = bat.state_of_charge().get::<percent>();
            let state = format!("{:?}", bat.state()).to_lowercase();
            let mw = (watts * 1000.0).round() as u64;
            let time = bat
                .time_to_empty()
                .map(|t| {
                    let mins = t.get::<minute>() as u64;
                    format!("{}h{:02}m", mins / 60, mins % 60)
                })
                .unwrap_or_else(|| "—".into());
            if mw == 0 {
                m.push(0, format!("plugged in · {:.0}%", charge));
            } else {
                m.push(
                    mw,
                    format!("{:.1} W {} · {:.0}% · {}", watts, state, charge, time),
                );
            }
        } else {
            m.push(0, "no battery detected".into());
        }
    }
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
