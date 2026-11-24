use super::*;
use crate::app::{App, Modal, SudoAction, SudoPrompt};
use crate::client::Backend;
use crate::config::Config;
use crate::model::{Page, VpnState};
use ratatui::backend::TestBackend;
use ratatui::style::Color;
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
    app.connection.reset(state);
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
        app.modal = Modal::Help;
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
    app.connection.reset(VpnState::Disconnected);
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
    app.connection.reset(VpnState::Disconnected);
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
    app.modal = Modal::Quit;
    let text = render(&app, 100, 32);
    assert!(text.contains("Quit?"));
    assert!(text.contains("Disconnect and quit"));
}

#[test]
fn renders_profile_delete_modal() {
    let mut app = test_app();
    app.page = Page::Profiles;
    app.modal = Modal::DeleteProfile("default".to_string());
    let text = render(&app, 100, 32);
    assert!(text.contains("Delete profile?"));
}

#[test]
fn renders_no_backend_setup() {
    let mut app = test_app();
    app.backend = Backend::None;
    app.installed = false;
    app.connection.reset(VpnState::NoBackend);
    let text = render(&app, 100, 30);
    assert!(text.contains("No GlobalProtect backend installed"));
}

#[test]
fn no_backend_setup_fits_minimum_terminal() {
    let mut app = test_app();
    app.backend = Backend::None;
    app.installed = false;
    app.connection.reset(VpnState::NoBackend);
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
    app.connection.reset(VpnState::NoBackend);
    app.error =
        Some("backend 'official' is not installed; install it or choose another backend".repeat(3));
    let text = render(&app, 60, 16);
    assert!(text.contains("[2] Profiles"));
}

#[test]
fn renders_sudo_modal_without_echoing_the_password() {
    let mut app = status_app(VpnState::WaitingSudo);
    let mut prompt = SudoPrompt::new();
    prompt.password = b"hunter2".to_vec();
    app.connection
        .begin_waiting_sudo(SudoAction::Connect, Some("cookie".to_string()));
    app.modal = Modal::Sudo(prompt);

    let text = render(&app, 80, 24);
    assert!(text.contains("sudo authentication"));
    assert!(text.contains("*******"));
    assert!(!text.contains("hunter2"));
    assert!(text.contains("[Enter] Authenticate"));
}

#[test]
fn sudo_modal_fits_minimum_terminal() {
    let mut app = status_app(VpnState::WaitingSudo);
    app.connection.begin_waiting_sudo(SudoAction::Quit, None);
    app.modal = Modal::Sudo(SudoPrompt::new());
    let text = render(&app, 60, 16);
    assert!(text.contains("sudo authentication"));
    assert!(text.contains("before quitting"));
    assert!(text.contains("[Esc] Cancel"));
}
