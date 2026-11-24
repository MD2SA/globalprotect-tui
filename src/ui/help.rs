use super::shared::*;
use crate::app::App;
use crate::client::Backend;
use crate::keymap::{self, Action as KeyAction, HelpGroup};
use ratatui::layout::Margin;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui::Frame;

pub(super) fn draw_help(frame: &mut Frame, app: &App) {
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
    let compact = inner_height < 20 || inner_width < 62;
    let detailed = inner_height >= 25 && inner_width >= 76;
    let mut lines = Vec::new();

    for (group, title) in keymap::help_groups() {
        let mut items = keymap::help_items(group);
        items.retain(|(action, _, _)| {
            if app.backend != Backend::Official && *action == KeyAction::StartGpa {
                return false;
            }
            !compact || compact_help_action(group, *action)
        });
        if items.is_empty() {
            continue;
        }

        let details = items
            .iter()
            .map(|(_, keys, help)| format!("{keys} {help}"))
            .collect::<Vec<_>>()
            .join(" · ");
        if detailed {
            lines.push(help_heading(title));
            lines.push(help_text(&details));
        } else {
            lines.push(help_text(&format!("{title}: {details}")));
        }
    }

    if !compact {
        lines.push(help_text(&format!(
            "Current: {} · backend {} · pref {}",
            app.state().label(),
            app.backend.label(),
            app.preference.as_str()
        )));
        lines.push(help_text("Backends: official CLI or free gpauth+gpclient"));
    }
    lines.push(help_text(
        "MFA stays in the browser; VPN credentials are not stored.",
    ));
    lines.push(help_text("Esc or ? to close"));
    lines
}

fn compact_help_action(group: HelpGroup, action: KeyAction) -> bool {
    match group {
        HelpGroup::Pages => matches!(
            action,
            KeyAction::StatusPage
                | KeyAction::ProfilesPage
                | KeyAction::LogsPage
                | KeyAction::NextPage
                | KeyAction::PreviousPage
                | KeyAction::MoveDown
                | KeyAction::MoveUp
        ),
        HelpGroup::Status => matches!(
            action,
            KeyAction::Connect | KeyAction::Disconnect | KeyAction::Refresh | KeyAction::StartGpa
        ),
        HelpGroup::Profiles => matches!(
            action,
            KeyAction::NewProfile
                | KeyAction::RenameProfile
                | KeyAction::EditProfile
                | KeyAction::DeleteProfile
                | KeyAction::ActivateProfile
        ),
        HelpGroup::Settings => matches!(
            action,
            KeyAction::CycleBackend
                | KeyAction::ConfigActivate
                | KeyAction::EditPortal
                | KeyAction::EditUsername
        ),
        HelpGroup::Logs => matches!(
            action,
            KeyAction::MoveDown
                | KeyAction::MoveUp
                | KeyAction::LogsTop
                | KeyAction::LogsBottom
                | KeyAction::ToggleLogFollow
                | KeyAction::ToggleLogWrap
                | KeyAction::SearchLogs
                | KeyAction::CollectLogs
                | KeyAction::LogPageUp
                | KeyAction::LogPageDown
                | KeyAction::LogHalfPageDown
                | KeyAction::LogHalfPageUp
        ),
        HelpGroup::Global => matches!(
            action,
            KeyAction::Help | KeyAction::Escape | KeyAction::Quit | KeyAction::Diagnostics
        ),
    }
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
