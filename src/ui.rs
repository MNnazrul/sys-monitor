//! Renders the tab bar and the active metric's sparkline.
use crate::app::{App, Tab};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    symbols,
    text::Line,
    widgets::{Block, Borders, Sparkline, Tabs},
};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(f.area());

    let titles: Vec<Line> = Tab::ALL.iter().map(|t| Line::from(t.title())).collect();
    let tabs = Tabs::new(titles)
        .select(app.tab.index())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" sys-monitor "),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chunks[0]);

    let metric = app.active_metric();
    let data = metric.data();
    let spark = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", metric.headline)),
        )
        .data(&data)
        .bar_set(symbols::bar::NINE_LEVELS)
        .style(Style::default().fg(Color::Green));
    f.render_widget(spark, chunks[1]);
}
