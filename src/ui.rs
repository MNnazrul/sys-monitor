//! Three-pane per-tab layout: stats │ filled graph │ stats, plus a tab bar.
use crate::app::{App, Tab};
use crate::graph::Graph;
use crate::metric::Metric;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
};

const SIDE_WIDTH: u16 = 24;

/// Activity-Monitor-ish palette per tab: (up/primary, down/secondary).
fn colors(tab: Tab) -> (Color, Color) {
    match tab {
        Tab::Cpu => (Color::Rgb(110, 200, 120), Color::Rgb(110, 200, 120)),
        Tab::Memory => (Color::Rgb(232, 174, 54), Color::Rgb(232, 174, 54)),
        Tab::Network => (Color::Rgb(74, 144, 226), Color::Rgb(224, 93, 70)),
        Tab::Disk => (Color::Rgb(74, 144, 226), Color::Rgb(224, 93, 70)),
        Tab::Energy => (Color::Rgb(90, 200, 210), Color::Rgb(90, 200, 210)),
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
    let (up, down) = colors(app.tab);
    let half = metric.stats.len().div_ceil(2);

    draw_stats(f, panes[0], &metric.stats[..half.min(metric.stats.len())]);
    draw_graph(f, panes[1], metric, up, down);
    draw_stats(f, panes[2], &metric.stats[half.min(metric.stats.len())..]);
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

fn draw_graph(f: &mut Frame, area: Rect, metric: &Metric, up: Color, down: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", metric.title))
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::DarkGray));
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
    fn renders_without_panic_and_fills() {
        let mut app = App::new();
        app.tab = Tab::Network;
        // Inject synthetic in/out waves into the Network metric.
        for i in 0..120u64 {
            let rx = (((i as f64 * 0.5).sin() * 0.5 + 0.5) * 100.0) as u64;
            let tx = (((i as f64 * 0.3).cos() * 0.5 + 0.5) * 60.0) as u64;
            app.metrics[2].update(
                rx,
                Some(tx),
                "NETWORK",
                vec![
                    ("In/sec".into(), format!("{rx} KB/s")),
                    ("Out/sec".into(), format!("{tx} KB/s")),
                    ("Total in".into(), "3.0 GB".into()),
                    ("Total out".into(), "323 MB".into()),
                ],
            );
        }
        let mut term = Terminal::new(TestBackend::new(100, 24)).unwrap();
        term.draw(|f| draw(f, &app)).unwrap();
        // Some block glyph must have been drawn in the graph region.
        let buf = term.backend().buffer();
        let drew_fill = buf.content().iter().any(|c| "▁▂▃▄▅▆▇█".contains(c.symbol()));
        assert!(drew_fill, "graph should render filled block glyphs");
    }
}
