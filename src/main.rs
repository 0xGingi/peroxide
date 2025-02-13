use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use std::io;
use peroxide::{App, AppError, FormState, InputMode, SettingsTab};

fn main() -> Result<()> {
    let mut terminal = setup_terminal()?;
    let mut app = App::new();
    
    if let Ok(connections) = App::load_connections() {
        app.connections = connections;
    }
    
    run(&mut terminal, app)?;
    restore_terminal(&mut terminal)?;
    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, mut app: App) -> Result<()> {
    if let Ok(additional_keys) = App::load_additional_keys() {
        for key in additional_keys {
            app.add_key_path(key);
        }
    }

    loop {
        terminal.draw(|f| ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            app.clear_error();
            
            match app.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('q') => {
                        app.save_connections()?;
                        return Ok(());
                    }
                    KeyCode::Char('a') => {
                        app.input_mode = InputMode::Adding;
                        app.form_state = FormState::new();
                    }
                    KeyCode::Char('e') => {
                        app.edit_connection();
                    }
                    KeyCode::Char('d') => {
                        app.delete_connection();
                        app.save_connections()?;
                    }
                    KeyCode::Char('y') => {
                        if let Err(e) = app.duplicate_connection() {
                            app.show_error(e);
                        } else {
                            app.save_connections()?;
                        }
                    }
                    KeyCode::Up => {
                        if let Some(selected) = app.selected_connection {
                            if selected > 0 {
                                app.selected_connection = Some(selected - 1);
                            }
                        } else {
                            app.selected_connection = Some(0);
                        }
                    }
                    KeyCode::Down => {
                        if let Some(selected) = app.selected_connection {
                            if selected < app.connections.len().saturating_sub(1) {
                                app.selected_connection = Some(selected + 1);
                            }
                        } else {
                            app.selected_connection = Some(0);
                        }
                    }
                    KeyCode::Char('c') => {
                        if let Some(idx) = app.selected_connection {
                            match app.test_connection(idx) {
                                Ok(_) => {
                                    match app.execute_ssh() {
                                        Ok(needs_redraw) => {
                                            if needs_redraw {
                                                terminal.clear()?;
                                                terminal.draw(|f| ui(f, &app))?;
                                            }
                                        }
                                        Err(e) => {
                                            app.show_error(format!("Failed to execute SSH: {}", e));
                                        }
                                    }
                                }
                                Err(e) => match e {
                                    AppError::ConnectionFailed(msg) => {
                                        app.show_error(format!("Connection test failed: {}", msg));
                                    }
                                    AppError::AuthenticationFailed(msg) => {
                                        app.show_error(format!("Authentication test failed: {}", msg));
                                    }
                                    AppError::NoConnectionSelected => {
                                        app.show_error("No connection selected");
                                    }
                                },
                            }
                        } else {
                            app.show_error("No connection selected");
                        }
                    }
                    KeyCode::Char('k') => {
                        if let Err(e) = app.select_key_file() {
                            app.show_error(e.to_string());
                        } else {
                            if let Err(e) = app.save_additional_keys() {
                                app.show_error(format!("Failed to save additional keys: {}", e));
                            }
                        }
                    }
                    KeyCode::Char('f') => {
                        if let Err(e) = app.select_key_folder() {
                            app.show_error(e.to_string());
                        } else {
                            if let Err(e) = app.save_additional_keys() {
                                app.show_error(format!("Failed to save additional keys: {}", e));
                            }
                        }
                    }
                    KeyCode::Char('t') => {
                        if let Some(idx) = app.selected_connection {
                            match app.test_connection(idx) {
                                Ok(_) => app.show_error("Connection test successful!"),
                                Err(e) => match e {
                                    AppError::ConnectionFailed(msg) => {
                                        app.show_error(format!("Connection test failed: {}", msg));
                                    }
                                    AppError::AuthenticationFailed(msg) => {
                                        app.show_error(format!("Authentication test failed: {}", msg));
                                    }
                                    AppError::NoConnectionSelected => {
                                        app.show_error("No connection selected");
                                    }
                                },
                            }
                        } else {
                            app.show_error("No connection selected");
                        }
                    }
                    KeyCode::Char('s') => {
                        app.input_mode = InputMode::Settings;
                    }
                    KeyCode::Enter => {
                        if let Some(idx) = app.selected_connection {
                            match app.test_connection(idx) {
                                Ok(_) => {
                                    match app.execute_ssh() {
                                        Ok(needs_redraw) => {
                                            if needs_redraw {
                                                terminal.clear()?;
                                                terminal.draw(|f| ui(f, &app))?;
                                            }
                                        }
                                        Err(e) => {
                                            app.show_error(format!("Failed to execute SSH: {}", e));
                                        }
                                    }
                                }
                                Err(e) => match e {
                                    AppError::ConnectionFailed(msg) => {
                                        app.show_error(format!("Connection test failed: {}", msg));
                                    }
                                    AppError::AuthenticationFailed(msg) => {
                                        app.show_error(format!("Authentication test failed: {}", msg));
                                    }
                                    AppError::NoConnectionSelected => {
                                        app.show_error("No connection selected");
                                    }
                                },
                            }
                        } else {
                            app.show_error("No connection selected");
                        }
                    }
                    _ => {}
                },
                InputMode::Adding | InputMode::Editing => match key.code {
                    KeyCode::Esc => app.input_mode = InputMode::Normal,
                    KeyCode::Tab => app.next_field(),
                    KeyCode::BackTab => app.previous_field(),
                    KeyCode::Backspace => app.delete_char(),
                    KeyCode::Enter => {
                        let result = match app.input_mode {
                            InputMode::Adding => app.save_connection(),
                            InputMode::Editing => app.update_connection(),
                            _ => unreachable!(),
                        };
                        if result.is_ok() {
                            app.save_connections()?;
                            app.input_mode = InputMode::Normal;
                        }
                    }
                    KeyCode::Char(c) => app.add_char(c),
                    KeyCode::Right => {
                        if app.form_state.active_field == 5 {
                            app.select_ssh_key(1)
                        }
                    },
                    KeyCode::Left => {
                        if app.form_state.active_field == 5 {
                            app.select_ssh_key(-1)
                        } else {
                            app.form_state.selected_key = None
                        }
                    },
                    _ => {}
                },
                InputMode::Settings => match key.code {
                    KeyCode::Esc => app.input_mode = InputMode::Normal,
                    KeyCode::Tab => app.next_settings_tab(),
                    KeyCode::Up => {
                        if app.settings_selected_item > 0 {
                            app.settings_selected_item -= 1;
                        }
                    }
                    KeyCode::Down => {
                        app.settings_selected_item += 1;
                    }
                    KeyCode::Enter => {
                        match app.settings_tab {
                            SettingsTab::SshKeys => {
                                match app.settings_selected_item {
                                    0 => if let Err(e) = app.select_key_file() {
                                        app.show_error(e.to_string());
                                    },
                                    1 => if let Err(e) = app.select_key_folder() {
                                        app.show_error(e.to_string());
                                    },
                                    _ => {}
                                }
                                if let Err(e) = app.save_additional_keys() {
                                    app.show_error(format!("Failed to save additional keys: {}", e));
                                }
                            }
                            SettingsTab::General => {
                            }
                        }
                    }
                    _ => {}
                },
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    let title = Paragraph::new("Peroxide - SSH Connection Manager")
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    match app.input_mode {
        InputMode::Normal => render_connections(f, app, chunks[1]),
        InputMode::Adding | InputMode::Editing => render_form(f, app, chunks[1]),
        InputMode::Settings => render_settings(f, app, chunks[1]),
    }

    let help = match app.input_mode {
        InputMode::Normal => "q: Quit | a: Add | e: Edit | d: Delete | y: Duplicate | s: Settings | ↑↓: Navigate",
        InputMode::Adding => "Esc: Cancel | Tab: Next Field | Enter: Save | ←→: Select SSH Key",
        InputMode::Editing => "Esc: Cancel | Tab: Next Field | Enter: Update | ←→: Select SSH Key",
        InputMode::Settings => "Esc: Back | Tab: Switch Tab | ↑↓: Navigate | Enter: Select",
    };

    let help = Paragraph::new(help)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[2]);

    if let Some(error) = &app.error_message {
        let error_message = Paragraph::new(error.as_str())
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center);
        f.render_widget(error_message, chunks[3]);
    }
}

fn render_connections(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .connections
        .iter()
        .map(|conn| {
            let auth_method = if conn.key_path.is_some() {
                "🔑"
            } else if conn.password.is_some() {
                "🔒"
            } else {
                "❌"
            };

            let status = match conn.last_connection_status {
                Some(true) => "✅",
                Some(false) => "❌",
                None => "  ",
            };
            
            ListItem::new(format!(
                "{} {} {} ({}@{}:{})",
                status, auth_method, conn.name, conn.username, conn.host, conn.port
            ))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().title("Connections").borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");

    f.render_stateful_widget(
        list,
        area,
        &mut ListState::default().with_selected(app.selected_connection),
    );
}

fn render_form(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    let form_fields = [
        ("Name", &app.form_state.name),
        ("Host", &app.form_state.host),
        ("Port", &app.form_state.port),
        ("Username", &app.form_state.username),
        ("Password", &app.form_state.password),
    ];

    for (i, (title, content)) in form_fields.iter().enumerate() {
        let style = if app.form_state.active_field == i {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let display_content = if i == 4 && !content.is_empty() {
            "*".repeat(content.len())
        } else {
            content.to_string()
        };

        let input = Paragraph::new(display_content)
            .style(style)
            .block(Block::default().title(*title).borders(Borders::ALL));
        f.render_widget(input, chunks[i]);
    }

    let key_items = app.ssh_keys
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let is_selected = Some(i) == app.form_state.selected_key;
            let file_name = path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let display_text = if is_selected {
                format!("《 {} 》", file_name)
            } else {
                format!("  {}  ", file_name)
            };

            Span::styled(
                display_text,
                if is_selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                }
            )
        })
        .collect::<Vec<_>>();

    let key_text = Line::from(key_items);
    
    let key_paragraph = Paragraph::new(key_text)
        .alignment(Alignment::Center)
        .block(Block::default()
            .title("SSH Key (←→ to select)")
            .borders(Borders::ALL)
            .style(if app.form_state.active_field == 5 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            }));

    f.render_widget(key_paragraph, chunks[5]);
}

fn render_settings(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(80),
        ])
        .split(area);

    let tabs = vec!["SSH Keys", "General"];
    let tabs = Tabs::new(tabs)
        .select(match app.settings_tab {
            SettingsTab::SshKeys => 0,
            SettingsTab::General => 1,
        })
        .block(Block::default().borders(Borders::ALL).title("Settings"))
        .highlight_style(Style::default().fg(Color::Yellow));
    f.render_widget(tabs, chunks[0]);

    match app.settings_tab {
        SettingsTab::SshKeys => {
            let items = vec![
                ListItem::new("Add SSH Key File"),
                ListItem::new("Add SSH Key Folder"),
                ListItem::new("Current SSH Keys:"),
            ];

            let mut key_items: Vec<ListItem> = app.ssh_keys
                .iter()
                .map(|path| {
                    ListItem::new(format!("  {}", 
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    ))
                })
                .collect();

            let mut all_items = items;
            all_items.append(&mut key_items);

            let list = List::new(all_items)
                .block(Block::default().borders(Borders::ALL))
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

            f.render_stateful_widget(
                list,
                chunks[1],
                &mut ListState::default().with_selected(Some(app.settings_selected_item)),
            );
        }
        SettingsTab::General => {
            let paragraph = Paragraph::new("General Settings (Coming Soon)")
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(paragraph, chunks[1]);
        }
    }
} 