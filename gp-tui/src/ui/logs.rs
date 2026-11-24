use super::shared::*;
use crate::app::{App, Modal, Page, VpnState};
use crate::client::Backend;
use crate::keymap;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use ratatui::Frame;

pub(super) fn draw_logs_page(frame: &mut Frame, app: &App, area: Rect) {
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

pub(super) fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
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

    let hints: Vec<&str> = match &app.modal {
        Modal::Edit(_) | Modal::RenameProfile => vec!["Enter save", "Esc cancel"],
        Modal::Search => vec!["Enter apply", "Esc cancel", "/ logs"],
        _ => {
            let mut hints = keymap::footer_for(app.page);
            if app.page == Page::Status && app.state() == VpnState::WaitingBrowser {
                hints.retain(|hint| !matches!(*hint, "c connect" | "d disconnect"));
                hints.push("Esc cancel auth");
            }
            if app.backend != Backend::Official {
                hints.retain(|hint| *hint != "g gpa");
            }
            hints
        }
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
