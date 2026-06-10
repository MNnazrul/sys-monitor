//! Three-pane per-tab layout: stats │ filled graph │ stats, plus a tab bar.
use crate::app::{App, Tab};
use crate::graph::Graph;
use crate::metric::Metric;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        Block, Borders, Paragraph, Tabs,
        canvas::{Canvas, Line as CanvasLine},
    },
};

const SIDE_WIDTH: u16 = 24;

const GREEN: Color = Color::Rgb(120, 200, 130);
const YELLOW: Color = Color::Rgb(232, 174, 54);
const RED: Color = Color::Rgb(224, 93, 70);
const BLUE: Color = Color::Rgb(74, 144, 226);
const CYAN: Color = Color::Rgb(90, 200, 210);

/// Activity-Monitor-ish palette per tab: (up/primary, down/secondary).
fn colors(tab: Tab) -> (Color, Color) {
    match tab {
        Tab::Cpu => (GREEN, GREEN),
        Tab::Memory => (GREEN, GREEN), // overridden by pressure_color() at draw time
        Tab::Network => (BLUE, RED),
        Tab::Disk => (BLUE, RED),
        Tab::Energy => (CYAN, CYAN),
    }
}

/// Memory pressure color, like Activity Monitor: green (fine) → yellow → red.
fn pressure_color(used_pct: u64) -> Color {
    match used_pct {
        0..=69 => GREEN,
        70..=84 => YELLOW,
        _ => RED,
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(f.area());

    draw_tabs(f, app, root[0]);

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(SIDE_WIDTH),
            Constraint::Min(20),
            Constraint::Length(SIDE_WIDTH),
        ])
        .split(root[1]);

    let metric = app.active_metric();
    let (up, down) = if app.tab == Tab::Memory {
        let used = metric.primary().last().copied().unwrap_or(0);
        let c = pressure_color(used);
        (c, c)
    } else {
        colors(app.tab)
    };
    let half = metric.stats.len().div_ceil(2);

    draw_stats(f, panes[0], &metric.stats[..half.min(metric.stats.len())]);
    if app.tab == Tab::Memory {
        // Memory shown as a smooth line on a fixed 0–100 scale.
        draw_line_graph(f, panes[1], metric, up);
    } else {
        // Percentages get a fixed 0–100 scale; rates auto-scale to their peak.
        let scale = match app.tab {
            Tab::Cpu => Some(100),
            _ => None,
        };
        draw_graph(f, panes[1], metric, up, down, scale);
    }
    draw_stats(f, panes[2], &metric.stats[half.min(metric.stats.len())..]);
}

/// A smooth braille line graph on a fixed 0–100 scale (used for Memory).
/// Right-anchored: the newest sample sits at the far-right edge.
fn draw_line_graph(f: &mut Frame, area: Rect, metric: &Metric, color: Color) {
    let block = graph_block(&metric.title);
    let inner = block.inner(area);
    let cols = inner.width.max(1) as f64;

    let all = metric.primary();
    // Keep one point per horizontal column (newest on the right).
    let take = (inner.width as usize + 1).min(all.len());
    let vis: Vec<f64> = all[all.len() - take..].iter().map(|&v| v as f64).collect();
    let m = vis.len();

    let canvas = Canvas::default()
        .block(block)
        .marker(Marker::Braille)
        .x_bounds([0.0, cols])
        .y_bounds([0.0, 100.0])
        .paint(move |ctx| {
            if m < 2 {
                return;
            }
            for i in 0..m - 1 {
                // Right-anchor: oldest visible sample maps left, newest to cols.
                let x1 = cols - (m - 1 - i) as f64;
                let x2 = cols - (m - 2 - i) as f64;
                ctx.draw(&CanvasLine {
                    x1,
                    y1: vis[i],
                    x2,
                    y2: vis[i + 1],
                    color,
                });
            }
        });
    f.render_widget(canvas, area);
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = Tab::ALL.iter().map(|t| Line::from(t.title())).collect();
    let tabs = Tabs::new(titles)
        .select(app.tab.index())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" sys-monitor ")
                .title_alignment(Alignment::Center),
        )
        .divider("│")
        .padding(" ", " ")
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(colors(app.tab).0)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().fg(Color::Gray));
    f.render_widget(tabs, area);
}

/// Bordered, centered-title block used by every graph pane.
fn graph_block(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "))
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::DarkGray))
}

fn draw_graph(f: &mut Frame, area: Rect, metric: &Metric, up: Color, down: Color, scale: Option<u64>) {
    let block = graph_block(&metric.title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let primary = metric.primary();
    let secondary = metric.secondary();
    let graph = Graph {
        primary: &primary,
        secondary: if metric.dual {
            Some(secondary.as_slice())
        } else {
            None
        },
        up_color: up,
        down_color: down,
        scale,
    };
    f.render_widget(graph, inner);
}

fn draw_stats(f: &mut Frame, area: Rect, stats: &[(String, String)]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let width = inner.width as usize;
    let label_style = Style::default().fg(Color::Gray);
    let value_style = Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    let mut lines: Vec<Line> = vec![Line::from("")]; // top breathing room
    for (k, v) in stats {
        let pad = width.saturating_sub(k.len() + v.len()).max(1);
        lines.push(Line::from(vec![
            Span::styled(k.clone(), label_style),
            Span::raw(" ".repeat(pad)),
            Span::styled(v.clone(), value_style),
        ]));
        lines.push(Line::from(""));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn pressure_color_thresholds() {
        assert_eq!(pressure_color(40), GREEN);
        assert_eq!(pressure_color(69), GREEN);
        assert_eq!(pressure_color(70), YELLOW);
        assert_eq!(pressure_color(84), YELLOW);
        assert_eq!(pressure_color(85), RED);
        assert_eq!(pressure_color(100), RED);
    }

    #[test]
    fn renders_without_panic_and_fills() {
        let mut app = App::new();
        // A few samples; graph must right-anchor (newest at the far-right column).
        for v in [10u64, 30, 50, 70, 90] {
            app.metrics[0].update(v, None, "CPU", vec![("Usage".into(), format!("{v}%"))]);
        }
        let mut term = Terminal::new(TestBackend::new(100, 24)).unwrap();
        term.draw(|f| draw(f, &app)).unwrap();
        let buf = term.backend().buffer();

        // Collect the x of every filled block cell.
        let fill = "▁▂▃▄▅▆▇█";
        let mut xs: Vec<u16> = vec![];
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                if fill.contains(buf.cell((x, y)).unwrap().symbol()) {
                    xs.push(x);
                }
            }
        }
        assert!(!xs.is_empty(), "graph should render filled block glyphs");

        // Graph pane inner right edge with width 100 = col 74. Right-anchored:
        // 5 samples occupy only the ~5 rightmost columns, nothing further left.
        let max_x = *xs.iter().max().unwrap();
        let min_x = *xs.iter().min().unwrap();
        assert!(max_x >= 73, "newest sample should sit at the far right (got {max_x})");
        assert!(min_x >= 69, "only the last few columns filled (got {min_x})");
    }

    #[test]
    fn memory_renders_braille_line() {
        let mut app = App::new();
        app.tab = Tab::Memory;
        for i in 0..200u64 {
            let v = (60.0 + (i as f64 * 0.15).sin() * 18.0) as u64;
            app.metrics[1].update(v, None, "MEMORY", vec![("Pressure".into(), format!("{v}%"))]);
        }
        let mut term = Terminal::new(TestBackend::new(100, 24)).unwrap();
        term.draw(|f| draw(f, &app)).unwrap();
        let buf = term.backend().buffer();

        // Canvas line uses braille glyphs (U+2800..U+28FF).
        let drew_line = buf
            .content()
            .iter()
            .any(|c| c.symbol().chars().any(|ch| ('\u{2800}'..='\u{28FF}').contains(&ch)));
        assert!(drew_line, "memory tab should render a braille line");
    }
}
