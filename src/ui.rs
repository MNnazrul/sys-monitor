//! Three-pane per-tab layout: stats │ filled graph │ stats, plus a tab bar.
use crate::app::{App, Tab};
use crate::collect::ProcRow;
use crate::graph::Graph;
use crate::metric::Metric;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, Paragraph, Row, Table, Tabs,
        canvas::{Canvas, Line as CanvasLine},
    },
};

const SIDE_WIDTH: u16 = 24;

const GREEN: Color = Color::Rgb(120, 200, 130);
const YELLOW: Color = Color::Rgb(232, 174, 54);
const RED: Color = Color::Rgb(224, 93, 70);
const BLUE: Color = Color::Rgb(74, 144, 226);

/// Activity-Monitor-ish palette per tab: (up/primary, down/secondary).
fn colors(tab: Tab) -> (Color, Color) {
    match tab {
        Tab::Cpu => (GREEN, GREEN),
        Tab::Memory => (GREEN, GREEN), // overridden by pressure_color() at draw time
        Tab::Network => (BLUE, RED),
        Tab::Disk => (BLUE, RED),
        Tab::Processes => (GREEN, GREEN),
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

    if app.tab == Tab::Processes {
        draw_processes(f, root[1], &app.procs);
    } else {
        draw_metric_panes(f, root[1], app);
    }

    if app.show_help {
        draw_help(f, f.area());
    }
}

/// CPU/Memory/Network/Disk: stats │ graph │ stats.
fn draw_metric_panes(f: &mut Frame, area: Rect, app: &App) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(SIDE_WIDTH),
            Constraint::Min(20),
            Constraint::Length(SIDE_WIDTH),
        ])
        .split(area);

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
    match app.tab {
        // Percentages: smooth line on a fixed 0–100 scale.
        Tab::Cpu | Tab::Memory => draw_line_graph(f, panes[1], metric, up),
        // Rates: filled area, auto-scaled to peak.
        _ => draw_graph(f, panes[1], metric, up, down, None),
    }
    draw_stats(f, panes[2], &metric.stats[half.min(metric.stats.len())..]);
}

/// Full-width process table sorted by CPU usage.
fn draw_processes(f: &mut Frame, area: Rect, procs: &[ProcRow]) {
    let header = Row::new(["PID", "NAME", "CPU %", "MEMORY"])
        .style(Style::default().fg(Color::Black).bg(GREEN).add_modifier(Modifier::BOLD));

    let rows = procs.iter().map(|p| {
        let cpu = format!("{:.1}", p.cpu);
        let cpu_style = if p.cpu >= 50.0 {
            Style::default().fg(RED)
        } else if p.cpu >= 15.0 {
            Style::default().fg(YELLOW)
        } else {
            Style::default().fg(Color::Gray)
        };
        Row::new(vec![
            Cell::from(p.pid.to_string()),
            Cell::from(p.name.clone()),
            Cell::from(cpu).style(cpu_style),
            Cell::from(bytes(p.mem)),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(12),
        ],
    )
    .header(header)
    .column_spacing(2)
    .block(graph_block("Processes — top by CPU"));
    f.render_widget(table, area);
}

/// Centered help popup listing keybindings.
fn draw_help(f: &mut Frame, area: Rect) {
    let lines = [
        ("Tab / → / l", "next tab"),
        ("Shift-Tab / ← / h", "previous tab"),
        ("1 – 5", "jump to tab"),
        ("space", "pause / resume"),
        ("?", "toggle this help"),
        ("q / Esc", "quit"),
    ];
    let text: Vec<Line> = std::iter::once(Line::from(""))
        .chain(lines.iter().map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("  {k:<20}"), Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
                Span::styled((*v).to_string(), Style::default().fg(Color::White)),
            ])
        }))
        .collect();

    let w = 44.min(area.width);
    let h = (text.len() as u16 + 2).min(area.height);
    let popup = center(area, w, h);
    f.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Keybindings ")
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(GREEN));
    f.render_widget(Paragraph::new(text).block(block), popup);
}

/// A `w × h` rect centered within `area`.
fn center(area: Rect, w: u16, h: u16) -> Rect {
    let [_, mid, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(h),
        Constraint::Fill(1),
    ])
    .flex(Flex::Center)
    .areas(area);
    let [_, rect, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(w),
        Constraint::Fill(1),
    ])
    .flex(Flex::Center)
    .areas(mid);
    rect
}

/// Human-readable bytes for the process table.
fn bytes(n: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{:.1} {}", v, U[i])
}

/// A smooth braille line graph on a fixed 0–100 scale (used for CPU/Memory).
/// Right-anchored: newest sample at the far right. While history fills, the
/// oldest real sample gets a single leading 0 so the line rises from the
/// baseline; everything left of that stays empty (no flat zero line).
fn draw_line_graph(f: &mut Frame, area: Rect, metric: &Metric, color: Color) {
    let block = graph_block(&metric.title);
    let inner = block.inner(area);
    let cols = inner.width.max(1) as f64;

    let want = inner.width as usize + 1;
    let all = metric.primary();
    let vis: Vec<f64> = if all.len() + 1 >= want {
        // Full window: real samples span the whole width.
        all[all.len() - want..].iter().map(|&v| v as f64).collect()
    } else {
        // Still filling: one leading 0 then the real samples, right-anchored.
        std::iter::once(0.0)
            .chain(all.iter().map(|&v| v as f64))
            .collect()
    };
    let m = vis.len();
    // Right-anchor: newest (i = m-1) at x = cols, older samples to the left.
    let base = cols - (m.saturating_sub(1)) as f64;

    let canvas = Canvas::default()
        .block(block)
        .marker(Marker::Braille)
        .x_bounds([0.0, cols])
        .y_bounds([0.0, 100.0])
        .paint(move |ctx| {
            for i in 0..m.saturating_sub(1) {
                ctx.draw(&CanvasLine {
                    x1: base + i as f64,
                    y1: vis[i],
                    x2: base + (i + 1) as f64,
                    y2: vis[i + 1],
                    color,
                });
            }
        });
    f.render_widget(canvas, area);
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = Tab::ALL.iter().map(|t| Line::from(t.title())).collect();
    let title = if app.paused {
        " sys-monitor  [PAUSED] "
    } else {
        " sys-monitor "
    };
    let tabs = Tabs::new(titles)
        .select(app.tab.index())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_alignment(Alignment::Center)
                .title_style(if app.paused {
                    Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                }),
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
        app.tab = Tab::Network;
        // A few samples; filled graph must right-anchor (newest at far right).
        for (rx, tx) in [(10u64, 5u64), (30, 15), (50, 25), (70, 35), (90, 45)] {
            app.metrics[2].update(rx, Some(tx), "NETWORK", vec![("In".into(), format!("{rx}"))]);
        }
        let mut term = Terminal::new(TestBackend::new(100, 24)).unwrap();
        term.draw(|f| draw(f, &app)).unwrap();
        let buf = term.backend().buffer();

        // Collect the x of every filled block cell (up and down glyphs).
        let fill = "▁▂▃▄▅▆▇█▀";
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

    #[test]
    fn processes_tab_and_help_render() {
        let mut app = App::new();
        app.tab = Tab::Processes;
        app.procs = vec![
            ProcRow { pid: 501, name: "WindowServer".into(), cpu: 62.4, mem: 1_400_000_000 },
            ProcRow { pid: 88, name: "kernel_task".into(), cpu: 3.0, mem: 320_000_000 },
        ];
        app.show_help = true;
        app.paused = true;
        let mut term = Terminal::new(TestBackend::new(100, 24)).unwrap();
        term.draw(|f| draw(f, &app)).unwrap();
        let text: String = term
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("WindowServer"), "process name should render");
        assert!(text.contains("Keybindings"), "help overlay should render");
        assert!(text.contains("PAUSED"), "paused indicator should render");
    }
}
