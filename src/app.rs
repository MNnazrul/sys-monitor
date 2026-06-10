//! Application state: which tab is active, the metrics, processes, flags.
use crate::collect::{Collector, ProcRow};
use crate::metric::Metric;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    /// CPU + Memory + Network + Disk together in a 2×2 grid.
    Overview,
    Processes,
}

impl Tab {
    pub const ALL: [Tab; 2] = [Tab::Overview, Tab::Processes];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
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
    /// Process filter query (matches name or PID); empty = no filter.
    pub search: String,
    /// True while typing into the search box.
    pub searching: bool,
    pub collector: Collector,
    pub paused: bool,
    pub show_help: bool,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            tab: Tab::Overview,
            metrics: [Metric::new(), Metric::new(), Metric::new(), Metric::new()],
            procs: Vec::new(),
            proc_scroll: 0,
            search: String::new(),
            searching: false,
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

    /// Exit search and drop the filter.
    pub fn cancel_search(&mut self) {
        self.search.clear();
        self.searching = false;
        self.proc_scroll = 0;
    }

    /// Processes matching the current search (name substring or PID prefix).
    /// Empty query returns all rows.
    pub fn filtered_procs(&self) -> Vec<&ProcRow> {
        if self.search.is_empty() {
            return self.procs.iter().collect();
        }
        let q = self.search.to_lowercase();
        self.procs
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&q) || p.pid.to_string().contains(&q))
            .collect()
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
}

#[cfg(test)]
mod tests {
    use super::Tab;

    #[test]
    fn next_wraps() {
        assert_eq!(
            Tab::ALL[(Tab::Processes.index() + 1) % Tab::ALL.len()],
            Tab::Overview
        );
    }

    #[test]
    fn prev_wraps() {
        assert_eq!(
            Tab::ALL[(Tab::Overview.index() + Tab::ALL.len() - 1) % Tab::ALL.len()],
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
