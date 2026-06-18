//! Application state: which tab is active, the metrics, processes, flags.
use crate::collect::{Collector, ProcRow};
use crate::metric::Metric;

/// Per-process action menu opened by pressing Enter on a row.
pub struct ActionMenu {
    pub pid: u32,
    pub name: String,
    pub selected: usize,
}

impl ActionMenu {
    /// Menu entries, in display order.
    pub const ITEMS: [&'static str; 3] = ["Terminate (SIGTERM)", "Force kill (SIGKILL)", "Cancel"];
}

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
    /// Selected row index into the *filtered* process list.
    pub proc_selected: usize,
    /// Open action menu for a process (Enter on a row), if any.
    pub menu: Option<ActionMenu>,
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
            proc_selected: 0,
            menu: None,
            search: String::new(),
            searching: false,
            collector: Collector::new(),
            paused: false,
            show_help: false,
            should_quit: false,
        }
    }

    /// Number of rows currently visible after filtering.
    fn filtered_len(&self) -> usize {
        self.filtered_procs().len()
    }

    /// Move the selected row by `delta`, clamped to the filtered list.
    pub fn move_selection(&mut self, delta: isize) {
        let max = self.filtered_len().saturating_sub(1) as isize;
        self.proc_selected = (self.proc_selected as isize + delta).clamp(0, max) as usize;
    }

    pub fn select_first(&mut self) {
        self.proc_selected = 0;
    }

    pub fn select_last(&mut self) {
        self.proc_selected = self.filtered_len().saturating_sub(1);
    }

    /// (pid, name) of the currently selected row, if any.
    pub fn selected_proc(&self) -> Option<(u32, String)> {
        self.filtered_procs()
            .get(self.proc_selected)
            .map(|p| (p.pid, p.name.clone()))
    }

    /// Open the action menu for the selected process.
    pub fn open_menu(&mut self) {
        if let Some((pid, name)) = self.selected_proc() {
            self.menu = Some(ActionMenu { pid, name, selected: 0 });
        }
    }

    pub fn close_menu(&mut self) {
        self.menu = None;
    }

    /// Move the menu cursor by `delta`, clamped to the entry count.
    pub fn menu_move(&mut self, delta: isize) {
        if let Some(m) = &mut self.menu {
            let max = (ActionMenu::ITEMS.len() - 1) as isize;
            m.selected = (m.selected as isize + delta).clamp(0, max) as usize;
        }
    }

    /// Run the highlighted menu entry. 0 = SIGTERM, 1 = SIGKILL, 2 = Cancel.
    pub fn menu_confirm(&mut self) {
        let Some(m) = self.menu.take() else { return };
        match m.selected {
            0 => self.kill(m.pid, false),
            1 => self.kill(m.pid, true),
            _ => {} // Cancel
        }
    }

    /// Kill a process, then re-sample and clamp the selection.
    fn kill(&mut self, pid: u32, hard: bool) {
        self.collector.kill(pid, hard);
        self.procs = self.collector.sample_procs();
        let max = self.filtered_len().saturating_sub(1);
        self.proc_selected = self.proc_selected.min(max);
    }

    /// Exit search and drop the filter.
    pub fn cancel_search(&mut self) {
        self.search.clear();
        self.searching = false;
        self.proc_selected = 0;
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
