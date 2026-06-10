mod app;
mod collect;
mod graph;
mod metric;
mod ui;

use std::io;
use std::time::{Duration, Instant};

use app::App;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

const TICK: Duration = Duration::from_secs(1);
const POLL: Duration = Duration::from_millis(250);

fn main() -> io::Result<()> {
    // Restore terminal even on panic.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        default_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let res = run(&mut terminal);

    restore()?;
    res
}

fn restore() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut app = App::new();
    app.tick(); // seed first sample
    let mut last = Instant::now();

    while !app.should_quit {
        terminal.draw(|f| ui::draw(f, &app))?;

        if event::poll(POLL)?
            && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press {
                    handle_key(&mut app, key.code, key.modifiers);
                }

        if last.elapsed() >= TICK {
            app.tick();
            last = Instant::now();
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    // Esc closes the help overlay first; otherwise it quits.
    if app.show_help && matches!(code, KeyCode::Esc | KeyCode::Char('?')) {
        app.toggle_help();
        return;
    }
    match code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => app.should_quit = true,
        KeyCode::Char('?') => app.toggle_help(),
        KeyCode::Char(' ') => app.toggle_pause(),
        KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => app.next_tab(),
        KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => app.prev_tab(),
        KeyCode::Char(c @ '1'..='5') => app.select((c as u8 - b'0') as usize),
        _ => {}
    }
}
