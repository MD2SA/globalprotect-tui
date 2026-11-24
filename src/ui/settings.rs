use super::shared::*;
use crate::app::{App, Field, Modal};
use crate::client::{detect_backend, Backend, BackendPreference};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub(super) fn draw_config_page(frame: &mut Frame, app: &App, area: Rect) {
    let editing =
        |field: Field| -> bool { matches!(&app.modal, Modal::Edit(current) if *current == field) };

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
