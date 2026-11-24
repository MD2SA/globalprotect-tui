use crate::app::{App, Field, Mode, Page, VpnState};
use crate::client::{detect_backend, Backend, BackendPreference};
use crate::config::Profile;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};
use ratatui::Frame;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const SPINNER_ASCII: [&str; 4] = ["|", "/", "-", "\\"];

fn ascii_mode() -> bool {
    std::env::var_os("GP_TUI_ASCII").is_some()
        || std::env::var("TERM")
            .map(|term| term == "linux" || term == "dumb")
            .unwrap_or(false)
}

fn tint(color: Color) -> Color {
    if std::env::var_os("NO_COLOR").is_some() {
        Color::Reset
    } else {
        color
    }
}

fn separator() -> &'static str {
    if ascii_mode() {
        " | "
    } else {
        " · "
    }
}

fn cursor() -> &'static str {
    if ascii_mode() {
        "|"
    } else {
        "█"
    }
}

fn panel(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(if ascii_mode() {
            BorderType::Plain
        } else {
            BorderType::Rounded
        })
        .title(title.to_string())
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < 60 || area.height < 16 {
        let message = format!(
            "Terminal too small ({}x{}).\nMinimum: 60x16.",
            area.width, area.height
        );
        frame.render_widget(Paragraph::new(message).block(panel(" gp-tui ")), area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(8),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_header(frame, app, chunks[0]);

    match app.page {
        Page::Status => draw_status_page(frame, app, chunks[1]),
        Page::Profiles => draw_profiles_page(frame, app, chunks[1]),
        Page::Config => draw_config_page(frame, app, chunks[1]),
        Page::Logs => draw_logs_page(frame, app, chunks[1]),
    }

    draw_footer(frame, app, chunks[2]);

    if app.help {
        draw_help(frame, app);
    }
    if app.quit_modal {
        draw_quit_modal(frame, app);
    }
    if app.profile_delete.is_some() {
        draw_profile_delete_modal(frame, app);
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let tabs_line = Line::from(vec![
        tab_span("Status", app.page == Page::Status),
        Span::raw(" "),
        tab_span(
            "Profiles",
            matches!(app.page, Page::Profiles | Page::Config),
        ),
        Span::raw(" "),
        tab_span("Logs", app.page == Page::Logs),
        Span::raw("   "),
        Span::styled("? help", Style::default().fg(tint(Color::Gray))),
    ]);

    // The Status page owns the prominent state and next action. Repeating them
    // in the persistent header would compete with that hierarchy.
    let first_line = if app.page == Page::Status {
        Line::raw("")
    } else {
        let color = state_color(app.state);
        let spinner = if app.busy || app.state.is_busy() {
            if ascii_mode() {
                SPINNER_ASCII[(app.tick as usize / 2) % SPINNER_ASCII.len()]
            } else {
                SPINNER[(app.tick as usize / 2) % SPINNER.len()]
            }
        } else {
            " "
        };
        Line::from(vec![
            Span::styled(
                format!(" {} ", state_dot(app.state)),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.state.label(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(spinner, Style::default().fg(tint(Color::Yellow))),
        ])
    };

    let block = panel(&format!(
        " gp-tui v{}  GlobalProtect ",
        env!("CARGO_PKG_VERSION")
    ));
    frame.render_widget(
        Paragraph::new(Text::from(vec![first_line, tabs_line])).block(block),
        area,
    );
}

fn tab_span(label: &str, active: bool) -> Span<'static> {
    if active {
        Span::styled(
            format!(" {label} "),
            Style::default()
                .fg(tint(Color::Black))
                .bg(tint(Color::Cyan))
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(format!(" {label} "), Style::default().fg(tint(Color::Gray)))
    }
}

fn draw_status_page(frame: &mut Frame, app: &App, area: Rect) {
    if !app.installed {
        draw_setup_page(frame, app, area);
    } else if area.width >= 100 {
        draw_status_wide(frame, app, area);
    } else {
        draw_status_narrow(frame, app, area);
    }
}

fn draw_setup_page(frame: &mut Frame, app: &App, area: Rect) {
    let roomy = area.height >= 18;
    let warning = Style::default()
        .fg(tint(Color::Yellow))
        .add_modifier(Modifier::BOLD);
    let command = Style::default().fg(tint(Color::Cyan));

    let mut lines = vec![Line::from(vec![
        Span::styled(format!(" {} ", state_dot(app.state)), warning),
        Span::styled("No GlobalProtect backend installed.", warning),
    ])];
    if roomy {
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(Span::styled(
        "Official client:",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "  sudo ./gp_install_arch.sh PanGPLinux-*.tgz",
        command,
    )));
    lines.push(Line::from(Span::styled(
        "Free backend:",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "  sudo pacman -S globalprotect-openconnect",
        command,
    )));
    if roomy {
        lines.push(Line::raw(""));
    }
    lines.push(field_line("Portal", &display(app.cfg.portal())));
    if roomy {
        lines.push(Line::raw(""));
    }
    lines.push(action_line(app));
    if let Some(error) = &app.error {
        lines.push(Line::from(Span::styled(
            truncate(error, 160),
            Style::default().fg(tint(Color::Yellow)),
        )));
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(panel(" Setup "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_status_wide(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);

    let roomy = area.height >= 20;
    let mut left = Vec::new();
    if roomy {
        left.push(Line::raw(""));
    }
    left.push(state_line(app));
    left.push(action_line(app));
    if let Some(hint) = state_hint(app) {
        left.push(hint);
    }
    if roomy {
        left.push(Line::raw(""));
    }
    left.push(field_line("Portal", &display(app.cfg.portal())));
    if !app.cfg.active.is_empty() {
        left.push(field_line("Profile", &app.cfg.active));
    }
    left.push(field_line(
        "Backend",
        &format!(
            "{} (pref: {})",
            app.backend.label(),
            app.preference.as_str()
        ),
    ));
    left.extend(backend_aux_lines(app));

    frame.render_widget(
        Paragraph::new(Text::from(left))
            .block(panel(" Status "))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    let mut right = Vec::new();
    if roomy {
        right.push(Line::raw(""));
    }
    if let Some(error) = &app.error {
        right.push(Line::from(Span::styled(
            truncate(&format!("Problem: {error}"), 240),
            Style::default()
                .fg(tint(Color::Red))
                .add_modifier(Modifier::BOLD),
        )));
        right.push(Line::raw(""));
    }
    right.extend(detail_lines(app));

    frame.render_widget(
        Paragraph::new(Text::from(right))
            .block(panel(" Details "))
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn draw_status_narrow(frame: &mut Frame, app: &App, area: Rect) {
    let roomy = area.height >= 18;
    let max_lines = area.height.saturating_sub(2) as usize;

    // State and action come first so they stay visible when a short terminal
    // forces lower-priority details to be clipped.
    let mut lines = vec![state_line(app), action_line(app)];
    if let Some(hint) = state_hint(app) {
        lines.push(hint);
    }
    if let Some(error) = &app.error {
        lines.push(Line::from(Span::styled(
            truncate(&format!("Problem: {error}"), 240),
            Style::default()
                .fg(tint(Color::Red))
                .add_modifier(Modifier::BOLD),
        )));
    }
    if roomy {
        lines.push(Line::raw(""));
    }
    lines.push(field_line("Portal", &display(app.cfg.portal())));
    if !app.cfg.active.is_empty() {
        lines.push(field_line("Profile", &app.cfg.active));
    }
    lines.push(field_line(
        "Backend",
        &format!(
            "{} (pref: {})",
            app.backend.label(),
            app.preference.as_str()
        ),
    ));
    lines.extend(backend_aux_lines(app));
    if roomy {
        lines.push(Line::raw(""));
    }
    lines.extend(detail_lines(app));

    lines.truncate(max_lines);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(panel(" Status "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn state_line(app: &App) -> Line<'static> {
    let color = state_color(app.state);
    Line::from(vec![
        Span::styled(
            format!(" {} ", state_dot(app.state)),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.state.label(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn state_hint(app: &App) -> Option<Line<'static>> {
    let (text, color) = match app.state {
        VpnState::WaitingBrowser => (
            "Browser opened. Complete SSO/MFA; this window updates automatically.",
            Color::Yellow,
        ),
        VpnState::WaitingSudo => (
            "The terminal will ask for your sudo password.",
            Color::Yellow,
        ),
        VpnState::Connecting => ("Starting the tunnel...", Color::Yellow),
        VpnState::Disconnecting => ("Stopping the tunnel...", Color::Yellow),
        VpnState::Error => ("Something failed. Check the logs, then retry.", Color::Red),
        VpnState::NoBackend => ("Install a client or choose another backend.", Color::Yellow),
        VpnState::Unknown => ("Status is stale. Press r to refresh.", Color::Gray),
        VpnState::Disconnected | VpnState::Connected => return None,
    };
    Some(Line::from(Span::styled(
        text,
        Style::default().fg(tint(color)),
    )))
}

fn detail_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !app.details.ip.is_empty() {
        lines.push(field_line("IP", &app.details.ip));
    }
    if !app.details.gateway.is_empty() {
        lines.push(field_line("Gateway", &app.details.gateway));
    }
    if !app.details.gateway_ip.is_empty() {
        lines.push(field_line("Gateway IP", &app.details.gateway_ip));
    }
    if !app.details.protocol.is_empty() {
        lines.push(field_line("Protocol", &app.details.protocol));
    }
    if !app.details.uptime.is_empty() {
        lines.push(field_line("Uptime", &app.details.uptime));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            if app.state == VpnState::Connected {
                "No connection details yet."
            } else {
                "Connect to see IP, gateway and uptime."
            },
            Style::default().fg(tint(Color::Gray)),
        )));
    }
    lines
}

fn backend_aux_lines(app: &App) -> Vec<Line<'static>> {
    match app.backend {
        Backend::Official => {
            let agent = if app.gpa_active {
                Span::styled("active", Style::default().fg(tint(Color::Green)))
            } else {
                Span::styled(
                    "inactive (press g to start)",
                    Style::default().fg(tint(Color::Yellow)),
                )
            };
            vec![Line::from(vec![
                Span::styled("Agent: ", Style::default().add_modifier(Modifier::BOLD)),
                agent,
            ])]
        }
        Backend::Gpclient => vec![field_line("Helper", "gpauth + sudo gpclient")],
        Backend::None => Vec::new(),
    }
}

fn action_line(app: &App) -> Line<'static> {
    let mut spans = Vec::new();
    match app.state {
        VpnState::Connected => {
            push_action(&mut spans, "d", "Disconnect", true);
            push_action(&mut spans, "2", "Profiles", false);
            push_action(&mut spans, "r", "Refresh", false);
        }
        VpnState::Disconnected => {
            push_action(&mut spans, "c", "Connect", true);
            push_action(&mut spans, "2", "Profiles", false);
            push_action(&mut spans, "r", "Refresh", false);
        }
        VpnState::Unknown => {
            push_action(&mut spans, "r", "Refresh", true);
            push_action(&mut spans, "c", "Connect", false);
            push_action(&mut spans, "2", "Profiles", false);
        }
        VpnState::Error => {
            push_action(&mut spans, "c", "Retry", true);
            push_action(&mut spans, "3", "Logs", false);
            push_action(&mut spans, "2", "Profiles", false);
        }
        VpnState::NoBackend => {
            push_action(&mut spans, "2", "Profiles", true);
            push_action(&mut spans, "?", "Help", false);
        }
        VpnState::WaitingBrowser => {
            push_action(&mut spans, "Esc", "Cancel auth", true);
            push_action(&mut spans, "?", "Help", false);
        }
        VpnState::WaitingSudo | VpnState::Connecting | VpnState::Disconnecting => {
            spans.push(Span::styled(
                "Please wait...",
                Style::default()
                    .fg(tint(Color::Yellow))
                    .add_modifier(Modifier::BOLD),
            ));
            push_action(&mut spans, "3", "Logs", false);
            push_action(&mut spans, "?", "Help", false);
        }
    }
    Line::from(spans)
}

fn push_action(spans: &mut Vec<Span<'static>>, key: &str, label: &str, primary: bool) {
    if !spans.is_empty() {
        spans.push(Span::raw("   "));
    }
    let style = if primary {
        Style::default()
            .fg(tint(Color::Cyan))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(tint(Color::Gray))
    };
    spans.push(Span::styled(format!("[{key}] {label}"), style));
}

fn draw_profiles_page(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();

    if app.cfg.profiles.is_empty() {
        lines.push(Line::from(Span::styled(
            "No VPN profiles yet.",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from("Press n to create one, then fill the portal."));
    } else {
        let active = if app.cfg.active.is_empty() {
            "(none)"
        } else {
            &app.cfg.active
        };
        lines.push(Line::from(vec![
            Span::styled("Active: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(active.to_string(), Style::default().fg(tint(Color::Green))),
        ]));
        lines.push(Line::raw(""));

        for (index, profile) in app.cfg.profiles.iter().enumerate() {
            let selected = index == app.profile_index;
            let is_active = profile.name == app.cfg.active;
            let rename = if selected && matches!(app.mode, Mode::RenameProfile) {
                Some(app.input.as_str())
            } else {
                None
            };
            lines.push(profile_row(profile, selected, is_active, rename, width));
        }
    }

    if let Some(error) = &app.error {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            truncate(error, 160),
            Style::default().fg(tint(Color::Red)),
        )));
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(panel(" Profiles "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn profile_row(
    profile: &Profile,
    selected: bool,
    active: bool,
    rename: Option<&str>,
    width: usize,
) -> Line<'static> {
    let backend = if width >= 58 {
        Some(profile.backend.as_str())
    } else {
        None
    };
    let backend_len = backend.map(|value| value.chars().count()).unwrap_or(0);
    let fixed = 4
        + 2
        + if backend.is_some() {
            2 + backend_len
        } else {
            0
        };
    let available = width.saturating_sub(fixed);
    let name_width = (available / 3)
        .clamp(10, 22)
        .min(available.saturating_sub(12).max(1));
    let portal_width = available.saturating_sub(name_width + 2).max(1);

    let prefix = if selected {
        if ascii_mode() {
            "> "
        } else {
            "› "
        }
    } else {
        "  "
    };
    let marker = if active {
        if ascii_mode() {
            "*"
        } else {
            "●"
        }
    } else {
        " "
    };
    let name = match rename {
        Some(input) => format!("{input}{}", cursor()),
        None => profile.name.clone(),
    };
    let name = pad_to(truncate(&name, name_width), name_width);
    let portal = if profile.portal.is_empty() {
        "(no portal)"
    } else {
        &profile.portal
    };
    let portal = pad_to(truncate(portal, portal_width), portal_width);

    let name_style = if selected {
        Style::default()
            .fg(tint(Color::Cyan))
            .add_modifier(Modifier::BOLD)
    } else if active {
        Style::default()
            .fg(tint(Color::Green))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let mut spans = vec![
        Span::styled(
            prefix,
            if selected {
                Style::default().fg(tint(Color::Cyan))
            } else {
                Style::default()
            },
        ),
        Span::styled(
            format!("{marker} "),
            Style::default().fg(tint(Color::Green)),
        ),
        Span::styled(name, name_style),
        Span::raw("  "),
        Span::styled(portal, Style::default().fg(tint(Color::Gray))),
    ];
    if let Some(backend) = backend {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            backend.to_string(),
            Style::default().fg(tint(Color::Gray)),
        ));
    }
    Line::from(spans)
}

fn draw_config_page(frame: &mut Frame, app: &App, area: Rect) {
    let editing =
        |field: Field| -> bool { matches!(app.mode, Mode::Edit(current) if current == field) };

    let portal = if editing(Field::Portal) {
        format!("{}{}", app.input, cursor())
    } else {
        display(app.cfg.portal())
    };
    let username = if editing(Field::Username) {
        format!("{}{}", app.input, cursor())
    } else if app.cfg.username().is_empty() {
        "(not set)".to_string()
    } else {
        app.cfg.username().to_string()
    };
    let backend = format!(
        "{} (active: {})",
        app.preference.as_str(),
        app.backend.label()
    );

    let ssl_only = match app.backend {
        Backend::Official => match app.ssl_only {
            Some(true) => "on".to_string(),
            Some(false) => "off".to_string(),
            None if app.ssl_only_inflight => "loading...".to_string(),
            None => "unavailable".to_string(),
        },
        _ => "n/a (official only)".to_string(),
    };
    let items = [
        ("Portal", portal),
        ("User (optional)", username),
        ("Backend", backend),
        ("SSL-only", ssl_only),
    ];

    let mut lines: Vec<Line> = Vec::new();
    for (index, (label, value)) in items.iter().enumerate() {
        let selected = index == app.config_index;
        let prefix = if selected {
            if ascii_mode() {
                "> "
            } else {
                "› "
            }
        } else {
            "  "
        };
        let style = if selected {
            Style::default()
                .fg(tint(Color::Cyan))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(
                format!("{label}: "),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(value.clone(), style),
        ]));
    }

    lines.push(Line::raw(""));
    let description = match app.config_index {
        0 => "Portal host, e.g. vpn.example.com (https:// is stripped)".to_string(),
        1 => "Optional. SSO usually identifies you; leave empty if unsure.".to_string(),
        2 => {
            let official = detect_backend(BackendPreference::Official) != Backend::None;
            let free = detect_backend(BackendPreference::Gpclient) != Backend::None;
            format!(
                "official: {} · free: {}",
                if official { "found" } else { "not installed" },
                if free { "found" } else { "not installed" }
            )
        }
        _ => "Force SSL when IPSec/UDP 4501 is blocked (official backend).".to_string(),
    };
    lines.push(Line::from(Span::styled(
        description,
        Style::default().fg(tint(Color::Gray)),
    )));
    lines.push(Line::from(Span::styled(
        "Saved in ~/.config/gp-tui/config.toml",
        Style::default().fg(tint(Color::DarkGray)),
    )));

    if let Some(error) = &app.error {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            truncate(error, 160),
            Style::default().fg(tint(Color::Red)),
        )));
    }

    let title = if app.cfg.active.is_empty() {
        " Settings ".to_string()
    } else {
        format!(" Settings — {} ", app.cfg.active)
    };
    let block = panel(&title);
    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), area);
}

fn draw_logs_page(frame: &mut Frame, app: &App, area: Rect) {
    let height = area.height.saturating_sub(2) as usize;
    let query = app.log_search.as_deref().map(str::to_lowercase);
    let lines: Vec<Line> = if app.logs.is_empty() {
        vec![Line::raw("(no logs yet)")]
    } else {
        let filtered: Vec<&String> = match &query {
            Some(query) => app
                .logs
                .iter()
                .filter(|line| line.to_lowercase().contains(query))
                .collect(),
            None => app.logs.iter().collect(),
        };
        if filtered.is_empty() {
            vec![Line::from(Span::styled(
                format!(
                    "(no log lines match \"{}\")",
                    app.log_search.as_deref().unwrap_or("")
                ),
                Style::default().fg(tint(Color::DarkGray)),
            ))]
        } else {
            filtered
                .into_iter()
                .map(|line| Line::raw(line.clone()))
                .collect()
        }
    };

    let max_offset = lines.len().saturating_sub(height.max(1));
    let offset = if app.log_follow {
        max_offset
    } else {
        max_offset.saturating_sub(app.log_from_bottom)
    };

    let title = if let Some(query) = &app.log_search {
        format!(" Logs — search: {query} ")
    } else if app.log_follow {
        " Logs (following) ".to_string()
    } else {
        " Logs (paused) ".to_string()
    };
    let block = panel(&title);
    let mut paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .scroll((offset.min(u16::MAX as usize) as u16, 0));
    if app.log_wrap {
        paragraph = paragraph.wrap(Wrap { trim: false });
    }
    frame.render_widget(paragraph, area);

    let mut scroll_state = ScrollbarState::new(max_offset.saturating_add(1)).position(offset);
    let (begin, end, track, thumb) = if ascii_mode() {
        ("^", "v", "|", "#")
    } else {
        ("↑", "↓", "│", "█")
    };
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(Some(begin))
        .end_symbol(Some(end))
        .track_symbol(Some(track))
        .thumb_symbol(thumb);
    frame.render_stateful_widget(
        scrollbar,
        area.inner(Margin {
            vertical: 1,
            horizontal: 0,
        }),
        &mut scroll_state,
    );
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width as usize;
    let notice = app.notice();
    if let Some(notice) = notice {
        let prefix = if ascii_mode() { "* " } else { "✓ " };
        let text = truncate(&format!("{prefix}{notice}"), width);
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(tint(Color::Green))),
            area,
        );
        return;
    }

    let hints: Vec<&str> = match app.mode {
        Mode::Edit(_) => vec!["Enter save", "Esc cancel"],
        Mode::RenameProfile => vec!["Enter save", "Esc cancel"],
        Mode::Search => vec!["Enter apply", "Esc cancel", "/ logs"],
        Mode::Normal => match app.page {
            Page::Status => {
                let mut hints = if app.state == VpnState::WaitingBrowser {
                    vec!["Esc cancel auth", "1/2/3 pages", "? all keys", "q quit"]
                } else if app.state == VpnState::NoBackend {
                    vec!["2 profiles", "1/2/3 pages", "? help", "q quit"]
                } else {
                    vec!["1/2/3 pages", "r refresh", "? all keys", "q quit"]
                };
                if app.backend == Backend::Official {
                    hints.push("g gpa");
                }
                hints.push("y diag");
                hints
            }
            Page::Profiles => vec![
                "j/k select",
                "Enter activate",
                "e settings",
                "? all keys",
                "n/R/D",
                "q quit",
                "h/l pages",
            ],
            Page::Config => vec![
                "j/k",
                "Enter edit/cycle",
                "b backend",
                "Esc back",
                "? all keys",
                "q quit",
                "h/l pages",
            ],
            Page::Logs => vec![
                "j/k scroll",
                "f follow",
                "/ search",
                "L load",
                "? all keys",
                "w wrap",
                "q quit",
                "Ctrl+d/u",
            ],
        },
    };
    let text = fit_hints(&hints, width);
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(tint(Color::Gray))),
        area,
    );
}

fn fit_hints(hints: &[&str], width: usize) -> String {
    let mut text = String::new();
    for hint in hints {
        let joiner = if text.is_empty() { "" } else { separator() };
        let next_len = text.chars().count() + joiner.chars().count() + hint.chars().count();
        if next_len > width {
            break;
        }
        text.push_str(joiner);
        text.push_str(hint);
    }
    if text.is_empty() {
        truncate(hints.first().copied().unwrap_or(""), width)
    } else {
        text
    }
}

fn draw_help(frame: &mut Frame, app: &App) {
    let frame_area = frame.area();
    // Clear the whole frame so the modal does not have header/footer borders
    // cutting through it when a margin is used on larger terminals.
    frame.render_widget(Clear, frame_area);
    let area = if frame_area.width >= 70 && frame_area.height >= 20 {
        frame_area.inner(Margin {
            vertical: 1,
            horizontal: 1,
        })
    } else {
        frame_area
    };

    let inner_width = area.width.saturating_sub(2) as usize;
    let inner_height = area.height.saturating_sub(2) as usize;
    let lines = help_lines(app, inner_width, inner_height);

    let block = panel(" Help ");
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn help_lines(app: &App, inner_width: usize, inner_height: usize) -> Vec<Line<'static>> {
    if inner_height >= 25 && inner_width >= 76 {
        full_help_lines(app)
    } else if inner_height >= 20 && inner_width >= 62 {
        standard_help_lines(app)
    } else {
        compact_help_lines()
    }
}

fn compact_help_lines() -> Vec<Line<'static>> {
    vec![
        help_text("Pages: 1 Status · 2 Profiles · 3 Logs · Tab next"),
        help_text("  Shift+Tab/h/l prev-next · j/k move/scroll"),
        help_text("Status: c connect · d disconnect · r refresh"),
        help_text("  Esc cancels browser auth · g gpa (official)"),
        help_text("Profiles: Enter activate · e settings · n new"),
        help_text("  R rename · D delete"),
        help_text("Settings: j/k · Enter edit/cycle · b backend · Esc back"),
        help_text("  p portal · u user · SSL-only needs official backend"),
        help_text("Logs: j/k/arrow scroll · Ctrl+d/u · PgUp/PgDn · Home/End"),
        help_text("  G tail · f follow · w wrap · / search · L load"),
        help_text("Global: ? help · Esc back/cancel · q/Ctrl+C quit"),
        help_text("  y diagnostics · MFA stays in browser"),
        help_text("Backends: official CLI or free gpauth+gpclient"),
        help_text("Esc or ? to close"),
    ]
}

fn standard_help_lines(app: &App) -> Vec<Line<'static>> {
    vec![
        help_text("Pages: 1/2/3 · Tab/Shift+Tab · h/l · j/k move"),
        help_text("Status: c connect · d disconnect · r refresh · g gpa (official)"),
        help_text("  Esc cancels browser authentication"),
        help_text(&format!(
            "States: {} connected · {} working · {} error",
            state_dot(VpnState::Connected),
            state_dot(VpnState::WaitingBrowser),
            state_dot(VpnState::Error)
        )),
        help_text("Profiles: Enter activate · e settings · n new"),
        help_text("  R rename · D delete"),
        help_text("Settings: j/k · Enter edit/cycle · b backend · Esc back"),
        help_text("  p portal · u user · SSL-only needs official backend"),
        help_text("Logs: j/k/arrows scroll · Ctrl+d/u · PgUp/PgDn · Home/End"),
        help_text("  G tail · f follow · w wrap · / search · L load"),
        help_text("Global: ? help · Esc back/cancel · q/Ctrl+C quit"),
        help_text("  y diagnostics · MFA stays in browser"),
        help_text(&format!(
            "Current: {} · backend {} · pref {}",
            app.state.label(),
            app.backend.label(),
            app.preference.as_str()
        )),
        help_text("Backends: official CLI or free gpauth+gpclient"),
        help_text("  official: sudo ./gp_install_arch.sh PanGPLinux-*.tgz"),
        help_text("  free: sudo pacman -S globalprotect-openconnect"),
        help_text("Esc or ? to close"),
    ]
}

fn full_help_lines(app: &App) -> Vec<Line<'static>> {
    vec![
        help_heading("Pages"),
        help_text("1/2/3 switch · Tab/Shift+Tab next/prev · h/l pages"),
        help_text("j/k select or scroll"),
        help_heading("Status"),
        help_text("c connect · d disconnect · r refresh · g start gpa (official)"),
        help_text("Esc cancels browser authentication"),
        help_text(&format!(
            "States: {} connected · {} working · {} error",
            state_dot(VpnState::Connected),
            state_dot(VpnState::WaitingBrowser),
            state_dot(VpnState::Error)
        )),
        help_heading("Profiles"),
        help_text("j/k select · Enter activate · e settings · n new"),
        help_text("R rename · D delete"),
        help_heading("Settings"),
        help_text("j/k select · Enter edit/cycle · b backend · Esc back"),
        help_text("p quick portal · u quick user · SSL-only requires official backend"),
        help_heading("Logs"),
        help_text("j/k or arrows scroll · Ctrl+d/u · PgUp/PgDn · Home/End"),
        help_text("G tail · f follow · w wrap · / search · L load"),
        help_heading("Global"),
        help_text("? help · Esc back/cancel · q/Ctrl+C quit · y diagnostics"),
        help_text(&format!(
            "Current: {} · backend {} · pref {}",
            app.state.label(),
            app.backend.label(),
            app.preference.as_str()
        )),
        help_heading("Backends"),
        help_text("official: sudo ./gp_install_arch.sh PanGPLinux-*.tgz"),
        help_text("free: sudo pacman -S globalprotect-openconnect"),
        help_text("MFA stays in the browser; no passwords are stored."),
        help_text("Esc or ? to close"),
    ]
}

fn help_heading(label: &str) -> Line<'static> {
    Line::from(Span::styled(
        terminal_text(label),
        Style::default().add_modifier(Modifier::BOLD),
    ))
}

fn help_text(text: &str) -> Line<'static> {
    Line::from(terminal_text(text))
}

fn terminal_text(text: &str) -> String {
    if ascii_mode() {
        text.replace('·', "|")
            .replace('↑', "Up")
            .replace('↓', "Down")
            .replace('—', "-")
            .replace('›', ">")
            .replace('…', "...")
    } else {
        text.to_string()
    }
}

fn draw_quit_modal(frame: &mut Frame, app: &App) {
    let area = centered(frame.area(), 64, 34);
    frame.render_widget(Clear, area);

    let first = if app.state == VpnState::Connected {
        "GlobalProtect is still connected."
    } else {
        "An operation is still running."
    };
    let lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            first,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from("[d] Disconnect and quit"),
        Line::from("[q] Quit anyway"),
        Line::from("[Esc] Stay"),
    ];

    let block = Block::default().borders(Borders::ALL).title(" Quit? ");
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_profile_delete_modal(frame: &mut Frame, app: &App) {
    let Some(name) = &app.profile_delete else {
        return;
    };
    let area = centered(frame.area(), 64, 30);
    frame.render_widget(Clear, area);

    let lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            format!("Delete profile \"{name}\"?"),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from("This only removes the saved configuration."),
        Line::raw(""),
        Line::from("[y] Delete profile"),
        Line::from("[n] / [Esc] Cancel"),
    ];

    let block = panel(" Delete profile? ");
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn centered(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}

fn field_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_string()),
    ])
}

fn display(value: &str) -> String {
    if value.is_empty() {
        "(not set)".to_string()
    } else {
        value.to_string()
    }
}

fn state_dot(state: VpnState) -> &'static str {
    if ascii_mode() {
        return match state {
            VpnState::Connected => "*",
            VpnState::WaitingBrowser
            | VpnState::WaitingSudo
            | VpnState::Connecting
            | VpnState::Disconnecting => "~",
            VpnState::Disconnected => "o",
            VpnState::Error => "x",
            VpnState::NoBackend => "!",
            VpnState::Unknown => "?",
        };
    }
    match state {
        VpnState::Connected => "●",
        VpnState::WaitingBrowser
        | VpnState::WaitingSudo
        | VpnState::Connecting
        | VpnState::Disconnecting => "◐",
        VpnState::Disconnected => "○",
        VpnState::Error => "✗",
        VpnState::NoBackend => "!",
        VpnState::Unknown => "?",
    }
}

fn state_hue(state: VpnState) -> Color {
    match state {
        VpnState::Connected => Color::Green,
        VpnState::WaitingBrowser
        | VpnState::WaitingSudo
        | VpnState::Connecting
        | VpnState::Disconnecting => Color::Yellow,
        VpnState::NoBackend => Color::Yellow,
        VpnState::Error => Color::Red,
        VpnState::Disconnected | VpnState::Unknown => Color::Gray,
    }
}

fn state_color(state: VpnState) -> Color {
    tint(state_hue(state))
}

fn truncate(value: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let count = value.chars().count();
    if count <= max {
        return value.to_string();
    }
    if ascii_mode() {
        let keep = max.saturating_sub(3);
        let mut truncated: String = value.chars().take(keep).collect();
        if keep > 0 {
            truncated.push_str("...");
        }
        truncated
    } else {
        let mut truncated: String = value.chars().take(max - 1).collect();
        truncated.push('…');
        truncated
    }
}

fn pad_to(value: String, width: usize) -> String {
    let length = value.chars().count();
    if length >= width {
        value
    } else {
        format!("{value}{}", " ".repeat(width - length))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::config::Config;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn test_app() -> App {
        let mut config = Config::default();
        config
            .add_profile("default".to_string(), "vpn.example.com".to_string())
            .unwrap();
        App::new(config)
    }

    fn status_app(state: VpnState) -> App {
        let mut app = test_app();
        app.installed = true;
        app.backend = Backend::Official;
        app.state = state;
        app
    }

    fn render(app: &App, width: u16, height: u16) -> String {
        render_rows(app, width, height).concat()
    }

    fn render_rows(app: &App, width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        let cells: Vec<String> = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol().to_string())
            .collect();
        cells
            .chunks(width as usize)
            .map(|row| row.concat())
            .collect()
    }

    #[test]
    fn renders_tabs_at_80x24() {
        let app = test_app();
        let text = render(&app, 80, 24);
        assert!(text.contains("Status"));
        assert!(text.contains("Profiles"));
        assert!(text.contains("Logs"));
    }

    #[test]
    fn renders_wide_layout_with_details() {
        let mut app = status_app(VpnState::Connected);
        app.details.ip = "10.22.32.92".to_string();
        let text = render(&app, 120, 40);
        assert!(text.contains("Details"));
        assert!(text.contains("10.22.32.92"));
    }

    #[test]
    fn keeps_state_action_and_footer_visible_at_supported_sizes() {
        for (width, height) in [(60, 16), (80, 24), (120, 40)] {
            let app = status_app(VpnState::Disconnected);
            let rows = render_rows(&app, width, height);
            let text = rows.concat();
            assert!(
                text.contains("Disconnected"),
                "state missing at {width}x{height}"
            );
            assert!(
                text.contains("[c] Connect"),
                "primary action missing at {width}x{height}"
            );
            let footer = rows.last().unwrap();
            assert!(
                footer.contains("1/2/3 pages") && footer.contains("? all keys"),
                "footer hints missing at {width}x{height}: {footer:?}"
            );
            assert!(
                footer.chars().count() <= width as usize,
                "footer overflows at {width}x{height}"
            );
        }
    }

    #[test]
    fn error_action_uses_direct_logs_key() {
        let mut app = status_app(VpnState::Error);
        app.error = Some("gpclient exited with status 2".to_string());
        let text = render(&app, 100, 32);
        assert!(text.contains("[3] Logs"));
        assert!(!text.contains("[l] Logs"));
    }

    #[test]
    fn help_reaches_final_content_at_common_sizes() {
        for (width, height) in [(60, 16), (80, 24), (100, 32), (120, 40)] {
            let mut app = test_app();
            app.help = true;
            let text = render(&app, width, height);
            assert!(
                text.contains("Pages"),
                "help pages missing at {width}x{height}"
            );
            assert!(
                text.contains("L load"),
                "help log keys missing at {width}x{height}"
            );
            assert!(
                text.contains("MFA stays in"),
                "help MFA note missing at {width}x{height}"
            );
            assert!(
                text.contains("Esc or ? to close"),
                "final help line missing at {width}x{height}"
            );
        }
    }

    #[test]
    fn no_backend_and_error_states_are_semantically_distinct() {
        assert_eq!(state_hue(VpnState::Error), Color::Red);
        assert_ne!(state_hue(VpnState::NoBackend), state_hue(VpnState::Error));
        assert_ne!(state_dot(VpnState::NoBackend), state_dot(VpnState::Error));
    }

    #[test]
    fn profiles_use_one_width_aware_row_on_narrow_terminals() {
        let mut app = test_app();
        app.page = Page::Profiles;
        app.installed = true;
        app.backend = Backend::Official;
        app.state = VpnState::Disconnected;
        app.cfg.profiles[0].name = "a-long-profile-name".to_string();
        let rows = render_rows(&app, 60, 16);
        let row = rows
            .iter()
            .find(|row| row.contains("a-long"))
            .expect("profile row should be visible");
        assert!(row.contains("vpn.example.com"));
        assert!(row.contains("auto"));
    }

    #[test]
    fn footer_hints_fit_narrow_terminal_on_every_page() {
        for page in [Page::Status, Page::Profiles, Page::Config, Page::Logs] {
            let mut app = status_app(VpnState::Disconnected);
            app.page = page;
            let rows = render_rows(&app, 60, 16);
            let footer = rows.last().unwrap();
            assert!(
                footer.chars().count() <= 60,
                "footer overflows on {page:?}: {footer:?}"
            );
            assert!(
                footer.contains("? all keys"),
                "help hint missing on {page:?}: {footer:?}"
            );
        }
    }

    #[test]
    fn warns_when_terminal_is_too_small() {
        let app = test_app();
        let text = render(&app, 40, 10);
        assert!(text.contains("Terminal too small"));
    }

    #[test]
    fn renders_profiles_page() {
        let mut app = test_app();
        app.page = Page::Profiles;
        let text = render(&app, 100, 30);
        assert!(text.contains("Profiles"));
        assert!(text.contains("Active:"));
    }

    #[test]
    fn renders_config_page_with_ssl_only() {
        let mut app = test_app();
        app.page = Page::Config;
        app.backend = Backend::Official;
        app.installed = true;
        app.state = VpnState::Disconnected;
        let text = render(&app, 100, 32);
        assert!(text.contains("SSL-only"));
        assert!(text.contains("Portal"));
    }

    #[test]
    fn renders_logs_and_search_title() {
        let mut app = test_app();
        app.page = Page::Logs;
        app.logs.push_back("hello world".to_string());
        app.log_search = Some("hello".to_string());
        let text = render(&app, 100, 30);
        assert!(text.contains("Logs"));
        assert!(text.contains("hello"));
    }

    #[test]
    fn renders_quit_modal() {
        let mut app = status_app(VpnState::Connected);
        app.quit_modal = true;
        let text = render(&app, 100, 32);
        assert!(text.contains("Quit?"));
        assert!(text.contains("Disconnect and quit"));
    }

    #[test]
    fn renders_profile_delete_modal() {
        let mut app = test_app();
        app.page = Page::Profiles;
        app.profile_delete = Some("default".to_string());
        let text = render(&app, 100, 32);
        assert!(text.contains("Delete profile?"));
    }

    #[test]
    fn renders_no_backend_setup() {
        let mut app = test_app();
        app.backend = Backend::None;
        app.installed = false;
        app.state = VpnState::NoBackend;
        let text = render(&app, 100, 30);
        assert!(text.contains("No GlobalProtect backend installed"));
    }

    #[test]
    fn no_backend_setup_fits_minimum_terminal() {
        let mut app = test_app();
        app.backend = Backend::None;
        app.installed = false;
        app.state = VpnState::NoBackend;
        let text = render(&app, 60, 16);
        assert!(text.contains("No GlobalProtect backend installed"));
        assert!(text.contains("gp_install_arch.sh"));
        assert!(text.contains("globalprotect-openconnect"));
        assert!(text.contains("[2] Profiles"));
    }

    #[test]
    fn setup_action_stays_visible_with_long_note() {
        let mut app = test_app();
        app.backend = Backend::None;
        app.installed = false;
        app.state = VpnState::NoBackend;
        app.error = Some(
            "backend 'official' is not installed; install it or choose another backend".repeat(3),
        );
        let text = render(&app, 60, 16);
        assert!(text.contains("[2] Profiles"));
    }
}
