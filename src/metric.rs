//! A single tracked metric: capped history of one or two u64 series, a graph
//! title, and a list of (label, value) stats for the side panels.
use std::collections::VecDeque;

pub const HISTORY: usize = 1024;

#[derive(Default)]
pub struct Metric {
    primary: VecDeque<u64>,
    secondary: VecDeque<u64>,
    /// True once a secondary series has been pushed (net/disk in-vs-out).
    pub dual: bool,
    pub title: String,
    pub stats: Vec<(String, String)>,
}

impl Metric {
    pub fn new() -> Self {
        Self::default()
    }

    fn push_capped(buf: &mut VecDeque<u64>, v: u64) {
        if buf.len() == HISTORY {
            buf.pop_front();
        }
        buf.push_back(v);
    }

    /// Push a new sample. `secondary` present => dual mirrored graph.
    pub fn update(
        &mut self,
        primary: u64,
        secondary: Option<u64>,
        title: impl Into<String>,
        stats: Vec<(String, String)>,
    ) {
        Self::push_capped(&mut self.primary, primary);
        if let Some(s) = secondary {
            self.dual = true;
            Self::push_capped(&mut self.secondary, s);
        }
        self.title = title.into();
        self.stats = stats;
    }

    pub fn primary(&self) -> Vec<u64> {
        self.primary.iter().copied().collect()
    }

    pub fn secondary(&self) -> Vec<u64> {
        self.secondary.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_at_history_len() {
        let mut m = Metric::new();
        for i in 0..(HISTORY as u64 + 10) {
            m.update(i, None, "t", vec![]);
        }
        assert_eq!(m.primary().len(), HISTORY);
    }

    #[test]
    fn keeps_newest_and_drops_oldest() {
        let mut m = Metric::new();
        for i in 0..(HISTORY as u64 + 5) {
            m.update(i, None, "t", vec![]);
        }
        let d = m.primary();
        assert_eq!(*d.last().unwrap(), HISTORY as u64 + 4);
        assert_eq!(*d.first().unwrap(), 5);
    }

    #[test]
    fn dual_flag_and_title_and_stats() {
        let mut m = Metric::new();
        assert!(!m.dual);
        m.update(1, Some(2), "NET", vec![("In".into(), "5".into())]);
        assert!(m.dual);
        assert_eq!(m.title, "NET");
        assert_eq!(m.stats.len(), 1);
        assert_eq!(m.secondary().last().copied(), Some(2));
    }
}
