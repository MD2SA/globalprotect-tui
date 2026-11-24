use super::shared::*;
use crate::app::{App, SudoAction, SudoPrompt, VpnState};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

pub(super) fn draw_quit_modal(frame: &mut Frame, app: &App) {
    let area = centered(frame.area(), 64, 34);
    frame.render_widget(Clear, area);

    let first = if app.state() == VpnState::Connected {
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

pub(super) fn draw_profile_delete_modal(frame: &mut Frame, name: &str) {
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

pub(super) fn draw_sudo_modal(
    frame: &mut Frame,
    prompt: &SudoPrompt,
    tick: u64,
    action: SudoAction,
) {
    let area = centered_size(frame.area(), 64, 10);
    frame.render_widget(Clear, area);

    let purpose = match action {
        SudoAction::Connect => "Root is needed to create the VPN tunnel.",
        SudoAction::Disconnect => "Root is needed to tear down the VPN tunnel.",
        SudoAction::Quit => "Root is needed to disconnect before quitting.",
    };
    let user = std::env::var("USER").unwrap_or_else(|_| "you".to_string());

    let mut lines = vec![
        Line::from(Span::styled(
            purpose,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        if prompt.inflight {
            Line::from(vec![
                Span::raw(format!("Password for {user}: ")),
                Span::styled(
                    format!("{} checking...", spinner(tick)),
                    Style::default().fg(tint(Color::Yellow)),
                ),
            ])
        } else {
            Line::from(vec![
                Span::raw(format!("Password for {user}: ")),
                Span::styled(
                    "*".repeat(
                        String::from_utf8_lossy(&prompt.password)
                            .chars()
                            .count()
                            .min(44),
                    ),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(cursor(), Style::default().fg(tint(Color::Cyan))),
            ])
        },
    ];

    if let Some(error) = &prompt.error {
        lines.push(Line::from(Span::styled(
            error.clone(),
            Style::default().fg(tint(Color::Red)),
        )));
    } else {
        lines.push(Line::raw(""));
    }

    lines.push(Line::raw(""));
    if prompt.inflight {
        lines.push(Line::from("Waiting for sudo..."));
    } else {
        lines.push(Line::from("[Enter] Authenticate   [Esc] Cancel"));
    }
    lines.push(Line::from(Span::styled(
        "Kept in memory only; never saved.",
        Style::default().fg(tint(Color::Gray)),
    )));

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(panel(" sudo authentication "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn centered_size(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
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
