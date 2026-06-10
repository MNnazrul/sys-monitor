//! Application state: which tab is active, the four metrics, quit flag.
use crate::collect::Collector;
use crate::metric::Metric;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Cpu,
    Memory,
    Network,
    Disk,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Cpu, Tab::Memory, Tab::Network, Tab::Disk];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Cpu => "CPU",
            Tab::Memory => "Memory",
            Tab::Network => "Network",
            Tab::Disk => "Disk",
        }
    }

    pub fn index(self) -> usize {
        Tab::ALL.iter().position(|&t| t == self).unwrap()
    }
}

pub struct App {
    pub tab: Tab,
    pub metrics: [Metric; 4],
    pub collector: Collector,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            tab: Tab::Cpu,
            metrics: [Metric::new(), Metric::new(), Metric::new(), Metric::new()],
            collector: Collector::new(),
            should_quit: false,
        }
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

    pub fn tick(&mut self) {
        self.collector.sample(&mut self.metrics);
    }

    pub fn active_metric(&self) -> &Metric {
        &self.metrics[self.tab.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::Tab;

    #[test]
    fn next_wraps() {
        assert_eq!(Tab::ALL[(Tab::Disk.index() + 1) % Tab::ALL.len()], Tab::Cpu);
    }

    #[test]
    fn prev_wraps() {
        assert_eq!(
            Tab::ALL[(Tab::Cpu.index() + Tab::ALL.len() - 1) % Tab::ALL.len()],
            Tab::Disk
        );
    }

    #[test]
    fn titles_present() {
        for t in Tab::ALL {
            assert!(!t.title().is_empty());
        }
    }
}
