use crate::app::VpnState;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};

pub(super) const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub(super) const SPINNER_ASCII: [&str; 4] = ["|", "/", "-", "\\"];

pub(super) fn ascii_mode() -> bool {
    std::env::var_os("GP_TUI_ASCII").is_some()
        || std::env::var("TERM")
            .map(|term| term == "linux" || term == "dumb")
            .unwrap_or(false)
}

pub(super) fn tint(color: Color) -> Color {
    if std::env::var_os("NO_COLOR").is_some() {
        Color::Reset
    } else {
        color
    }
}

pub(super) fn separator() -> &'static str {
    if ascii_mode() {
        " | "
    } else {
        " · "
    }
}

pub(super) fn cursor() -> &'static str {
    if ascii_mode() {
        "|"
    } else {
        "█"
    }
}

pub(super) fn spinner(tick: u64) -> &'static str {
    if ascii_mode() {
        SPINNER_ASCII[(tick as usize / 2) % SPINNER_ASCII.len()]
    } else {
        SPINNER[(tick as usize / 2) % SPINNER.len()]
    }
}

pub(super) fn panel(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(if ascii_mode() {
            BorderType::Plain
        } else {
            BorderType::Rounded
        })
        .title(title.to_string())
}
pub(super) fn tab_span(label: &str, active: bool) -> Span<'static> {
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
pub(super) fn field_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_string()),
    ])
}

pub(super) fn display(value: &str) -> String {
    if value.is_empty() {
        "(not set)".to_string()
    } else {
        value.to_string()
    }
}

pub(super) fn state_dot(state: VpnState) -> &'static str {
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

pub(super) fn state_hue(state: VpnState) -> Color {
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

pub(super) fn state_color(state: VpnState) -> Color {
    tint(state_hue(state))
}

pub(super) fn truncate(value: &str, max: usize) -> String {
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

pub(super) fn pad_to(value: String, width: usize) -> String {
    let length = value.chars().count();
    if length >= width {
        value
    } else {
        format!("{value}{}", " ".repeat(width - length))
    }
}
