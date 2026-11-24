use super::*;
use crossterm::event::{KeyCode, KeyEvent};

#[test]
fn maps_snapshot_status_to_state() {
    assert_eq!(VpnState::from_snapshot("Connected"), VpnState::Connected);
    assert_eq!(
        VpnState::from_snapshot("Disconnected"),
        VpnState::Disconnected
    );
    assert_eq!(
        VpnState::from_snapshot("Not installed"),
        VpnState::NoBackend
    );
    assert_eq!(VpnState::from_snapshot("nonsense"), VpnState::Unknown);
}

#[test]
fn keeps_connect_error_after_disconnected_snapshot() {
    let mut config = Config::default();
    config
        .add_profile("default".to_string(), "vpn.example.com".to_string())
        .unwrap();
    let mut app = App::new(config);
    app.backend = Backend::Official;
    app.installed = true;
    app.connection.failed();
    app.error = Some("connect failed".to_string());

    app.handle_msg(Msg::Snapshot(
        app.poll_epoch,
        app.connection.epoch(),
        Snapshot {
            backend: Backend::Official,
            installed: true,
            status: "Disconnected".to_string(),
            details: Details::default(),
            gpa_active: false,
            error: None,
        },
    ));

    assert_eq!(app.state(), VpnState::Error);
    assert_eq!(app.error.as_deref(), Some("connect failed"));
}

#[test]
fn ignores_a_snapshot_from_the_previous_backend() {
    let mut app = test_app();
    app.backend = Backend::Gpclient;
    app.poll_epoch = 2;
    app.poll_inflight = true;

    app.handle_msg(Msg::Snapshot(
        1,
        app.connection.epoch(),
        Snapshot {
            backend: Backend::Official,
            installed: true,
            status: "Connected".to_string(),
            details: Details::default(),
            gpa_active: false,
            error: None,
        },
    ));

    assert_eq!(app.backend, Backend::Gpclient);
    assert!(app.poll_inflight);
}

#[test]
fn ignores_snapshot_from_before_a_connection_transition() {
    let mut app = test_app();
    app.connection.begin_official_connect();
    let current_epoch = app.connection.epoch();
    app.poll_epoch = 1;
    app.poll_inflight = true;

    app.handle_msg(Msg::Snapshot(
        1,
        current_epoch.wrapping_sub(1),
        Snapshot {
            backend: Backend::Official,
            installed: true,
            status: "Connected".to_string(),
            details: Details::default(),
            gpa_active: false,
            error: None,
        },
    ));

    assert_eq!(app.state(), VpnState::WaitingBrowser);
    assert!(!app.poll_inflight);
}

#[test]
fn ignores_a_connect_result_from_an_old_transition() {
    let mut app = test_app();
    let old_epoch = app.connection.begin_official_connect();
    app.connection.begin_disconnect();

    app.handle_msg(Msg::ConnectDone(
        old_epoch,
        Ok(vec!["connected".to_string()]),
    ));

    assert_eq!(app.state(), VpnState::Disconnecting);
}

#[test]
fn gpclient_process_keeps_a_disconnected_snapshot_in_connecting_state() {
    let mut connection = Connection::new(VpnState::Disconnected);
    connection.reconcile(VpnState::Disconnected, true);
    assert_eq!(connection.state(), VpnState::Connecting);
}

#[test]
fn normalizes_portal() {
    assert_eq!(
        normalize_portal(" https://vpn.example.com/ "),
        "vpn.example.com"
    );
    assert_eq!(
        normalize_portal("http://vpn.example.com"),
        "vpn.example.com"
    );
    assert_eq!(
        normalize_portal("vpn.example.com:443"),
        "vpn.example.com:443"
    );
}

fn test_app() -> App {
    let mut config = Config::default();
    config
        .add_profile("default".to_string(), "vpn.example.com".to_string())
        .unwrap();
    let mut app = App::new(config);
    app.backend = Backend::Gpclient;
    app
}

#[test]
fn sudo_prompt_edits_a_masked_password() {
    let mut app = test_app();
    app.connection
        .begin_waiting_sudo(SudoAction::Connect, Some("cookie".to_string()));
    app.open_sudo_prompt();

    for character in "pä".chars() {
        app.handle_sudo_key(KeyEvent::from(KeyCode::Char(character)));
    }
    app.handle_sudo_key(KeyEvent::from(KeyCode::Backspace));

    let prompt = app.sudo_prompt().unwrap();
    assert_eq!(prompt.password, b"p");
    assert!(prompt.error.is_none());
}

#[test]
fn empty_sudo_password_is_rejected_inline() {
    let mut app = test_app();
    app.connection
        .begin_waiting_sudo(SudoAction::Connect, Some("cookie".to_string()));
    app.open_sudo_prompt();
    app.submit_sudo();

    let prompt = app.sudo_prompt().unwrap();
    assert_eq!(prompt.error.as_deref(), Some("password cannot be empty"));
    assert!(!prompt.inflight);
}

#[test]
fn cancelling_connect_prompt_drops_the_cookie_and_resets_state() {
    let mut app = test_app();
    app.begin_sudo_connect("cookie".to_string());

    app.handle_sudo_key(KeyEvent::from(KeyCode::Esc));

    assert!(app.sudo_prompt().is_none());
    assert_eq!(app.state(), VpnState::Disconnected);
}

#[test]
fn reentering_sudo_connect_keeps_the_typed_password() {
    let mut app = test_app();
    app.begin_sudo_connect("first".to_string());
    app.handle_sudo_key(KeyEvent::from(KeyCode::Char('x')));

    // The run loop used to call this again on every tick.
    app.begin_sudo_connect("second".to_string());

    let prompt = app.sudo_prompt().unwrap();
    assert_eq!(prompt.password, b"x");
    assert_eq!(app.state(), VpnState::WaitingSudo);
    assert_eq!(app.connection.sudo_action(), Some(SudoAction::Connect));
}

#[test]
fn expired_sudo_on_disconnect_opens_the_password_prompt() {
    let mut app = test_app();
    let epoch = app.connection.begin_disconnect();

    app.handle_msg(Msg::GpclientDisconnected(
        epoch,
        Err("sudo: a password is required".to_string()),
    ));

    assert_eq!(app.state(), VpnState::WaitingSudo);
    assert!(app.state().is_busy());
    assert_eq!(app.connection.sudo_action(), Some(SudoAction::Disconnect));
}

#[test]
fn cancelling_disconnect_prompt_keeps_the_tunnel_state() {
    let mut app = test_app();
    app.connection
        .begin_waiting_sudo(SudoAction::Disconnect, None);
    app.open_sudo_prompt();

    app.handle_sudo_key(KeyEvent::from(KeyCode::Esc));

    assert_eq!(app.state(), VpnState::Connected);
}
