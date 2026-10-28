use peroxide::{display_path, is_key_file, App, InputMode, Notice, SshConnection, KEY_FIELD};
use ratatui::{
    layout::{Constraint, Layout, Margin, Rect},
    prelude::*,
    widgets::*,
};
use std::path::Path;

/// One color theme. `raised` backs the selected row and the focused form field;
/// `rule` draws lines and dims the screen behind dialogs.
pub struct Theme {
    pub name: &'static str,
    bg: Color,
    raised: Color,
    rule: Color,
    muted: Color,
    text: Color,
    accent: Color,
    ok: Color,
    err: Color,
}

/// Saved by name, so add new themes anywhere but don't rename old ones.
/// Secondary text is kept at 4.5:1 contrast or better, so some `muted`s are a touch lighter
/// than the theme's own comment color.
pub const THEMES: &[Theme] = &[
    // Your terminal's own palette, so peroxide matches whatever you already use, light or dark.
    Theme {
        name: "Terminal",
        bg: Color::Reset,
        raised: Color::Reset,
        rule: Color::DarkGray,
        muted: Color::DarkGray,
        text: Color::Reset,
        accent: Color::Cyan,
        ok: Color::Green,
        err: Color::Red,
    },
    Theme {
        name: "Catppuccin Mocha",
        bg: Color::from_u32(0x1e1e2e),
        raised: Color::from_u32(0x313244),
        rule: Color::from_u32(0x45475a),
        muted: Color::from_u32(0x9399b2),
        text: Color::from_u32(0xcdd6f4),
        accent: Color::from_u32(0xcba6f7),
        ok: Color::from_u32(0xa6e3a1),
        err: Color::from_u32(0xf38ba8),
    },
    Theme {
        name: "Catppuccin Latte",
        bg: Color::from_u32(0xeff1f5),
        raised: Color::from_u32(0xccd0da),
        rule: Color::from_u32(0xacb0be),
        muted: Color::from_u32(0x6c6f85),
        text: Color::from_u32(0x4c4f69),
        accent: Color::from_u32(0x8839ef),
        ok: Color::from_u32(0x40a02b),
        err: Color::from_u32(0xd20f39),
    },
    Theme {
        name: "Tokyo Night",
        bg: Color::from_u32(0x1a1b26),
        raised: Color::from_u32(0x292e42),
        rule: Color::from_u32(0x3b4261),
        muted: Color::from_u32(0x7d84ad),
        text: Color::from_u32(0xc0caf5),
        accent: Color::from_u32(0x7aa2f7),
        ok: Color::from_u32(0x9ece6a),
        err: Color::from_u32(0xf7768e),
    },
    Theme {
        name: "Gruvbox Dark",
        bg: Color::from_u32(0x282828),
        raised: Color::from_u32(0x3c3836),
        rule: Color::from_u32(0x504945),
        muted: Color::from_u32(0xa89984),
        text: Color::from_u32(0xebdbb2),
        accent: Color::from_u32(0xfabd2f),
        ok: Color::from_u32(0xb8bb26),
        err: Color::from_u32(0xfb4934),
    },
    Theme {
        name: "Nord",
        bg: Color::from_u32(0x2e3440),
        raised: Color::from_u32(0x3b4252),
        rule: Color::from_u32(0x4c566a),
        muted: Color::from_u32(0x949eb0),
        text: Color::from_u32(0xd8dee9),
        accent: Color::from_u32(0x88c0d0),
        ok: Color::from_u32(0xa3be8c),
        err: Color::from_u32(0xbf616a),
    },
    Theme {
        name: "Dracula",
        bg: Color::from_u32(0x282a36),
        raised: Color::from_u32(0x44475a),
        rule: Color::from_u32(0x6272a4),
        muted: Color::from_u32(0x8a94c8),
        text: Color::from_u32(0xf8f8f2),
        accent: Color::from_u32(0xbd93f9),
        ok: Color::from_u32(0x50fa7b),
        err: Color::from_u32(0xff5555),
    },
    Theme {
        name: "Rosé Pine",
        bg: Color::from_u32(0x191724),
        raised: Color::from_u32(0x26233a),
        rule: Color::from_u32(0x403d52),
        muted: Color::from_u32(0x908caa),
        text: Color::from_u32(0xe0def4),
        accent: Color::from_u32(0xebbcba),
        ok: Color::from_u32(0x9ccfd8),
        err: Color::from_u32(0xeb6f92),
    },
    // A brown peroxide bottle with something fizzing in it.
    Theme {
        name: "Peroxide",
        bg: Color::from_u32(0x17110d),
        raised: Color::from_u32(0x2b2017),
        rule: Color::from_u32(0x524132),
        muted: Color::from_u32(0x9c8873),
        text: Color::from_u32(0xeee4d2),
        accent: Color::from_u32(0xf0a04b),
        ok: Color::from_u32(0x9ee6df),
        err: Color::from_u32(0xf2626b),
    },
];

pub fn theme_index(name: &str) -> usize {
    THEMES.iter().position(|t| t.name == name).unwrap_or(0)
}

/// The saved theme, or the one under the cursor while picking.
fn theme(app: &App) -> &'static Theme {
    let index = if app.input_mode == InputMode::Themes {
        app.theme_cursor
    } else {
        theme_index(&app.settings.theme)
    };
    &THEMES[index.min(THEMES.len() - 1)]
}

/// A bubble rising and popping, one frame per tick.
const BUBBLE: [&str; 6] = [".", "∘", "°", "˚", " ", " "];

pub fn draw(f: &mut Frame, app: &App) {
    let t = theme(app);
    let area = f.area();
    f.buffer_mut()
        .set_style(area, Style::new().fg(t.text).bg(t.bg));
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(2),
    ])
    .areas(area);

    header(f, app, top);
    if area.width >= 90 && !app.connections.is_empty() {
        let [list, side] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Percentage(42)]).areas(body);
        server_list(f, app, list);
        details(f, app, side);
    } else {
        server_list(f, app, body);
    }
    footer(f, app, bottom);

    // The theme picker leaves the screen undimmed so the preview shows on real content.
    if !matches!(
        app.input_mode,
        InputMode::Normal | InputMode::Search | InputMode::Themes
    ) {
        f.buffer_mut().set_style(
            area,
            Style::new().fg(t.rule).remove_modifier(Modifier::BOLD),
        );
    }
    match app.input_mode {
        InputMode::Adding | InputMode::Editing => form(f, app),
        InputMode::ConfirmDelete => confirm_delete(f, app),
        InputMode::Keys => keys(f, app),
        InputMode::FileBrowser => file_browser(f, app),
        InputMode::Themes => theme_picker(f, app),
        InputMode::Normal | InputMode::Search => {}
    }
}

fn header(f: &mut Frame, app: &App, area: Rect) {
    let t = theme(app);
    let block = Block::new()
        .borders(Borders::BOTTOM)
        .border_style(t.rule)
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut brand = vec![
        Span::styled("°∘ ", t.ok),
        Span::styled("peroxide", Style::new().fg(t.accent).bold()),
    ];
    let total = app.connections.len();
    let count = if app.input_mode == InputMode::Search {
        brand.push(Span::styled("    / ", t.accent));
        brand.push(Span::raw(app.search_query.as_str()));
        let x = inner.x + Line::from(brand.clone()).width() as u16;
        f.set_cursor_position((x, inner.y));
        format!("{} of {}", app.filtered_indices().len(), total)
    } else {
        plural(total, "server")
    };
    f.render_widget(Line::from(brand), inner);
    // Style the span, not the line: a styled Line restyles the whole row under it.
    f.render_widget(
        Line::from(Span::styled(count, t.muted)).right_aligned(),
        inner,
    );
}

/// The connection under the cursor, accounting for the search filter.
fn highlighted(app: &App) -> Option<(usize, &SshConnection)> {
    let idx = if app.input_mode == InputMode::Search {
        app.search_selected
            .and_then(|sel| app.filtered_indices().get(sel).copied())
    } else {
        app.selected_connection
    }?;
    app.connections.get(idx).map(|conn| (idx, conn))
}

fn server_list(f: &mut Frame, app: &App, area: Rect) {
    let t = theme(app);
    let area = area.inner(Margin::new(0, 1));
    if app.connections.is_empty() {
        return empty_state(f, t, area);
    }
    let indices = app.filtered_indices();
    if indices.is_empty() {
        let msg = Line::styled(format!("Nothing matches “{}”", app.search_query), t.muted);
        return f.render_widget(
            msg.centered(),
            area.centered_vertically(Constraint::Length(1)),
        );
    }

    let selected = if app.input_mode == InputMode::Search {
        app.search_selected
    } else {
        app.selected_connection
    };
    let checking = app.pending.as_ref().map(|p| p.idx);
    let name_width = indices
        .iter()
        .map(|&i| app.connections[i].name.chars().count())
        .max()
        .unwrap_or(0)
        .clamp(8, 28) as u16;

    let rows = indices.iter().enumerate().map(|(pos, &i)| {
        let conn = &app.connections[i];
        let status = if checking == Some(i) {
            Span::styled(bubbles(app, 1), t.accent)
        } else {
            status_glyph(t, conn.last_connection_status)
        };
        let name = if Some(pos) == selected {
            Span::styled(conn.name.as_str(), Style::new().fg(t.text).bold())
        } else {
            Span::raw(conn.name.as_str())
        };
        let forward = conn
            .local_forward
            .as_deref()
            .and_then(|fwd| fwd.split(':').next())
            .map(|port| format!("⇄ {port}"))
            .unwrap_or_default();
        Row::new([
            Line::from(status),
            Line::from(name),
            Line::styled(conn.address(), t.muted),
            Line::styled(forward, t.muted),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(1),
            Constraint::Length(name_width),
            Constraint::Fill(1),
            Constraint::Length(7),
        ],
    )
    .column_spacing(2);
    render_selectable(f, t, table, area, selected);
}

/// Renders a table with the amber selection bar used across the app.
fn render_selectable(f: &mut Frame, t: &Theme, table: Table, area: Rect, selected: Option<usize>) {
    let table = table
        .row_highlight_style(Style::new().bg(t.raised))
        .highlight_symbol(Line::styled("▌ ", t.accent))
        .highlight_spacing(HighlightSpacing::Always);
    f.render_stateful_widget(table, area, &mut TableState::new().with_selected(selected));
}

fn empty_state(f: &mut Frame, t: &Theme, area: Rect) {
    let glass = |s: &'static str| Line::styled(s, t.accent);
    let lines = vec![
        Line::styled("  ˚ °   ", t.ok),
        Line::styled("   ∘    ", t.ok),
        glass("  ╭──╮  "),
        glass("  │  │  "),
        glass("╭─╯  ╰─╮"),
        Line::from(vec![
            Span::styled("│ ", t.accent),
            Span::styled("H₂O₂", Style::new().fg(t.text).bold()),
            Span::styled(" │", t.accent),
        ]),
        glass("│      │"),
        glass("╰──────╯"),
        Line::default(),
        Line::styled("No servers yet", Style::new().fg(t.text).bold()),
        Line::from(vec![
            Span::styled("Press ", t.muted),
            Span::styled("a", t.accent),
            Span::styled(" to add one.", t.muted),
        ]),
    ];
    let height = lines.len() as u16;
    f.render_widget(
        Paragraph::new(lines).centered(),
        area.centered_vertically(Constraint::Length(height)),
    );
}

fn details(f: &mut Frame, app: &App, area: Rect) {
    let t = theme(app);
    let block = Block::new()
        .borders(Borders::LEFT)
        .border_style(t.rule)
        .padding(Padding::new(3, 2, 1, 1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some((idx, conn)) = highlighted(app) else {
        return;
    };

    let label = |s: &str| Span::styled(format!("{s:<10}"), t.muted);
    let mut lines = vec![
        Line::styled(conn.name.as_str(), Style::new().fg(t.text).bold()),
        Line::styled(conn.address(), t.muted),
        Line::default(),
    ];

    lines.push(match (&conn.key_path, &conn.password) {
        (Some(key), _) => Line::from(vec![label("Key"), Span::raw(display_path(key))]),
        (None, Some(_)) => Line::from(vec![label("Auth"), Span::raw("saved password")]),
        (None, None) => Line::from(vec![label("Auth"), Span::raw("ssh-agent")]),
    });
    if let Some(fwd) = &conn.local_forward {
        let (local, remote) = fwd.split_once(':').unwrap_or((fwd, ""));
        lines.push(Line::from(vec![
            label("Forward"),
            Span::raw(format!("{local} → {remote}")),
        ]));
    }
    let status = match (app.pending.as_ref(), conn.last_connection_status) {
        (Some(p), _) if p.idx == idx => {
            Span::styled(format!("{} checking", bubbles(app, 3)), t.accent)
        }
        (_, Some(true)) => Span::styled("● signed in", t.ok),
        (_, Some(false)) => Span::styled("✕ failed", t.err),
        (_, None) => Span::styled("not checked yet", t.muted),
    };
    lines.push(Line::from(vec![label("Status"), status]));
    lines.push(Line::default());

    // One flag per line, shell-continued, so it reads well and still pastes.
    let args = conn.ssh_args();
    let (dest, flags) = args
        .split_last()
        .expect("ssh_args ends with the destination");
    let mut parts: Vec<String> = flags
        .chunks(2)
        .map(|pair| {
            pair.iter()
                .map(|a| display_path(Path::new(a)))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    parts.push(dest.clone());
    let last = parts.len() - 1;
    for (i, part) in parts.into_iter().enumerate() {
        lines.push(Line::from(vec![
            Span::styled(if i == 0 { "$ " } else { "  " }, t.accent),
            Span::styled(if i == 0 { "ssh " } else { "    " }, t.muted),
            Span::styled(part, t.muted),
            Span::styled(if i == last { "" } else { " \\" }, t.rule),
        ]));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn footer(f: &mut Frame, app: &App, area: Rect) {
    let t = theme(app);
    let block = Block::new()
        .borders(Borders::TOP)
        .border_style(t.rule)
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let hints: &[(&str, &str)] = match app.input_mode {
        InputMode::Search => &[("enter", "select"), ("↑↓", "move"), ("esc", "clear")],
        _ if app.connections.is_empty() => &[
            ("a", "add server"),
            ("s", "keys"),
            ("T", "theme"),
            ("q", "quit"),
        ],
        _ => &[
            ("enter", "connect"),
            ("a", "add"),
            ("e", "edit"),
            ("t", "test"),
            ("/", "filter"),
            ("d", "delete"),
            ("y", "duplicate"),
            ("s", "keys"),
            ("T", "theme"),
            ("⇧↑↓", "reorder"),
            ("q", "quit"),
        ],
    };
    if matches!(app.input_mode, InputMode::Normal | InputMode::Search) {
        status_line(f, app, hints, inner);
    }
}

/// What's happening right now, a message from the last action, or the keys for this screen.
fn status_line(f: &mut Frame, app: &App, hints: &[(&str, &str)], area: Rect) {
    let t = theme(app);
    let line = if let Some(pending) = &app.pending {
        let name = app
            .connections
            .get(pending.idx)
            .map_or("", |c| c.name.as_str());
        let verb = if pending.connect {
            "Connecting to"
        } else {
            "Checking"
        };
        Line::from(vec![
            Span::styled(bubbles(app, 3), t.ok),
            Span::raw(format!(" {verb} {name}   ")),
            Span::styled("esc", t.accent),
            Span::styled(" cancel", t.muted),
        ])
    } else {
        match &app.notice {
            Some(Notice::Info(msg)) => Line::from(vec![Span::styled("● ", t.ok), Span::raw(msg)]),
            Some(Notice::Error(msg)) => Line::from(vec![Span::styled("✕ ", t.err), Span::raw(msg)]),
            None => {
                // Drop whole hints that don't fit rather than cutting one in half.
                let mut line = Line::default();
                for (i, (key, action)) in hints.iter().enumerate() {
                    let hint = [
                        Span::raw(if i == 0 { "" } else { "   " }),
                        Span::styled(key.to_string(), t.accent),
                        Span::styled(format!(" {action}"), t.muted),
                    ];
                    let width: usize = hint.iter().map(Span::width).sum();
                    if line.width() + width > area.width as usize {
                        break;
                    }
                    line.spans.extend(hint);
                }
                line
            }
        }
    };
    f.render_widget(line, area);
}

/// A centered modal; returns its content area and the row for its status line.
fn modal(
    f: &mut Frame,
    t: &Theme,
    title: String,
    color: Color,
    width: u16,
    height: u16,
) -> (Rect, Rect) {
    let screen = f.area();
    let area = screen.centered(
        Constraint::Length(width.min(screen.width.saturating_sub(2))),
        Constraint::Length(height.min(screen.height)),
    );
    modal_at(f, t, title, color, area)
}

fn modal_at(f: &mut Frame, t: &Theme, title: String, color: Color, area: Rect) -> (Rect, Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(color)
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(color).bold(),
        ))
        .padding(Padding::new(2, 2, 1, 1))
        .style(Style::new().fg(t.text).bg(t.bg));
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);
    let [content, _, status] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);
    (content, status)
}

fn form(f: &mut Frame, app: &App) {
    let t = theme(app);
    let form = &app.form_state;
    let title = match (&app.input_mode, app.selected()) {
        (InputMode::Editing, Some(conn)) => format!("Edit {}", conn.name),
        _ => "New server".to_string(),
    };
    let (content, status) = modal(f, t, title, t.accent, 68, 20);

    let key = match form.selected_key {
        0 => String::new(),
        i => app
            .ssh_keys
            .get(i - 1)
            .map(|p| display_path(p))
            .unwrap_or_default(),
    };
    let mask = |s: &str| "•".repeat(s.chars().count());
    // (label, value, placeholder, hint)
    let fields = [
        (
            "Name",
            form.name.clone(),
            "e.g. prod-db",
            "What this server is called in your list.",
        ),
        (
            "Host",
            form.host.clone(),
            "e.g. example.com or 10.0.0.5",
            "The address to connect to.",
        ),
        (
            "Port",
            form.port.clone(),
            "required",
            "The SSH port. Almost always 22.",
        ),
        (
            "User",
            form.username.clone(),
            "e.g. root",
            "The account to log in as.",
        ),
        (
            "Key",
            key,
            "none",
            "Use ←/→ to pick a key. With none, peroxide uses the password or your ssh-agent.",
        ),
        (
            "Passphrase",
            mask(&form.key_passphrase),
            "only if the key has one",
            "Typed for you when the key asks. Needs sshpass.",
        ),
        (
            "Password",
            mask(&form.password),
            "used when there's no key",
            "Typed for you at login. Needs sshpass.",
        ),
        (
            "Forward",
            form.local_forward.clone(),
            "local_port:host:remote_port",
            "Optional port forward, like 8080:localhost:80.",
        ),
    ];

    let mut y = content.y;
    for (i, (label, value, placeholder, _)) in fields.iter().enumerate() {
        if i == KEY_FIELD || i == fields.len() - 1 {
            y += 1;
        }
        let row = Rect::new(content.x, y, content.width, 1).intersection(content);
        y += 1;
        let focused = form.active_field == i;
        let [label_area, value_area] =
            Layout::horizontal([Constraint::Length(12), Constraint::Fill(1)]).areas(row);
        let label_style = if focused {
            Style::new().fg(t.accent).bold()
        } else {
            Style::new().fg(t.muted)
        };
        f.render_widget(Span::styled(*label, label_style), label_area);
        if focused {
            f.buffer_mut()
                .set_style(value_area, Style::new().bg(t.raised));
        }
        let value_area = value_area.inner(Margin::new(1, 0));

        if i == KEY_FIELD {
            let value = Span::styled(
                if value.is_empty() {
                    *placeholder
                } else {
                    value.as_str()
                },
                if value.is_empty() { t.muted } else { t.text },
            );
            let line = if focused {
                Line::from(vec![
                    Span::styled("‹ ", t.accent),
                    value,
                    Span::styled(" ›", t.accent),
                ])
            } else {
                Line::from(value)
            };
            f.render_widget(line, value_area);
            if focused {
                let count = format!("{} of {}", form.selected_key + 1, app.ssh_keys.len() + 1);
                f.render_widget(
                    Line::from(Span::styled(count, t.muted)).right_aligned(),
                    value_area,
                );
            }
            continue;
        }

        // Keep the end of long values in view, where the cursor is.
        let fit = value_area.width.saturating_sub(1) as usize;
        let visible: String = value
            .chars()
            .skip(value.chars().count().saturating_sub(fit))
            .collect();
        if visible.is_empty() {
            f.render_widget(Span::styled(*placeholder, t.rule), value_area);
        } else {
            f.render_widget(Span::raw(visible.as_str()), value_area);
        }
        if focused {
            let x = value_area.x + Span::raw(visible.as_str()).width() as u16;
            f.set_cursor_position((x, value_area.y));
        }
    }

    let hint = fields.get(form.active_field).map_or("", |field| field.3);
    let hint_area = Rect::new(content.x, y + 1, content.width, 2).intersection(content);
    f.render_widget(
        Paragraph::new(hint).fg(t.muted).wrap(Wrap { trim: true }),
        hint_area,
    );

    let mut hints = vec![("enter", "save"), ("tab", "next")];
    if form.active_field == KEY_FIELD {
        hints.push(("←→", "pick key"));
    }
    hints.push(("esc", "cancel"));
    status_line(f, app, &hints, status);
}

fn confirm_delete(f: &mut Frame, app: &App) {
    let t = theme(app);
    let Some(conn) = app.selected() else {
        return;
    };
    let (content, status) = modal(f, t, "Delete server".to_string(), t.err, 60, 9);
    let text = vec![
        Line::from(vec![
            Span::raw("Delete "),
            Span::styled(conn.name.as_str(), Style::new().bold()),
            Span::raw("?"),
        ]),
        Line::styled(
            format!(
                "{} will be removed from your list. The server and your keys stay as they are.",
                conn.address()
            ),
            t.muted,
        ),
    ];
    f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), content);
    status_line(f, app, &[("y", "delete"), ("esc", "keep it")], status);
}

fn keys(f: &mut Frame, app: &App) {
    let t = theme(app);
    let shown = app.ssh_keys.len().clamp(1, 14) as u16;
    let (content, status) = modal(f, t, "SSH keys".to_string(), t.accent, 72, shown + 8);
    let [list, _, note] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(content);

    if app.ssh_keys.is_empty() {
        f.render_widget(Line::styled("No keys yet.", t.muted), list);
    } else {
        let rows = app
            .ssh_keys
            .iter()
            .map(|path| Row::new([Line::raw(display_path(path))]));
        let table = Table::new(rows, [Constraint::Fill(1)]);
        render_selectable(f, t, table, list, Some(app.keys_selected));
    }
    f.render_widget(
        Line::styled("Keys in ~/.ssh are picked up automatically.", t.muted),
        note,
    );
    let hints = [
        ("a", "add key"),
        ("f", "add folder"),
        ("d", "remove"),
        ("esc", "close"),
    ];
    status_line(f, app, &hints, status);
}

fn file_browser(f: &mut Frame, app: &App) {
    let t = theme(app);
    let Some(browser) = &app.file_browser else {
        return;
    };
    let title = if browser.pick_folder {
        "Add a folder of keys"
    } else {
        "Add a key"
    };
    let height = f.area().height.saturating_sub(4);
    let (content, status) = modal(f, t, title.to_string(), t.accent, 72, height);
    let [path, _, list] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(content);

    f.render_widget(
        Line::styled(
            display_path(&browser.current_path),
            Style::new().fg(t.text).bold(),
        ),
        path,
    );
    let rows = browser.entries.iter().map(|entry| {
        let name = entry.file_name().unwrap_or_default().to_string_lossy();
        let line = if *entry == browser.current_path {
            Line::styled("+ Use this folder", Style::new().fg(t.accent).bold())
        } else if entry.ends_with("..") {
            Line::styled("../", t.muted)
        } else if entry.is_dir() {
            Line::raw(format!("{name}/"))
        } else if is_key_file(entry) {
            Line::raw(name.into_owned())
        } else {
            Line::styled(name.into_owned(), t.rule)
        };
        Row::new([line])
    });
    render_selectable(
        f,
        t,
        Table::new(rows, [Constraint::Fill(1)]),
        list,
        Some(browser.selected),
    );

    let open = if browser.pick_folder {
        "open or use"
    } else {
        "open or add"
    };
    let hints = [("enter", open), ("←", "parent folder"), ("esc", "back")];
    status_line(f, app, &hints, status);
}

fn theme_picker(f: &mut Frame, app: &App) {
    let t = theme(app);
    // Off to the right rather than centered, so the list stays in view as you preview.
    let screen = f.area();
    let width = 44.min(screen.width);
    let height = (THEMES.len() as u16 + 6).min(screen.height.saturating_sub(3));
    let x = screen.right().saturating_sub(width + 2).max(screen.x);
    let area = Rect::new(x, screen.y + 3, width, height).intersection(screen);
    let (content, status) = modal_at(f, t, "Theme".to_string(), t.accent, area);

    let rows = THEMES.iter().map(|theme| {
        let dot = |color: Color| Span::styled("● ", color);
        Row::new([
            Line::raw(theme.name),
            Line::from(vec![
                dot(theme.accent),
                dot(theme.ok),
                dot(theme.err),
                dot(theme.muted),
            ]),
        ])
    });
    let table = Table::new(rows, [Constraint::Fill(1), Constraint::Length(8)]);
    render_selectable(f, t, table, content, Some(app.theme_cursor));
    let hints = [("enter", "use"), ("↑↓", "preview"), ("esc", "cancel")];
    status_line(f, app, &hints, status);
}

fn status_glyph(t: &Theme, status: Option<bool>) -> Span<'static> {
    match status {
        Some(true) => Span::styled("●", t.ok),
        Some(false) => Span::styled("✕", t.err),
        None => Span::styled("○", t.rule),
    }
}

/// `n` cells of fizz, each a few frames apart so they don't pop together.
fn bubbles(app: &App, n: usize) -> String {
    let tick = app
        .pending
        .as_ref()
        .map_or(0, |p| p.started.elapsed().as_millis() as usize / 110);
    (0..n)
        .map(|cell| BUBBLE[(tick + cell * 2) % BUBBLE.len()])
        .collect()
}

fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::draw;
    use peroxide::{App, InputMode, SshConnection};
    use ratatui::{backend::TestBackend, Terminal};
    use std::path::PathBuf;

    #[test]
    fn every_screen_renders_at_any_size() {
        let mut app = App::new();
        app.connections.push(SshConnection {
            name: "prod-db".into(),
            host: "10.0.3.12".into(),
            port: 2222,
            username: "deploy".into(),
            password: None,
            key_path: Some(PathBuf::from("/keys/id_ed25519")),
            key_passphrase: None,
            local_forward: Some("8080:localhost:80".into()),
            last_connection_status: Some(true),
        });
        app.selected_connection = Some(0);
        app.edit_connection();
        app.file_browser = Some(peroxide::FileBrowser::new(std::env::temp_dir(), true));

        use InputMode::*;
        for mode in [
            Normal,
            Search,
            Adding,
            Editing,
            ConfirmDelete,
            Keys,
            FileBrowser,
            Themes,
        ] {
            app.input_mode = mode;
            for (w, h) in [(1, 1), (12, 4), (40, 10), (80, 24), (140, 45)] {
                let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
                t.draw(|f| draw(f, &app)).unwrap();
            }
        }

        for cursor in 0..super::THEMES.len() {
            app.theme_cursor = cursor;
            let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
            t.draw(|f| draw(f, &app)).unwrap();
        }

        app.input_mode = Normal;
        let mut t = Terminal::new(TestBackend::new(110, 30)).unwrap();
        t.draw(|f| draw(f, &app)).unwrap();
        let screen: String = t
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        for expected in [
            "peroxide",
            "deploy@10.0.3.12:2222",
            "-L 8080:localhost:80",
            "enter connect",
        ] {
            assert!(screen.contains(expected), "missing {expected:?}");
        }
    }
}
