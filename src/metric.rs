//! A single tracked metric: a capped history of u64 samples plus a headline.
use std::collections::VecDeque;

pub const HISTORY: usize = 60;

pub struct Metric {
    history: VecDeque<u64>,
    pub headline: String,
}

impl Metric {
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(HISTORY),
            headline: String::new(),
        }
    }

    /// Push a sample and headline; drops the oldest beyond HISTORY.
    pub fn push(&mut self, value: u64, headline: String) {
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(value);
        self.headline = headline;
    }

    /// Newest-last contiguous slice for the Sparkline widget.
    pub fn data(&self) -> Vec<u64> {
        self.history.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_at_history_len() {
        let mut m = Metric::new();
        for i in 0..(HISTORY as u64 + 10) {
            m.push(i, String::new());
        }
        assert_eq!(m.data().len(), HISTORY);
    }

    #[test]
    fn keeps_newest_and_drops_oldest() {
        let mut m = Metric::new();
        for i in 0..(HISTORY as u64 + 5) {
            m.push(i, String::new());
        }
        let d = m.data();
        assert_eq!(*d.last().unwrap(), HISTORY as u64 + 4);
        assert_eq!(*d.first().unwrap(), 5); // 0..=4 dropped
    }

    #[test]
    fn stores_headline() {
        let mut m = Metric::new();
        m.push(1, "CPU 5%".into());
        assert_eq!(m.headline, "CPU 5%");
    }
}
