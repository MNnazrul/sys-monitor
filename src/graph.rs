//! A filled-area graph widget in the style of macOS Activity Monitor.
//!
//! Single series: fills upward from the bottom with smooth 8-level blocks.
//! Dual series: mirrors about a baseline — primary fills up (e.g. bytes in),
//! secondary fills down (bytes out), so both stay visible without overlap.
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

/// Lower partial blocks, 0..=8 eighths. Index 0 = empty.
const UP: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// Upper partial blocks for the downward (mirrored) series, 0..=2 halves.
const DOWN: [char; 3] = [' ', '▀', '█'];

pub struct Graph<'a> {
    pub primary: &'a [u64],
    pub secondary: Option<&'a [u64]>,
    pub up_color: Color,
    pub down_color: Color,
}

impl<'a> Widget for Graph<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let w = area.width as usize;
        let last = |s: &'a [u64]| -> &'a [u64] {
            let n = s.len();
            &s[n.saturating_sub(w)..]
        };
        let p = last(self.primary);
        let s = self.secondary.map(last);

        // Shared scale across both visible series so proportions match.
        let max = p
            .iter()
            .chain(s.unwrap_or(&[]).iter())
            .copied()
            .max()
            .unwrap_or(0)
            .max(1);

        let left = area.left();
        match s {
            None => {
                // Single series: full height, bottom-anchored.
                fill_up(buf, left, area.bottom(), area.height, p, max, self.up_color);
            }
            Some(sec) => {
                // Baseline at ~55% down: more room for the (usually larger) up series.
                let up_h = (area.height as u32 * 11 / 20).max(1) as u16;
                let baseline = area.top() + up_h;
                let down_h = area.height - up_h;
                fill_up(buf, left, baseline, up_h, p, max, self.up_color);
                fill_down(buf, left, baseline, down_h, sec, max, self.down_color);
            }
        }
    }
}

/// Fill columns upward, bottom edge at `bottom_y` (exclusive), `rows` tall.
fn fill_up(buf: &mut Buffer, left: u16, bottom_y: u16, rows: u16, data: &[u64], max: u64, color: Color) {
    let style = Style::default().fg(color);
    let cells = rows as u64 * 8;
    for (i, &v) in data.iter().enumerate() {
        let x = left + i as u16;
        let filled = (v as u128 * cells as u128 / max as u128) as u64;
        for r in 0..rows {
            let level = filled.saturating_sub(r as u64 * 8).min(8) as usize;
            if level == 0 {
                continue;
            }
            if let Some(cell) = buf.cell_mut((x, bottom_y - 1 - r)) {
                cell.set_char(UP[level]).set_style(style);
            }
        }
    }
}

/// Fill columns downward, top edge at `top_y`, `rows` tall.
fn fill_down(buf: &mut Buffer, left: u16, top_y: u16, rows: u16, data: &[u64], max: u64, color: Color) {
    if rows == 0 {
        return;
    }
    let style = Style::default().fg(color);
    let halves = rows as u64 * 2;
    for (i, &v) in data.iter().enumerate() {
        let x = left + i as u16;
        let filled = (v as u128 * halves as u128 / max as u128) as u64;
        for r in 0..rows {
            let level = filled.saturating_sub(r as u64 * 2).min(2) as usize;
            if level == 0 {
                continue;
            }
            if let Some(cell) = buf.cell_mut((x, top_y + r)) {
                cell.set_char(DOWN[level]).set_style(style);
            }
        }
    }
}
