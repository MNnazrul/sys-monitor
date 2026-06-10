//! Rendering: tab bar, the Overview 2×2 graph grid, and the Processes table.
use crate::app::{App, Tab};
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

const GREEN: Color = Color::Rgb(120, 200, 130);
const YELLOW: Color = Color::Rgb(232, 174, 54);
const RED: Color = Color::Rgb(224, 93, 70);
const BLUE: Color = Color::Rgb(74, 144, 226);

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

    match app.tab {
        Tab::Overview => draw_overview(f, root[1], app),
        Tab::Processes => draw_processes(f, root[1], app),
    }

    if app.show_help {
        draw_help(f, f.area());
    }
}

/// CPU + Memory + Network + Disk in a 2×2 grid, each a titled mini graph.
/// metrics order: 0 = CPU, 1 = Memory, 2 = Network, 3 = Disk.
fn draw_overview(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
    let top = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[0]);
    let bot = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);
    let cells = [top[0], top[1], bot[0], bot[1]];
    for (i, cell) in cells.into_iter().enumerate() {
        draw_mini(f, cell, &app.metrics[i], i);
    }
}

/// One cell of the overview grid: bordered block titled "<NAME>  <value>" with
/// the graph inside. CPU/Memory render as lines; Network/Disk as filled areas.
fn draw_mini(f: &mut Frame, area: Rect, metric: &Metric, idx: usize) {
    // Headline value pulled from the most relevant stat for this metric.
    let key = match idx {
        0 => "Usage",
        1 => "Pressure",
        2 => "In/sec",
        _ => "Read/sec",
    };
    let head = metric
        .stats
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
        .unwrap_or("");

    let mem_color = pressure_color(metric.primary().last().copied().unwrap_or(0));
    let accent = match idx {
        0 => GREEN,
        1 => mem_color,
        _ => BLUE,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {}  {} ", metric.title, head))
        .title_alignment(Alignment::Center)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    match idx {
        0 => line_into(f, inner, &metric.primary(), GREEN),
        1 => line_into(f, inner, &metric.primary(), mem_color),
        _ => area_into(f, inner, metric, accent, RED),
    }
}

/// Full-width process table (PID-ordered, stable) with optional search box.
fn draw_processes(f: &mut Frame, area: Rect, app: &App) {
    // Reserve a search bar when actively typing or a filter is set.
    let show_search = app.searching || !app.search.is_empty();
    let table_area = if show_search {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);
        draw_search_box(f, parts[0], &app.search, app.searching);
        parts[1]
    } else {
        area
    };

    let procs = app.filtered_procs();
    let total = procs.len();
    // Visible rows = inner height minus the border (2) and header (1).
    let visible = table_area.height.saturating_sub(3) as usize;
    let offset = app.proc_scroll.min(total.saturating_sub(1));
    let end = (offset + visible).min(total);
    let shown = &procs[offset.min(total)..end];

    let header = Row::new(["PID", "NAME", "CPU %", "MEMORY"])
        .style(Style::default().fg(Color::Black).bg(GREEN).add_modifier(Modifier::BOLD));

    let rows = shown.iter().map(|p| {
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
            Cell::from(format!("{:.1}", p.cpu)).style(cpu_style),
            Cell::from(bytes(p.mem)),
        ])
    });

    let title = format!(
        "Processes — {}–{} of {}",
        (offset + 1).min(total.max(1)),
        end,
        total
    );
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
    .block(graph_block(&title));
    f.render_widget(table, table_area);
}

/// One-line search input box above the process table.
fn draw_search_box(f: &mut Frame, area: Rect, query: &str, active: bool) {
    let cursor = if active { "▏" } else { "" };
    let line = Line::from(vec![
        Span::styled(" Search ", Style::default().fg(Color::Black).bg(GREEN).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(format!("{query}{cursor}"), Style::default().fg(Color::White)),
        Span::styled(
            if active {
                "   (Enter: keep · Esc: clear)"
            } else {
                "   (/ to edit)"
            },
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let border = if active { GREEN } else { Color::DarkGray };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border));
    f.render_widget(Paragraph::new(line).block(block), area);
}

/// Centered help popup listing keybindings.
fn draw_help(f: &mut Frame, area: Rect) {
    let lines = [
        ("Tab / → / l", "next tab"),
        ("Shift-Tab / ← / h", "previous tab"),
        ("1 – 2", "jump to tab"),
        ("↑ ↓ / j k", "scroll processes"),
        ("PgUp / PgDn", "scroll by page"),
        ("/", "search processes (name/PID)"),
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

/// A smooth braille line graph on a fixed 0–100 scale (CPU/Memory), drawn into
/// `inner` (no surrounding block). Right-anchored: newest sample at the far
/// right. While history fills, the oldest real sample gets a single leading 0
/// so the line rises from the baseline; left of that stays empty.
fn line_into(f: &mut Frame, inner: Rect, data: &[u64], color: Color) {
    let cols = inner.width.max(1) as f64;
    let want = inner.width as usize + 1;
    let vis: Vec<f64> = if data.len() + 1 >= want {
        data[data.len() - want..].iter().map(|&v| v as f64).collect()
    } else {
        std::iter::once(0.0)
            .chain(data.iter().map(|&v| v as f64))
            .collect()
    };
    let m = vis.len();
    let base = cols - (m.saturating_sub(1)) as f64;

    let canvas = Canvas::default()
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
    f.render_widget(canvas, inner);
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
                .bg(GREEN)
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

/// A filled-area graph (auto-scaled) drawn into `inner` (no surrounding block).
/// Dual-series metrics mirror about a baseline (in/read up, out/write down).
fn area_into(f: &mut Frame, inner: Rect, metric: &Metric, up: Color, down: Color) {
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
        scale: None,
    };
    f.render_widget(graph, inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::collect::ProcRow;
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
    fn overview_renders_lines_and_fills() {
        let mut app = App::new();
        app.tab = Tab::Overview;
        for i in 0..200u64 {
            let cpu = (50.0 + (i as f64 * 0.2).sin() * 40.0) as u64;
            let mem = (60.0 + (i as f64 * 0.15).sin() * 18.0) as u64;
            app.metrics[0].update(cpu, None, "CPU", vec![("Usage".into(), format!("{cpu}%"))]);
            app.metrics[1].update(mem, None, "MEMORY", vec![("Pressure".into(), format!("{mem}%"))]);
            app.metrics[2].update(1000 + i, Some(500 + i), "NETWORK", vec![("In/sec".into(), "x".into())]);
            app.metrics[3].update(800 + i, Some(200 + i), "DISK", vec![("Read/sec".into(), "y".into())]);
        }
        let mut term = Terminal::new(TestBackend::new(120, 30)).unwrap();
        term.draw(|f| draw(f, &app)).unwrap();
        let content: Vec<_> = term.backend().buffer().content().to_vec();

        // CPU/Memory render braille lines.
        let braille = content
            .iter()
            .any(|c| c.symbol().chars().any(|ch| ('\u{2800}'..='\u{28FF}').contains(&ch)));
        // Network/Disk render filled block glyphs.
        let fill = "▁▂▃▄▅▆▇█▀";
        let filled = content.iter().any(|c| fill.contains(c.symbol()));
        assert!(braille, "overview should show braille lines for CPU/Memory");
        assert!(filled, "overview should show filled areas for Network/Disk");

        // All four titles present.
        let text: String = content.iter().map(|c| c.symbol()).collect();
        for t in ["CPU", "MEMORY", "NETWORK", "DISK"] {
            assert!(text.contains(t), "overview should label {t}");
        }
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

    #[test]
    fn processes_scroll_windows_rows() {
        let mut app = App::new();
        app.tab = Tab::Processes;
        app.procs = (0..50)
            .map(|i| ProcRow {
                pid: 100 + i,
                name: format!("proc-{i}"),
                cpu: 0.0,
                mem: 1_000_000,
            })
            .collect();
        app.proc_scroll = 5;
        let mut term = Terminal::new(TestBackend::new(80, 12)).unwrap();
        term.draw(|f| draw(f, &app)).unwrap();
        let text: String = term
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        // Scrolled past the first rows; window starts at PID 105.
        assert!(text.contains("proc-5"), "scrolled window should show proc-5");
        assert!(!text.contains("proc-0 "), "first rows scrolled out of view");
        assert!(text.contains("of 50"), "title should show total count");
    }

    #[test]
    fn search_filters_by_name_and_pid() {
        let mut app = App::new();
        app.tab = Tab::Processes;
        app.procs = vec![
            ProcRow { pid: 501, name: "WindowServer".into(), cpu: 1.0, mem: 1_000 },
            ProcRow { pid: 88, name: "kernel_task".into(), cpu: 1.0, mem: 1_000 },
            ProcRow { pid: 777, name: "Safari".into(), cpu: 1.0, mem: 1_000 },
        ];

        // Name match (case-insensitive).
        app.search = "saf".into();
        assert_eq!(app.filtered_procs().len(), 1);
        assert_eq!(app.filtered_procs()[0].name, "Safari");

        // PID match.
        app.search = "88".into();
        assert_eq!(app.filtered_procs().len(), 1);
        assert_eq!(app.filtered_procs()[0].pid, 88);

        // Empty = all.
        app.search.clear();
        assert_eq!(app.filtered_procs().len(), 3);

        // Search box renders when active.
        app.search = "saf".into();
        app.searching = true;
        let mut term = Terminal::new(TestBackend::new(90, 12)).unwrap();
        term.draw(|f| draw(f, &app)).unwrap();
        let text: String = term
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(text.contains("Search"), "search box should render");
        assert!(text.contains("Safari") && !text.contains("kernel_task"));
    }
}
