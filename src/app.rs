//! Application state: which tab is active, the metrics, processes, flags.
use crate::collect::{Collector, ProcRow};
use crate::metric::Metric;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Cpu,
    Memory,
    Network,
    Disk,
    Processes,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Cpu,
        Tab::Memory,
        Tab::Network,
        Tab::Disk,
        Tab::Processes,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Cpu => "CPU",
            Tab::Memory => "Memory",
            Tab::Network => "Network",
            Tab::Disk => "Disk",
            Tab::Processes => "Processes",
        }
    }

    pub fn index(self) -> usize {
        Tab::ALL.iter().position(|&t| t == self).unwrap()
    }
}

pub struct App {
    pub tab: Tab,
    /// Graph metrics, indexed by Tab order for CPU/Memory/Network/Disk only.
    pub metrics: [Metric; 4],
    pub procs: Vec<ProcRow>,
    /// First visible row index in the Processes table.
    pub proc_scroll: usize,
    pub collector: Collector,
    pub paused: bool,
    pub show_help: bool,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            tab: Tab::Cpu,
            metrics: [Metric::new(), Metric::new(), Metric::new(), Metric::new()],
            procs: Vec::new(),
            proc_scroll: 0,
            collector: Collector::new(),
            paused: false,
            show_help: false,
            should_quit: false,
        }
    }

    /// Move the process-table view by `delta` rows, clamped to the list.
    pub fn scroll_procs(&mut self, delta: isize) {
        let max = self.procs.len().saturating_sub(1) as isize;
        self.proc_scroll = (self.proc_scroll as isize + delta).clamp(0, max) as usize;
    }

    pub fn scroll_top(&mut self) {
        self.proc_scroll = 0;
    }

    pub fn next_tab(&mut self) {
        let i = (self.tab.index() + 1) % Tab::ALL.len();
        self.tab = Tab::ALL[i];
    }

    pub fn prev_tab(&mut self) {
        let i = (self.tab.index() + Tab::ALL.len() - 1) % Tab::ALL.len();
        self.tab = Tab::ALL[i];
    }

    /// Select by 1-based number; out-of-range is ignored.
    pub fn select(&mut self, n: usize) {
        if n >= 1 && n <= Tab::ALL.len() {
            self.tab = Tab::ALL[n - 1];
        }
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    /// Sample everything for this tick (no-op while paused).
    pub fn tick(&mut self) {
        if self.paused {
            return;
        }
        self.collector.sample(&mut self.metrics);
        self.procs = self.collector.sample_procs();
    }

    /// Graph metric for the active tab. Only valid for graph tabs (not Processes).
    pub fn active_metric(&self) -> &Metric {
        &self.metrics[self.tab.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::Tab;

    #[test]
    fn next_wraps() {
        assert_eq!(
            Tab::ALL[(Tab::Processes.index() + 1) % Tab::ALL.len()],
            Tab::Cpu
        );
    }

    #[test]
    fn prev_wraps() {
        assert_eq!(
            Tab::ALL[(Tab::Cpu.index() + Tab::ALL.len() - 1) % Tab::ALL.len()],
            Tab::Processes
        );
    }

    #[test]
    fn titles_present() {
        for t in Tab::ALL {
            assert!(!t.title().is_empty());
        }
    }
}
