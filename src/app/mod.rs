//! Application state, event handling and backend actions.

use crate::client::{self, Backend, BackendPreference, Details, Snapshot};
use crate::config::Config;
use crate::connection::Connection;
pub use crate::connection::SudoAction;
pub use crate::model::{Field, Page, VpnState};
use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;
use std::collections::VecDeque;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const MAX_LOGS: usize = 300;

mod actions;
mod config;
mod input;
mod messages;
mod profiles;

#[cfg(test)]
mod tests;

pub enum Msg {
    Snapshot(u64, u64, Snapshot),
    GpclientAuth(u64, Result<client::AuthResult, String>),
    GpclientDisconnected(u64, Result<(), String>),
    SudoAuth(u64, Result<(), String>),
    ConnectDone(u64, Result<Vec<String>, String>),
    DisconnectDone(u64, Result<Vec<String>, String>),
    GpaStarted(u64, Result<(), String>),
    LogCollected(u64, Result<String, String>),
    SslOnlyLoaded(u64, Result<bool, String>),
    SslOnlySet(u64, Result<bool, String>),
}

pub(super) fn normalize_portal(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

pub(super) fn systemctl_state(args: &[&str]) -> String {
    Command::new("systemctl")
        .args(args)
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_else(|_| "unavailable".to_string())
}

pub enum Modal {
    None,
    Help,
    Quit,
    DeleteProfile(String),
    Edit(Field),
    RenameProfile,
    Search,
    Sudo(SudoPrompt),
}

/// Password prompt shown inside the TUI. The password is kept as bytes so it
/// can be overwritten before release; it is never written to disk or logs.
pub struct SudoPrompt {
    pub password: Vec<u8>,
    pub error: Option<String>,
    pub inflight: bool,
}

impl SudoPrompt {
    pub(crate) fn new() -> Self {
        Self {
            password: Vec::new(),
            error: None,
            inflight: false,
        }
    }

    pub fn push_char(&mut self, character: char) {
        let mut buffer = [0u8; 4];
        self.password
            .extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
    }

    pub fn pop_char(&mut self) {
        if let Ok(text) = std::str::from_utf8(&self.password) {
            if let Some((index, _)) = text.char_indices().next_back() {
                self.password.truncate(index);
            }
        } else if !self.password.is_empty() {
            self.password.pop();
        }
    }
}

impl Drop for SudoPrompt {
    fn drop(&mut self) {
        self.password.fill(0);
    }
}

pub struct App {
    pub cfg: Config,
    pub preference: BackendPreference,
    pub backend: Backend,
    pub connection: Connection,
    pub details: Details,
    pub gpa_active: bool,
    pub installed: bool,
    pub error: Option<String>,
    pub logs: VecDeque<String>,
    pub log_collect_inflight: bool,
    pub poll_inflight: bool,
    poll_epoch: u64,
    pub modal: Modal,
    pub page: Page,
    pub config_index: usize,
    pub profile_index: usize,
    pub log_follow: bool,
    pub log_wrap: bool,
    pub log_search: Option<String>,
    pub log_from_bottom: usize,
    pub should_quit: bool,
    pub ssl_only: Option<bool>,
    pub ssl_only_inflight: bool,
    pub input: String,
    pub tick: u64,
    pub gpclient_log: Option<std::path::PathBuf>,
    notice: Option<(String, Instant)>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl App {
    pub fn new(mut cfg: Config) -> Self {
        let (tx, rx) = mpsc::channel();
        let preference = BackendPreference::parse(cfg.backend());
        cfg.set_backend(preference.as_str().to_string());
        let backend = client::detect_backend(preference);
        let mut app = Self {
            cfg,
            preference,
            backend,
            connection: Connection::new(if backend == Backend::None {
                VpnState::NoBackend
            } else {
                VpnState::Unknown
            }),
            details: Details::default(),
            gpa_active: false,
            installed: backend != Backend::None,
            error: None,
            logs: VecDeque::new(),
            log_collect_inflight: false,
            poll_inflight: false,
            poll_epoch: 0,
            modal: Modal::None,
            page: Page::Status,
            config_index: 0,
            profile_index: 0,
            log_follow: true,
            log_wrap: false,
            log_search: None,
            log_from_bottom: 0,
            should_quit: false,
            ssl_only: None,
            ssl_only_inflight: false,
            input: String::new(),
            tick: 0,
            gpclient_log: None,
            notice: None,
            tx,
            rx,
        };
        app.push_log(format!(
            "gp-tui v{} — unofficial GlobalProtect TUI",
            env!("CARGO_PKG_VERSION")
        ));
        app.push_log(format!("backend: {}", backend.label()));
        app.push_log(format!(
            "preference: {} (press 'b' to cycle)",
            app.preference.as_str()
        ));
        app.push_log("1 status · 2 config · 3 logs · c connect · d disconnect · q quit");
        if app.cfg.portal().trim().is_empty() {
            app.push_log("no portal configured; enter one and press Enter");
            app.begin_edit(Field::Portal);
        }
        app
    }

    pub fn state(&self) -> VpnState {
        self.connection.state()
    }

    pub fn report_startup_warning(&mut self, warning: String) {
        self.push_log(format!("config warning: {warning}"));
        self.set_notice("Config warning; see logs");
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal, shutdown: Arc<AtomicBool>) -> Result<()> {
        let mut last_poll = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(Instant::now);

        loop {
            if shutdown.load(Ordering::Relaxed) {
                self.shutdown_from_signal();
                break;
            }
            self.reap_gpclient();
            self.clear_notice();
            self.check_auth_timeout();
            self.maybe_load_ssl_only();
            terminal.draw(|frame| crate::ui::draw(frame, self))?;

            while let Ok(message) = self.rx.try_recv() {
                self.handle_msg(message);
            }

            if self.should_quit {
                break;
            }

            if event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press && self.handle_key(key) {
                        break;
                    }
                }
            }

            if !self.poll_inflight && last_poll.elapsed() >= Duration::from_secs(2) {
                self.spawn_refresh();
                last_poll = Instant::now();
            }

            self.tick = self.tick.wrapping_add(1);
        }

        Ok(())
    }

    fn reap_gpclient(&mut self) {
        let status = self.connection.reap_child();
        let Some(status) = status else {
            return;
        };

        self.push_log(format!("gpclient exited: {status}"));
        if !status.success() {
            let log = self
                .gpclient_log
                .as_ref()
                .map(|path| format!("; see {}", path.display()))
                .unwrap_or_default();
            self.error = Some(format!("gpclient exited with {status}{log}"));
        }
    }

    pub(super) fn push_log(&mut self, message: impl Into<String>) {
        let message = message.into();
        for line in message.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            self.logs.push_back(line.to_string());
            while self.logs.len() > MAX_LOGS {
                self.logs.pop_front();
            }
        }
    }

    pub(super) fn spawn_refresh(&mut self) {
        if self.poll_inflight {
            return;
        }
        self.poll_inflight = true;
        self.poll_epoch = self.poll_epoch.wrapping_add(1);
        let epoch = self.poll_epoch;
        let connection_epoch = self.connection.epoch();
        let tx = self.tx.clone();
        let preference = self.preference;
        thread::spawn(move || {
            let snapshot = client::snapshot(preference);
            let _ = tx.send(Msg::Snapshot(epoch, connection_epoch, snapshot));
        });
    }
}
