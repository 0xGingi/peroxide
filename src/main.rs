use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use peroxide::{App, AppError, InputMode, SshConnection, KEY_FIELD};
use ratatui::DefaultTerminal;
use std::io::{self, ErrorKind};
use std::time::Duration;

mod ui;

fn main() -> Result<()> {
    let mut app = App::new();
    // Refuse to start on a corrupt file rather than overwrite it with an empty list on quit.
    app.connections = App::load_connections().context("Couldn't read saved connections")?;
    // A bad settings file only costs the theme choice, so don't refuse to start over it.
    app.settings = App::load_settings().unwrap_or_default();
    for key in App::load_additional_keys().unwrap_or_default() {
        app.add_key_path(key);
    }
    if !app.connections.is_empty() {
        app.selected_connection = Some(0);
    }

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // While a check runs, wake up regularly to animate it and pick up its result.
        if app.pending.is_none() || event::poll(Duration::from_millis(90))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && handle_key(app, key) {
                    return app.save_connections();
                }
            }
        }

        if let Some((idx, connect, result)) = app.poll_check() {
            let name = app
                .connections
                .get(idx)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            match result {
                Ok(()) if connect => {
                    let conn = app.connections[idx].clone();
                    run_ssh(terminal, app, &conn)?;
                }
                Ok(()) => app.info(format!("Signed in to {name}")),
                Err(AppError::ConnectionFailed(e)) => {
                    app.error(format!("Couldn't reach {name}: {e}"))
                }
                Err(AppError::AuthenticationFailed(e)) => {
                    app.error(format!("Sign-in to {name} failed: {e}"))
                }
            }
        }
    }
}

/// Hands the terminal to ssh until it exits.
fn run_ssh(terminal: &mut DefaultTerminal, app: &mut App, conn: &SshConnection) -> Result<()> {
    let mut cmd = conn.command();
    ratatui::restore();
    let status = cmd.status();
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    terminal.clear()?;

    match status {
        Ok(status) if status.success() => app.info(format!("Disconnected from {}", conn.name)),
        Ok(status) => app.info(format!(
            "Disconnected from {} (exit code {})",
            conn.name,
            status.code().map_or("none".to_string(), |c| c.to_string())
        )),
        Err(e) if e.kind() == ErrorKind::NotFound => app.error(format!(
            "{} isn't installed. It's needed to connect to {}.",
            cmd.get_program().to_string_lossy(),
            conn.name
        )),
        Err(e) => app.error(format!("Couldn't start ssh: {e}")),
    }
    Ok(())
}

/// Saves connections, reporting `done` on success.
fn persist(app: &mut App, done: Option<String>) {
    match app.save_connections() {
        Ok(()) => {
            if let Some(done) = done {
                app.info(done);
            }
        }
        Err(e) => app.error(format!("Couldn't save: {e}")),
    }
}

/// Returns true when the app should quit.
fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return true;
    }
    app.notice = None;

    if app.pending.is_some() {
        if key.code == KeyCode::Esc {
            app.pending = None;
            app.info("Cancelled");
        }
        return key.code == KeyCode::Char('q');
    }

    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    match app.input_mode {
        InputMode::Normal => match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Up if shift => {
                app.move_selected_up();
                persist(app, None);
            }
            KeyCode::Down if shift => {
                app.move_selected_down();
                persist(app, None);
            }
            KeyCode::Char('K') => {
                app.move_selected_up();
                persist(app, None);
            }
            KeyCode::Char('J') => {
                app.move_selected_down();
                persist(app, None);
            }
            KeyCode::Up | KeyCode::Char('k') => app.move_selection(false),
            KeyCode::Down | KeyCode::Char('j') => app.move_selection(true),
            KeyCode::Enter | KeyCode::Char('c') => app.start_check(true),
            KeyCode::Char('t') => app.start_check(false),
            KeyCode::Char('a') => app.start_adding(),
            KeyCode::Char('e') => app.edit_connection(),
            KeyCode::Char('y') => {
                if let Some(name) = app.duplicate_selected() {
                    persist(app, Some(format!("Created {name}")));
                }
            }
            KeyCode::Char('d') if app.selected().is_some() => {
                app.input_mode = InputMode::ConfirmDelete
            }
            KeyCode::Char('/') => app.start_search(),
            KeyCode::Char('s') => app.input_mode = InputMode::Keys,
            KeyCode::Char('T') => {
                app.theme_cursor = ui::theme_index(&app.settings.theme);
                app.input_mode = InputMode::Themes;
            }
            _ => {}
        },
        InputMode::Themes => {
            let count = ui::THEMES.len();
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    app.theme_cursor = (app.theme_cursor + count - 1) % count
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.theme_cursor = (app.theme_cursor + 1) % count
                }
                KeyCode::Enter => {
                    let name = ui::THEMES[app.theme_cursor].name;
                    app.settings.theme = name.to_string();
                    app.input_mode = InputMode::Normal;
                    match app.save_settings() {
                        Ok(()) => app.info(format!("Theme set to {name}")),
                        Err(e) => app.error(format!("Couldn't save theme: {e}")),
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('T') => {
                    app.input_mode = InputMode::Normal
                }
                _ => {}
            }
        }
        InputMode::Search => match key.code {
            KeyCode::Esc => app.end_search(false),
            KeyCode::Enter => app.end_search(true),
            KeyCode::Up => app.move_selection(false),
            KeyCode::Down => app.move_selection(true),
            KeyCode::Backspace => app.edit_search(None),
            KeyCode::Char(c) => app.edit_search(Some(c)),
            _ => {}
        },
        InputMode::Adding | InputMode::Editing => match key.code {
            KeyCode::Esc => app.input_mode = InputMode::Normal,
            KeyCode::Tab | KeyCode::Down => app.next_field(),
            KeyCode::BackTab | KeyCode::Up => app.previous_field(),
            KeyCode::Left if app.form_state.active_field == KEY_FIELD => app.select_ssh_key(-1),
            KeyCode::Right if app.form_state.active_field == KEY_FIELD => app.select_ssh_key(1),
            KeyCode::Backspace => app.delete_char(),
            KeyCode::Enter => match app.submit_form() {
                Ok(name) => persist(app, Some(format!("Saved {name}"))),
                Err(e) => app.error(e),
            },
            KeyCode::Char(c) => app.add_char(c),
            _ => {}
        },
        InputMode::ConfirmDelete => {
            app.input_mode = InputMode::Normal;
            if key.code == KeyCode::Char('y') {
                if let Some(conn) = app.delete_selected() {
                    persist(app, Some(format!("Deleted {}", conn.name)));
                }
            }
        }
        InputMode::Keys => match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('s') => {
                app.input_mode = InputMode::Normal
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.keys_selected = app.keys_selected.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if app.keys_selected + 1 < app.ssh_keys.len() {
                    app.keys_selected += 1;
                }
            }
            KeyCode::Char('a') => app.open_file_browser(false),
            KeyCode::Char('f') => app.open_file_browser(true),
            KeyCode::Char('d') | KeyCode::Delete => {
                app.remove_ssh_key(app.keys_selected);
                if let Err(e) = app.save_additional_keys() {
                    app.error(format!("Couldn't save keys: {e}"));
                }
            }
            _ => {}
        },
        InputMode::FileBrowser => {
            let Some(browser) = &mut app.file_browser else {
                return false;
            };
            match key.code {
                KeyCode::Esc => app.close_file_browser(),
                KeyCode::Up | KeyCode::Char('k') => browser.move_up(),
                KeyCode::Down | KeyCode::Char('j') => browser.move_down(),
                KeyCode::Left | KeyCode::Backspace | KeyCode::Char('h') => browser.go_up(),
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => app.browser_select(),
                _ => {}
            }
        }
    }
    false
}
