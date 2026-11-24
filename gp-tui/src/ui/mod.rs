mod help;
mod logs;
mod modals;
mod profiles;
mod settings;
mod shared;
mod status;

#[cfg(test)]
mod tests;

use crate::app::{App, Modal, Page, SudoAction};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use shared::*;

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
        Page::Status => status::draw_status_page(frame, app, chunks[1]),
        Page::Profiles => profiles::draw_profiles_page(frame, app, chunks[1]),
        Page::Config => settings::draw_config_page(frame, app, chunks[1]),
        Page::Logs => logs::draw_logs_page(frame, app, chunks[1]),
    }
    logs::draw_footer(frame, app, chunks[2]);

    match &app.modal {
        Modal::Help => help::draw_help(frame, app),
        Modal::Quit => modals::draw_quit_modal(frame, app),
        Modal::DeleteProfile(name) => modals::draw_profile_delete_modal(frame, name),
        Modal::Sudo(prompt) => modals::draw_sudo_modal(
            frame,
            prompt,
            app.tick,
            app.connection.sudo_action().unwrap_or(SudoAction::Connect),
        ),
        Modal::None | Modal::Edit(_) | Modal::RenameProfile | Modal::Search => {}
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
        let color = state_color(app.state());
        let spinner = if app.log_collect_inflight || app.state().is_busy() {
            spinner(app.tick)
        } else {
            " "
        };
        Line::from(vec![
            Span::styled(
                format!(" {} ", state_dot(app.state())),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                app.state().label(),
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
