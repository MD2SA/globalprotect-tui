use super::shared::*;
use crate::app::{App, Modal};
use crate::config::Profile;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

pub(super) fn draw_profiles_page(frame: &mut Frame, app: &App, area: Rect) {
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
            let rename = if selected && matches!(&app.modal, Modal::RenameProfile) {
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
