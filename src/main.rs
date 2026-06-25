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
    // Action menu captures keys until an entry is chosen or it's dismissed.
    if app.menu.is_some() {
        match code {
            KeyCode::Up | KeyCode::Char('k') => app.menu_move(-1),
            KeyCode::Down | KeyCode::Char('j') => app.menu_move(1),
            KeyCode::Enter => app.menu_confirm(),
            KeyCode::Esc | KeyCode::Char('q') => app.close_menu(),
            _ => {}
        }
        return;
    }

    // Search input mode captures typing, but still allows selection moves.
    if app.searching {
        match code {
            KeyCode::Esc => app.cancel_search(),     // clear filter + exit
            KeyCode::Enter => app.searching = false, // keep filter, exit input
            KeyCode::Backspace => {
                app.search.pop();
                app.proc_selected = 0;
            }
            KeyCode::Up => app.move_selection(-1),
            KeyCode::Down => app.move_selection(1),
            KeyCode::PageUp => app.move_selection(-10),
            KeyCode::PageDown => app.move_selection(10),
            KeyCode::Char(c) => {
                app.search.push(c);
                app.proc_selected = 0;
            }
            _ => {}
        }
        return;
    }

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
        KeyCode::Char('/') if app.tab == app::Tab::Processes => app.searching = true,
        KeyCode::Tab | KeyCode::Right => app.next_tab(),
        KeyCode::BackTab | KeyCode::Left => app.prev_tab(),
        KeyCode::Char(c @ '1'..='5') => app.select((c as u8 - b'0') as usize),
        // Process-table selection.
        KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
        KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
        KeyCode::PageUp => app.move_selection(-10),
        KeyCode::PageDown => app.move_selection(10),
        KeyCode::Home | KeyCode::Char('g') => app.select_first(),
        KeyCode::End | KeyCode::Char('G') => app.select_last(),
        // Sort the process table.
        KeyCode::Char('s') if app.tab == app::Tab::Processes => app.cycle_sort(),
        KeyCode::Char('r') if app.tab == app::Tab::Processes => app.toggle_sort_dir(),
        // Open the action menu for the selected process.
        KeyCode::Enter if app.tab == app::Tab::Processes => app.open_menu(),
        _ => {}
    }
}
