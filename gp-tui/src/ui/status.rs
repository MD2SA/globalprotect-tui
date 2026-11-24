use super::shared::*;
use crate::app::{App, VpnState};
use crate::client::Backend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

pub(super) fn draw_status_page(frame: &mut Frame, app: &App, area: Rect) {
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
        Span::styled(format!(" {} ", state_dot(app.state())), warning),
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
    let color = state_color(app.state());
    Line::from(vec![
        Span::styled(
            format!(" {} ", state_dot(app.state())),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            app.state().label(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn state_hint(app: &App) -> Option<Line<'static>> {
    let (text, color) = match app.state() {
        VpnState::WaitingBrowser => (
            "Browser opened. Complete SSO/MFA; this window updates automatically.",
            Color::Yellow,
        ),
        VpnState::WaitingSudo => (
            "Enter your sudo password in the prompt to start the tunnel.",
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
            if app.state() == VpnState::Connected {
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
    match app.state() {
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
