//! Application state: which tab is active, the metrics, processes, flags.
use crate::collect::{Collector, ProcRow};
use crate::metric::Metric;

/// Column the process table is sorted by.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Pid,
    Name,
    Cpu,
    Mem,
}

impl SortKey {
    /// Cycle order for the `s` key.
    pub fn next(self) -> Self {
        match self {
            SortKey::Pid => SortKey::Cpu,
            SortKey::Cpu => SortKey::Mem,
            SortKey::Mem => SortKey::Name,
            SortKey::Name => SortKey::Pid,
        }
    }

    /// Sensible default direction when switching to this column.
    pub fn default_desc(self) -> bool {
        matches!(self, SortKey::Cpu | SortKey::Mem)
    }
}

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
    /// Column the process table is sorted by, and the direction.
    pub sort_key: SortKey,
    pub sort_desc: bool,
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
            sort_key: SortKey::Pid,
            sort_desc: false,
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

    /// Kill a process, then re-sample (keeping the cursor on its process).
    fn kill(&mut self, pid: u32, hard: bool) {
        self.collector.kill(pid, hard);
        let keep = self.selected_proc().map(|(p, _)| p);
        self.procs = self.collector.sample_procs();
        self.sort_procs();
        self.reselect(keep);
    }

    /// Sort `procs` by the active key/direction, with PID as a stable
    /// tie-break so equal values never jitter between ticks.
    pub fn sort_procs(&mut self) {
        let (key, desc) = (self.sort_key, self.sort_desc);
        self.procs.sort_by(|a, b| {
            let ord = match key {
                SortKey::Pid => a.pid.cmp(&b.pid),
                SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortKey::Cpu => a.cpu.total_cmp(&b.cpu),
                SortKey::Mem => a.mem.cmp(&b.mem),
            };
            let ord = if desc { ord.reverse() } else { ord };
            ord.then(a.pid.cmp(&b.pid))
        });
    }

    /// Cycle to the next sort column, resetting to its default direction.
    pub fn cycle_sort(&mut self) {
        let keep = self.selected_proc().map(|(p, _)| p);
        self.sort_key = self.sort_key.next();
        self.sort_desc = self.sort_key.default_desc();
        self.sort_procs();
        self.reselect(keep);
    }

    /// Flip the current sort direction.
    pub fn toggle_sort_dir(&mut self) {
        let keep = self.selected_proc().map(|(p, _)| p);
        self.sort_desc = !self.sort_desc;
        self.sort_procs();
        self.reselect(keep);
    }

    /// Move the cursor back to `pid` after a re-sort; clamp if it's gone.
    fn reselect(&mut self, pid: Option<u32>) {
        if let Some(pid) = pid
            && let Some(i) = self.filtered_procs().iter().position(|p| p.pid == pid)
        {
            self.proc_selected = i;
            return;
        }
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
        let keep = self.selected_proc().map(|(p, _)| p);
        self.procs = self.collector.sample_procs();
        self.sort_procs();
        self.reselect(keep);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with(rows: &[(u32, &str, f32, u64)]) -> App {
        let mut app = App::new();
        app.procs = rows
            .iter()
            .map(|&(pid, name, cpu, mem)| ProcRow { pid, name: name.into(), cpu, mem })
            .collect();
        app
    }

    #[test]
    fn cycle_sort_orders_by_cpu_desc() {
        let mut app = app_with(&[(1, "a", 5.0, 10), (2, "b", 90.0, 10), (3, "c", 40.0, 10)]);
        app.sort_procs(); // default Pid asc
        assert_eq!(app.procs.iter().map(|p| p.pid).collect::<Vec<_>>(), [1, 2, 3]);

        app.cycle_sort(); // Pid -> Cpu (desc)
        assert_eq!(app.sort_key, SortKey::Cpu);
        assert!(app.sort_desc);
        assert_eq!(app.procs.iter().map(|p| p.pid).collect::<Vec<_>>(), [2, 3, 1]);
    }

    #[test]
    fn toggle_dir_reverses() {
        let mut app = app_with(&[(1, "a", 5.0, 10), (2, "b", 90.0, 10)]);
        app.sort_key = SortKey::Cpu;
        app.sort_desc = true;
        app.sort_procs();
        assert_eq!(app.procs[0].pid, 2);
        app.toggle_sort_dir();
        assert!(!app.sort_desc);
        assert_eq!(app.procs[0].pid, 1);
    }

    #[test]
    fn selection_follows_pid_after_sort() {
        let mut app = app_with(&[(1, "a", 5.0, 10), (2, "b", 90.0, 10), (3, "c", 40.0, 10)]);
        app.sort_procs();
        app.proc_selected = 2; // pid 3
        app.cycle_sort(); // now Cpu desc => [2,3,1]; pid 3 is at index 1
        assert_eq!(app.selected_proc().map(|(p, _)| p), Some(3));
    }

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
