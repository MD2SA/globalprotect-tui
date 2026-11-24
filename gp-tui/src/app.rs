use crate::client::{self, Backend, BackendPreference, Details, Snapshot};
use crate::config::Config;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use std::collections::VecDeque;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const MAX_LOGS: usize = 300;

pub enum Msg {
    Snapshot(Snapshot),
    AuthPid(u64, u32),
    GpclientAuth(u64, Result<client::AuthResult, String>),
    GpclientDisconnected(Result<(), String>),
    SudoAuth(u64, Result<(), String>),
    ConnectDone(Result<Vec<String>, String>),
    DisconnectDone(Result<Vec<String>, String>),
    GpaStarted(Result<(), String>),
    LogCollected(Result<String, String>),
    SslOnlyLoaded(Result<bool, String>),
    SslOnlySet(Result<bool, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Portal,
    Username,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Status,
    Profiles,
    Config,
    Logs,
}

impl Page {
    pub fn next_visible(self) -> Self {
        match self {
            Self::Status => Self::Profiles,
            Self::Profiles | Self::Config => Self::Logs,
            Self::Logs => Self::Status,
        }
    }

    pub fn previous_visible(self) -> Self {
        match self {
            Self::Status => Self::Logs,
            Self::Profiles => Self::Status,
            Self::Logs | Self::Config => Self::Profiles,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VpnState {
    NoBackend,
    Disconnected,
    WaitingBrowser,
    WaitingSudo,
    Connecting,
    Connected,
    Disconnecting,
    Error,
    Unknown,
}

impl VpnState {
    pub fn from_snapshot(status: &str) -> Self {
        match status {
            "Connected" => Self::Connected,
            "Disconnected" => Self::Disconnected,
            "Connecting" => Self::Connecting,
            "Error" => Self::Error,
            "Not installed" => Self::NoBackend,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NoBackend => "No backend",
            Self::Disconnected => "Disconnected",
            Self::WaitingBrowser => "Waiting for browser",
            Self::WaitingSudo => "Waiting for sudo",
            Self::Connecting => "Connecting",
            Self::Connected => "Connected",
            Self::Disconnecting => "Disconnecting",
            Self::Error => "Error",
            Self::Unknown => "Unknown",
        }
    }

    pub fn is_busy(self) -> bool {
        matches!(
            self,
            Self::WaitingBrowser | Self::WaitingSudo | Self::Connecting | Self::Disconnecting
        )
    }

    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Connected
                | Self::WaitingBrowser
                | Self::WaitingSudo
                | Self::Connecting
                | Self::Disconnecting
        )
    }
}

fn reconcile_state(current: VpnState, snapshot: VpnState, gpclient_running: bool) -> VpnState {
    let next = match (current, snapshot) {
        (VpnState::WaitingBrowser, VpnState::Disconnected) => VpnState::WaitingBrowser,
        (VpnState::WaitingSudo, VpnState::Disconnected) => VpnState::WaitingSudo,
        (VpnState::Disconnecting, VpnState::Connected) => VpnState::Disconnecting,
        (VpnState::Error, VpnState::Disconnected) => VpnState::Error,
        (_, state) => state,
    };

    if gpclient_running && next == VpnState::Disconnected {
        VpnState::Connecting
    } else {
        next
    }
}

fn normalize_portal(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

fn systemctl_state(args: &[&str]) -> String {
    Command::new("systemctl")
        .args(args)
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_else(|_| "unavailable".to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Edit(Field),
    RenameProfile,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SudoAction {
    Connect,
    Disconnect,
    Quit,
}

/// Password prompt shown inside the TUI. The password is kept as bytes so it
/// can be overwritten before release; it is never written to disk or logs.
pub struct SudoPrompt {
    pub action: SudoAction,
    pub password: Vec<u8>,
    pub error: Option<String>,
    pub inflight: bool,
}

impl SudoPrompt {
    pub(crate) fn new(action: SudoAction) -> Self {
        Self {
            action,
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
    pub state: VpnState,
    pub details: Details,
    pub gpa_active: bool,
    pub installed: bool,
    pub error: Option<String>,
    pub logs: VecDeque<String>,
    pub busy: bool,
    pub poll_inflight: bool,
    pub mode: Mode,
    pub page: Page,
    pub config_index: usize,
    pub profile_index: usize,
    pub profile_delete: Option<String>,
    pub log_follow: bool,
    pub log_wrap: bool,
    pub log_search: Option<String>,
    pub log_from_bottom: usize,
    pub help: bool,
    pub quit_modal: bool,
    pub sudo_prompt: Option<SudoPrompt>,
    pub should_quit: bool,
    pub ssl_only: Option<bool>,
    pub ssl_only_inflight: bool,
    pub input: String,
    pub tick: u64,
    pub gpclient_log: Option<std::path::PathBuf>,
    gpclient_child: Option<std::process::Child>,
    pending_cookie: Option<String>,
    sudo_cookie: Option<String>,
    auth_epoch: u64,
    sudo_epoch: u64,
    pub auth_pid: Option<u32>,
    auth_started: Option<Instant>,
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
            state: if backend == Backend::None {
                VpnState::NoBackend
            } else {
                VpnState::Unknown
            },
            details: Details::default(),
            gpa_active: false,
            installed: backend != Backend::None,
            error: None,
            logs: VecDeque::new(),
            busy: false,
            poll_inflight: false,
            mode: Mode::Normal,
            page: Page::Status,
            config_index: 0,
            profile_index: 0,
            profile_delete: None,
            log_follow: true,
            log_wrap: false,
            log_search: None,
            log_from_bottom: 0,
            help: false,
            quit_modal: false,
            sudo_prompt: None,
            should_quit: false,
            ssl_only: None,
            ssl_only_inflight: false,
            input: String::new(),
            tick: 0,
            gpclient_log: None,
            gpclient_child: None,
            pending_cookie: None,
            sudo_cookie: None,
            auth_epoch: 0,
            sudo_epoch: 0,
            auth_pid: None,
            auth_started: None,
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

            if let Some(cookie) = self.pending_cookie.take() {
                self.finish_gpclient_connect(cookie);
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
        let status = match self.gpclient_child.as_mut() {
            Some(child) => child.try_wait().ok().flatten(),
            None => None,
        };
        let Some(status) = status else {
            return;
        };

        self.gpclient_child = None;
        self.push_log(format!("gpclient exited: {status}"));
        if !status.success() {
            self.state = VpnState::Error;
            let log = self
                .gpclient_log
                .as_ref()
                .map(|path| format!("; see {}", path.display()))
                .unwrap_or_default();
            self.error = Some(format!("gpclient exited with {status}{log}"));
        } else if self.state == VpnState::Connecting {
            self.state = VpnState::Disconnected;
        }
    }

    fn push_log(&mut self, message: impl Into<String>) {
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

    fn spawn_refresh(&mut self) {
        if self.poll_inflight {
            return;
        }
        self.poll_inflight = true;
        let tx = self.tx.clone();
        let preference = self.preference;
        thread::spawn(move || {
            let snapshot = client::snapshot(preference);
            let _ = tx.send(Msg::Snapshot(snapshot));
        });
    }

    fn spawn_connect(&mut self) {
        if self.backend != Backend::Official {
            self.push_log("official globalprotect CLI not found".to_string());
            return;
        }
        if self.busy {
            self.push_log("already connecting or disconnecting".to_string());
            return;
        }
        if !self.installed {
            self.push_log(
                "globalprotect not found in PATH; install it with gp_install_arch.sh first"
                    .to_string(),
            );
            return;
        }
        if self.cfg.portal().trim().is_empty() {
            self.push_log("portal is empty; press 'p' to configure it".to_string());
            return;
        }
        if self.cfg.username().trim().is_empty() {
            self.push_log(
                "warning: username is empty; press 'u' if the client asks for one".to_string(),
            );
        }

        self.busy = true;
        self.error = None;
        self.state = VpnState::WaitingBrowser;
        self.push_log(format!(
            "> globalprotect connect --portal {}",
            self.cfg.portal().trim()
        ));
        self.push_log("waiting for the browser (SSO/MFA)...".to_string());

        let tx = self.tx.clone();
        let portal = self.cfg.portal().to_string();
        let username = self.cfg.username().to_string();
        thread::spawn(move || {
            let result = client::connect(&portal, &username);
            let _ = tx.send(Msg::ConnectDone(result));
        });
    }

    fn spawn_disconnect(&mut self) {
        if self.backend != Backend::Official {
            self.push_log("official globalprotect CLI not found".to_string());
            return;
        }
        if self.busy {
            self.push_log("already connecting or disconnecting".to_string());
            return;
        }
        self.busy = true;
        self.error = None;
        self.state = VpnState::Disconnecting;
        self.push_log("> globalprotect disconnect".to_string());

        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::disconnect();
            let _ = tx.send(Msg::DisconnectDone(result));
        });
    }

    fn cancel_auth(&mut self, timeout: bool) {
        self.auth_epoch = self.auth_epoch.wrapping_add(1);
        if let Some(pid) = self.auth_pid.take() {
            let _ = Command::new("kill")
                .args(["-INT", &pid.to_string()])
                .status();
        } else {
            let _ = Command::new("pkill")
                .args(["-INT", "-f", "gpauth"])
                .status();
        }
        self.auth_started = None;
        self.busy = false;
        self.pending_cookie = None;
        self.state = VpnState::Disconnected;
        self.error = if timeout {
            Some("authentication timed out".to_string())
        } else {
            None
        };
        let message = if timeout {
            "Authentication timed out"
        } else {
            "Authentication cancelled"
        };
        self.push_log(message.to_string());
        self.set_notice(message);
    }

    fn check_auth_timeout(&mut self) {
        if self.state == VpnState::WaitingBrowser
            && self
                .auth_started
                .is_some_and(|started| started.elapsed() > Duration::from_secs(180))
        {
            self.cancel_auth(true);
        }
    }

    fn gpclient_connect(&mut self) {
        if self.busy {
            self.push_log("already connecting or disconnecting".to_string());
            return;
        }
        if self.gpclient_child.is_some() {
            self.push_log("gpclient is already running; press 'd' to disconnect".to_string());
            return;
        }
        if self.cfg.portal().trim().is_empty() {
            self.push_log("portal is empty; press 'p' to configure it".to_string());
            return;
        }

        self.busy = true;
        self.error = None;
        self.state = VpnState::WaitingBrowser;
        self.auth_epoch = self.auth_epoch.wrapping_add(1);
        let epoch = self.auth_epoch;
        self.auth_pid = None;
        self.auth_started = None;
        self.push_log(format!("> gpauth {} --browser", self.cfg.portal().trim()));
        self.push_log("browser will open; complete SSO/MFA. The TUI stays visible.".to_string());

        let tx = self.tx.clone();
        let pid_tx = self.tx.clone();
        let portal = self.cfg.portal().to_string();
        thread::spawn(move || {
            let result = client::gpclient_auth(&portal, move |pid| {
                let _ = pid_tx.send(Msg::AuthPid(epoch, pid));
            });
            let _ = tx.send(Msg::GpclientAuth(epoch, result));
        });
    }

    fn finish_gpclient_connect(&mut self, cookie: String) {
        self.push_log("SSO done; starting gpclient".to_string());

        if client::sudo_connect_ready() {
            self.start_gpclient(cookie);
            return;
        }

        self.begin_sudo_connect(cookie);
    }

    /// Holds the SSO cookie until sudo authentication finishes. Keeping it out
    /// of `pending_cookie` matters: the run loop drains that field every tick,
    /// so re-arming it would reopen the prompt (and wipe what was typed).
    fn begin_sudo_connect(&mut self, cookie: String) {
        self.state = VpnState::WaitingSudo;
        self.sudo_cookie = Some(cookie);
        if self.sudo_prompt.is_none() {
            self.push_log("sudo needs a password; enter it in the prompt".to_string());
            self.open_sudo_prompt(SudoAction::Connect);
        }
    }

    fn start_gpclient(&mut self, cookie: String) {
        self.push_log(format!(
            "> sudo -n gpclient connect {}",
            self.cfg.portal().trim()
        ));
        let result = client::gpclient_start(&cookie, self.cfg.portal(), self.cfg.username());

        self.busy = false;
        match result {
            Ok(child) => {
                self.gpclient_child = Some(child);
                self.gpclient_log = client::gpclient_log_path();
                self.state = VpnState::Connecting;
                self.push_log("gpclient started in the background".to_string());
                self.set_notice("gpclient started");
                if let Some(path) = &self.gpclient_log {
                    self.push_log(format!("gpclient log: {}", path.display()));
                }
            }
            Err(error) => {
                self.push_log(format!("gpclient connect failed: {error}"));
                self.error = Some(error);
                self.state = VpnState::Error;
            }
        }
        self.spawn_refresh();
    }

    fn gpclient_disconnect(&mut self) {
        if self.busy {
            self.push_log("already connecting or disconnecting".to_string());
            return;
        }

        // Try the cached timestamp first. If it expired, the worker reports
        // it and Msg::GpclientDisconnected opens the password prompt.
        self.begin_gpclient_disconnect();
    }

    fn begin_gpclient_disconnect(&mut self) {
        self.busy = true;
        self.error = None;
        self.state = VpnState::Disconnecting;
        self.push_log("> sudo -n gpclient disconnect".to_string());

        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::gpclient_disconnect();
            let _ = tx.send(Msg::GpclientDisconnected(result));
        });
    }

    fn open_sudo_prompt(&mut self, action: SudoAction) {
        self.sudo_prompt = Some(SudoPrompt::new(action));
    }

    fn handle_sudo_key(&mut self, key: KeyEvent) -> bool {
        if self.sudo_prompt.is_none() {
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.cancel_sudo_prompt();
            return false;
        }
        match key.code {
            KeyCode::Enter => self.submit_sudo(),
            KeyCode::Esc => self.cancel_sudo_prompt(),
            KeyCode::Backspace => {
                if let Some(prompt) = self.sudo_prompt.as_mut() {
                    prompt.pop_char();
                }
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(prompt) = self.sudo_prompt.as_mut() {
                    prompt.push_char(character);
                }
            }
            _ => {}
        }
        false
    }

    fn submit_sudo(&mut self) {
        let Some(prompt) = self.sudo_prompt.as_mut() else {
            return;
        };
        if prompt.inflight {
            return;
        }
        if prompt.password.is_empty() {
            prompt.error = Some("password cannot be empty".to_string());
            return;
        }

        prompt.inflight = true;
        prompt.error = None;
        self.sudo_epoch = self.sudo_epoch.wrapping_add(1);
        let epoch = self.sudo_epoch;
        let mut password = std::mem::take(&mut prompt.password);
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::sudo_authenticate(&password);
            password.fill(0);
            let _ = tx.send(Msg::SudoAuth(epoch, result));
        });
    }

    fn cancel_sudo_prompt(&mut self) {
        let Some(prompt) = self.sudo_prompt.take() else {
            return;
        };
        self.sudo_epoch = self.sudo_epoch.wrapping_add(1);
        self.busy = false;
        self.pending_cookie = None;
        self.sudo_cookie = None;
        self.state = match prompt.action {
            SudoAction::Connect => VpnState::Disconnected,
            SudoAction::Disconnect | SudoAction::Quit => VpnState::Connected,
        };
        self.push_log("sudo authentication cancelled".to_string());
        self.set_notice("Sudo cancelled");
    }

    fn sudo_authenticated(&mut self) {
        let Some(prompt) = self.sudo_prompt.take() else {
            return;
        };
        self.push_log("sudo authenticated".to_string());

        match prompt.action {
            SudoAction::Connect => {
                if let Some(cookie) = self.sudo_cookie.take() {
                    self.start_gpclient(cookie);
                } else {
                    self.busy = false;
                    self.state = VpnState::Disconnected;
                    self.push_log("authentication cookie expired; reconnect".to_string());
                    self.set_notice("Authentication expired");
                }
            }
            SudoAction::Disconnect => self.begin_gpclient_disconnect(),
            SudoAction::Quit => {
                if self.gpclient_child.is_some() || self.state.is_active() {
                    let result = client::gpclient_disconnect();
                    if let Err(error) = result {
                        self.push_log(format!("disconnect before quit failed: {error}"));
                    }
                }
                self.busy = false;
                self.should_quit = true;
            }
        }
    }

    fn spawn_start_gpa(&mut self) {
        if self.backend != Backend::Official {
            self.push_log("gpa is only used by the official backend".to_string());
            return;
        }
        self.push_log("> systemctl --user start gpa.service".to_string());
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::start_gpa();
            let _ = tx.send(Msg::GpaStarted(result));
        });
    }

    fn spawn_collect_log(&mut self) {
        if self.backend == Backend::Gpclient {
            match client::gpclient_log_tail(40) {
                Ok(lines) if lines.is_empty() => {
                    self.push_log("gpclient log is empty".to_string());
                }
                Ok(lines) => {
                    for line in lines {
                        self.push_log(format!("gpclient: {line}"));
                    }
                }
                Err(error) => self.push_log(format!("could not read gpclient log: {error}")),
            }
            return;
        }
        if self.backend != Backend::Official {
            self.push_log("no backend installed".to_string());
            return;
        }
        if self.busy {
            self.push_log("wait for the current operation to finish".to_string());
            return;
        }
        self.busy = true;
        self.push_log("> globalprotect collect-log".to_string());
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::collect_log();
            let _ = tx.send(Msg::LogCollected(result));
        });
    }

    fn maybe_load_ssl_only(&mut self) {
        if self.page != Page::Config
            || self.backend != Backend::Official
            || self.ssl_only.is_some()
            || self.ssl_only_inflight
        {
            return;
        }
        self.ssl_only_inflight = true;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::ssl_only_status();
            let _ = tx.send(Msg::SslOnlyLoaded(result));
        });
    }

    fn toggle_ssl_only(&mut self) {
        if self.backend != Backend::Official {
            self.push_log("SSL-only is only available with the official backend".to_string());
            return;
        }
        let Some(current) = self.ssl_only else {
            self.push_log("SSL-only status is still loading".to_string());
            return;
        };
        if self.ssl_only_inflight {
            return;
        }
        self.ssl_only_inflight = true;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::toggle_ssl_only(current);
            let _ = tx.send(Msg::SslOnlySet(result));
        });
    }

    fn begin_edit(&mut self, field: Field) {
        self.mode = Mode::Edit(field);
        self.input = match field {
            Field::Portal => self.cfg.portal().to_string(),
            Field::Username => self.cfg.username().to_string(),
        };
    }

    fn commit_edit(&mut self) {
        let Mode::Edit(field) = self.mode else {
            return;
        };
        let mut value = self.input.trim().to_string();
        if field == Field::Portal {
            value = normalize_portal(&value);
            if value.is_empty() {
                self.error = Some("Portal cannot be empty".to_string());
                self.set_notice("Portal cannot be empty");
                return;
            }
        }
        match field {
            Field::Portal => self.cfg.set_portal(value),
            Field::Username => self.cfg.set_username(value),
        }
        if let Err(error) = self.cfg.save() {
            self.push_log(format!("could not save config: {error}"));
            self.error = Some(error.to_string());
        } else {
            self.push_log("config saved".to_string());
            self.error = None;
            self.set_notice("Config saved");
        }
        self.mode = Mode::Normal;
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.sudo_prompt.is_some() {
            return self.handle_sudo_key(key);
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            return self.request_quit();
        }

        if self.help {
            if matches!(key.code, KeyCode::Char('?') | KeyCode::Esc) {
                self.help = false;
            }
            return false;
        }

        if self.quit_modal {
            return self.handle_quit_modal(key);
        }

        if self.profile_delete.is_some() {
            return self.handle_profile_delete(key);
        }

        if let Mode::Search = self.mode {
            match key.code {
                KeyCode::Enter => {
                    let query = self.input.trim().to_string();
                    self.log_search = if query.is_empty() { None } else { Some(query) };
                    self.log_follow = true;
                    self.log_from_bottom = 0;
                    self.mode = Mode::Normal;
                }
                KeyCode::Esc => {
                    self.mode = Mode::Normal;
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Char(character) => self.input.push(character),
                _ => {}
            }
            return false;
        }

        if let Mode::RenameProfile = self.mode {
            match key.code {
                KeyCode::Enter => self.commit_profile_rename(),
                KeyCode::Esc => {
                    self.mode = Mode::Normal;
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Char(character) => self.input.push(character),
                _ => {}
            }
            return false;
        }

        if let Mode::Edit(_) = self.mode {
            match key.code {
                KeyCode::Enter => self.commit_edit(),
                KeyCode::Esc => {
                    self.mode = Mode::Normal;
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Char(character) => self.input.push(character),
                _ => {}
            }
            return false;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('d') if self.page == Page::Logs => self.scroll_logs(-10),
                KeyCode::Char('u') if self.page == Page::Logs => self.scroll_logs(10),
                _ => {}
            }
            return false;
        }

        match key.code {
            KeyCode::Char('?') => {
                self.help = true;
                return false;
            }
            KeyCode::Tab => {
                self.page = self.page.next_visible();
                return false;
            }
            KeyCode::BackTab => {
                self.page = self.page.previous_visible();
                return false;
            }
            KeyCode::Char('1') => {
                self.page = Page::Status;
                return false;
            }
            KeyCode::Char('2') => {
                self.page = Page::Profiles;
                return false;
            }
            KeyCode::Char('3') => {
                self.page = Page::Logs;
                return false;
            }
            KeyCode::Char('h') => {
                self.page = self.page.previous_visible();
                return false;
            }
            KeyCode::Char('l') => {
                self.page = self.page.next_visible();
                return false;
            }
            KeyCode::Char('q') => return self.request_quit(),
            KeyCode::Esc => {
                if self.state == VpnState::WaitingBrowser {
                    self.cancel_auth(false);
                    return false;
                }
                if self.page == Page::Logs && self.log_search.is_some() {
                    self.log_search = None;
                    self.log_follow = true;
                    self.log_from_bottom = 0;
                } else if self.page == Page::Config {
                    self.page = Page::Profiles;
                } else if self.page == Page::Status {
                    return self.request_quit();
                } else {
                    self.page = Page::Status;
                }
                return false;
            }
            KeyCode::Char('n') if self.page == Page::Profiles => self.new_profile(),
            KeyCode::Char('R') if self.page == Page::Profiles => self.begin_rename_profile(),
            KeyCode::Char('e') if self.page == Page::Profiles => self.edit_selected_profile(),
            KeyCode::Char('D') if self.page == Page::Profiles => self.request_delete_profile(),
            KeyCode::Enter if self.page == Page::Profiles => self.activate_selected_profile(),
            KeyCode::Char('c') => match self.backend {
                Backend::Gpclient => self.gpclient_connect(),
                Backend::Official => self.spawn_connect(),
                Backend::None => self.push_log(
                    "no backend installed: install globalprotect or globalprotect-openconnect"
                        .to_string(),
                ),
            },
            KeyCode::Char('d') => match self.backend {
                Backend::Gpclient => self.gpclient_disconnect(),
                Backend::Official => self.spawn_disconnect(),
                Backend::None => self.push_log(
                    "no backend installed: install globalprotect or globalprotect-openconnect"
                        .to_string(),
                ),
            },
            KeyCode::Char('r') => self.spawn_refresh(),
            KeyCode::Char('y') => self.write_diagnostics(),
            KeyCode::Char('b') => self.cycle_backend(),
            KeyCode::Char('g') => self.spawn_start_gpa(),
            KeyCode::Char('/') => {
                self.page = Page::Logs;
                self.mode = Mode::Search;
                self.input = self.log_search.clone().unwrap_or_default();
            }
            KeyCode::Char('L') => {
                self.page = Page::Logs;
                self.spawn_collect_log();
            }
            KeyCode::Char('p') => {
                self.page = Page::Config;
                self.config_index = 0;
                self.begin_edit(Field::Portal);
            }
            KeyCode::Char('u') => {
                self.page = Page::Config;
                self.config_index = 1;
                self.begin_edit(Field::Username);
            }
            KeyCode::Char('j') => match self.page {
                Page::Profiles => self.next_profile(),
                Page::Config => {
                    self.config_index = (self.config_index + 1) % 4;
                }
                Page::Logs => self.scroll_logs(-1),
                Page::Status => {}
            },
            KeyCode::Char('k') => match self.page {
                Page::Profiles => self.previous_profile(),
                Page::Config => {
                    self.config_index = (self.config_index + 3) % 4;
                }
                Page::Logs => self.scroll_logs(1),
                Page::Status => {}
            },
            KeyCode::Char('G') if self.page == Page::Logs => {
                self.log_follow = true;
                self.log_from_bottom = 0;
            }
            KeyCode::Up => match self.page {
                Page::Profiles => self.previous_profile(),
                Page::Config => {
                    self.config_index = (self.config_index + 3) % 4;
                }
                Page::Logs => self.scroll_logs(1),
                Page::Status => {}
            },
            KeyCode::Down => match self.page {
                Page::Profiles => self.next_profile(),
                Page::Config => {
                    self.config_index = (self.config_index + 1) % 4;
                }
                Page::Logs => self.scroll_logs(-1),
                Page::Status => {}
            },
            KeyCode::PageUp if self.page == Page::Logs => self.scroll_logs(10),
            KeyCode::PageDown if self.page == Page::Logs => self.scroll_logs(-10),
            KeyCode::Home if self.page == Page::Logs => {
                self.log_follow = false;
                self.log_from_bottom = self.logs.len();
            }
            KeyCode::End if self.page == Page::Logs => {
                self.log_follow = true;
                self.log_from_bottom = 0;
            }
            KeyCode::Char('f') if self.page == Page::Logs => {
                self.log_follow = !self.log_follow;
                if self.log_follow {
                    self.log_from_bottom = 0;
                }
            }
            KeyCode::Char('w') if self.page == Page::Logs => {
                self.log_wrap = !self.log_wrap;
                self.set_notice(if self.log_wrap {
                    "Log wrap on"
                } else {
                    "Log wrap off"
                });
            }
            KeyCode::Enter if self.page == Page::Config => match self.config_index {
                0 => self.begin_edit(Field::Portal),
                1 => self.begin_edit(Field::Username),
                2 => self.cycle_backend(),
                _ => self.toggle_ssl_only(),
            },
            _ => {}
        }
        false
    }

    fn scroll_logs(&mut self, delta: isize) {
        if delta > 0 {
            self.log_follow = false;
            self.log_from_bottom = self.log_from_bottom.saturating_add(delta as usize);
        } else {
            self.log_from_bottom = self.log_from_bottom.saturating_sub((-delta) as usize);
            if self.log_from_bottom == 0 {
                self.log_follow = true;
            }
        }
    }

    /// Best-effort cleanup when the terminal is closed or the process is
    /// signalled. There is no terminal left to prompt on, so only a cached
    /// sudo timestamp can disconnect a gpclient tunnel started by this TUI.
    fn shutdown_from_signal(&mut self) {
        if self.auth_pid.is_some() || self.state == VpnState::WaitingBrowser {
            self.cancel_auth(false);
        }

        match self.backend {
            Backend::Gpclient if self.gpclient_child.is_some() => {
                let _ = client::gpclient_disconnect();
                self.gpclient_child = None;
            }
            Backend::Official if self.state.is_active() => {
                let _ = client::disconnect();
            }
            _ => {}
        }
    }

    fn request_quit(&mut self) -> bool {
        if self.state.is_active() || self.gpclient_child.is_some() || self.busy {
            self.help = false;
            self.quit_modal = true;
            return false;
        }
        true
    }

    fn handle_quit_modal(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('d') => {
                self.quit_modal = false;
                self.quit_and_disconnect();
                self.should_quit
            }
            KeyCode::Char('q') => true,
            KeyCode::Esc => {
                self.quit_modal = false;
                false
            }
            _ => false,
        }
    }

    fn quit_and_disconnect(&mut self) {
        match self.backend {
            Backend::Gpclient => {
                if self.gpclient_child.is_some() || self.state.is_active() {
                    match client::gpclient_disconnect() {
                        Ok(()) => self.should_quit = true,
                        Err(error) if client::sudo_needs_password(&error) => {
                            self.busy = true;
                            self.error = None;
                            self.state = VpnState::WaitingSudo;
                            self.push_log(
                                "sudo needs a password; enter it in the prompt".to_string(),
                            );
                            self.open_sudo_prompt(SudoAction::Quit);
                        }
                        Err(error) => {
                            self.push_log(format!("disconnect before quit failed: {error}"));
                            self.should_quit = true;
                        }
                    }
                } else {
                    self.should_quit = true;
                }
            }
            Backend::Official => {
                if self.state.is_active() {
                    if let Err(error) = client::disconnect() {
                        self.push_log(format!("disconnect before quit failed: {error}"));
                    }
                }
                self.should_quit = true;
            }
            Backend::None => self.should_quit = true,
        }
    }

    fn set_notice(&mut self, message: impl Into<String>) {
        self.notice = Some((message.into(), Instant::now() + Duration::from_secs(3)));
    }

    fn write_diagnostics(&mut self) {
        let mut text = format!(
            "gp-tui {}\nstate: {}\nbackend: {}\npreference: {}\nportal: {}\nuser: {}\noverall: {}\n",
            env!("CARGO_PKG_VERSION"),
            self.state.label(),
            self.backend.label(),
            self.preference.as_str(),
            self.cfg.portal(),
            self.cfg.username(),
            if self.installed { "installed" } else { "missing" },
        );
        text.push_str(&format!(
            "ip: {}\ngateway: {}\ngateway_ip: {}\nprotocol: {}\nuptime: {}\n",
            self.details.ip,
            self.details.gateway,
            self.details.gateway_ip,
            self.details.protocol,
            self.details.uptime,
        ));
        text.push_str(&format!(
            "gpa: {}\ngpd: {}\n",
            systemctl_state(&["--user", "is-active", "gpa.service"]),
            systemctl_state(&["is-active", "gpd.service"]),
        ));
        if let Some(error) = &self.error {
            text.push_str(&format!("error: {error}\n"));
        }
        if let Some(path) = &self.gpclient_log {
            text.push_str(&format!("gpclient_log: {}\n", path.display()));
            if let Ok(lines) = client::gpclient_log_tail(60) {
                text.push_str("--- gpclient.log ---\n");
                for line in lines {
                    text.push_str(&line);
                    text.push('\n');
                }
            }
        }

        let Some(cache) = dirs::cache_dir() else {
            self.error = Some("cannot determine cache directory".to_string());
            return;
        };
        let dir = cache.join("gp-tui");
        if let Err(error) = std::fs::create_dir_all(&dir) {
            self.error = Some(format!("creating {}: {error}", dir.display()));
            return;
        }
        let path = dir.join("diagnostics.txt");
        if let Err(error) = std::fs::write(&path, text) {
            self.error = Some(format!("writing {}: {error}", path.display()));
            return;
        }
        if let Ok(metadata) = std::fs::metadata(&path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            let _ = std::fs::set_permissions(&path, permissions);
        }
        self.push_log(format!("diagnostics written to {}", path.display()));
        self.set_notice(format!("Diagnostics: {}", path.display()));
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice
            .as_ref()
            .filter(|(_, until)| Instant::now() < *until)
            .map(|(message, _)| message.as_str())
    }

    fn clear_notice(&mut self) {
        if let Some((_, until)) = &self.notice {
            if Instant::now() >= *until {
                self.notice = None;
            }
        }
    }

    fn cycle_backend(&mut self) {
        if self.busy || self.gpclient_child.is_some() {
            self.push_log(
                "finish or disconnect the current operation before switching backends".to_string(),
            );
            return;
        }

        self.preference = self.preference.next();
        self.cfg.set_backend(self.preference.as_str().to_string());
        if let Err(error) = self.cfg.save() {
            self.push_log(format!("could not save config: {error}"));
        }

        self.backend = client::detect_backend(self.preference);
        self.installed = self.backend != Backend::None;
        self.ssl_only = None;
        self.ssl_only_inflight = false;
        self.state = if self.installed {
            VpnState::Unknown
        } else {
            VpnState::NoBackend
        };
        self.error = if self.installed {
            None
        } else {
            Some(format!(
                "backend '{}' is not installed",
                self.preference.as_str()
            ))
        };
        self.push_log(format!(
            "backend preference: {} ({})",
            self.preference.as_str(),
            self.preference.label()
        ));
        self.set_notice(format!("Backend: {}", self.preference.as_str()));
        self.spawn_refresh();
    }

    fn selected_profile_name(&self) -> Option<String> {
        self.cfg
            .profiles
            .get(self.profile_index)
            .map(|profile| profile.name.clone())
    }

    fn next_profile(&mut self) {
        if self.cfg.profiles.is_empty() {
            return;
        }
        self.profile_index = (self.profile_index + 1) % self.cfg.profiles.len();
    }

    fn previous_profile(&mut self) {
        if self.cfg.profiles.is_empty() {
            return;
        }
        self.profile_index =
            (self.profile_index + self.cfg.profiles.len() - 1) % self.cfg.profiles.len();
    }

    fn refresh_active_profile(&mut self) {
        self.preference = BackendPreference::parse(self.cfg.backend());
        self.backend = client::detect_backend(self.preference);
        self.installed = self.backend != Backend::None;
        self.state = if self.installed {
            VpnState::Unknown
        } else {
            VpnState::NoBackend
        };
        self.ssl_only = None;
        self.ssl_only_inflight = false;
        self.error = None;
        self.details = Details::default();
        self.spawn_refresh();
    }

    fn block_profile_change_if_active(&mut self, action: &str) -> bool {
        if self.state.is_active() || self.busy || self.gpclient_child.is_some() {
            self.set_notice(format!("Disconnect before {action}"));
            self.push_log(format!("cannot {action} while connected/busy"));
            return true;
        }
        false
    }

    fn activate_selected_profile(&mut self) {
        let Some(name) = self.selected_profile_name() else {
            return;
        };
        if name == self.cfg.active {
            self.set_notice(format!("Profile '{name}' is already active"));
            return;
        }
        if self.block_profile_change_if_active("switch profiles") {
            return;
        }
        if self.cfg.select_profile(&name) {
            if let Err(error) = self.cfg.save() {
                self.push_log(format!("could not save config: {error}"));
            }
            self.refresh_active_profile();
            self.page = Page::Profiles;
            self.set_notice(format!("Active profile: {name}"));
            self.push_log(format!("active profile: {name}"));
        }
    }

    fn new_profile(&mut self) {
        if self.block_profile_change_if_active("create a profile") {
            return;
        }
        let name = self.cfg.unique_profile_name();
        if let Err(error) = self.cfg.add_profile(name.clone(), String::new()) {
            self.set_notice(error);
            return;
        }
        if let Err(error) = self.cfg.save() {
            self.push_log(format!("could not save config: {error}"));
        }
        self.profile_index = self.cfg.profiles.len().saturating_sub(1);
        self.page = Page::Config;
        self.config_index = 0;
        self.begin_edit(Field::Portal);
        self.refresh_active_profile();
        self.set_notice(format!("Profile '{name}' created"));
    }

    fn begin_rename_profile(&mut self) {
        let Some(name) = self.selected_profile_name() else {
            return;
        };
        self.input = name;
        self.mode = Mode::RenameProfile;
        self.page = Page::Profiles;
    }

    fn commit_profile_rename(&mut self) {
        let Some(old) = self.selected_profile_name() else {
            self.mode = Mode::Normal;
            return;
        };
        let new = self.input.trim().to_string();
        match self.cfg.rename_profile(&old, &new) {
            Ok(()) => {
                if let Err(error) = self.cfg.save() {
                    self.push_log(format!("could not save config: {error}"));
                }
                if let Some(index) = self
                    .cfg
                    .profiles
                    .iter()
                    .position(|profile| profile.name == new)
                {
                    self.profile_index = index;
                }
                self.set_notice("Profile renamed");
            }
            Err(error) => self.set_notice(error),
        }
        self.mode = Mode::Normal;
    }

    fn request_delete_profile(&mut self) {
        if self.cfg.profiles.len() <= 1 {
            self.set_notice("At least one profile is required");
            return;
        }
        let Some(name) = self.selected_profile_name() else {
            return;
        };
        if self.cfg.active == name
            && self.block_profile_change_if_active("delete the active profile")
        {
            return;
        }
        self.profile_delete = Some(name);
    }

    fn handle_profile_delete(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                let Some(name) = self.profile_delete.take() else {
                    return false;
                };
                if self.cfg.delete_profile(&name) {
                    let _ = self.cfg.save();
                    self.profile_index = self
                        .profile_index
                        .min(self.cfg.profiles.len().saturating_sub(1));
                    self.refresh_active_profile();
                    self.set_notice(format!("Profile '{name}' deleted"));
                }
            }
            KeyCode::Char('n') | KeyCode::Esc => self.profile_delete = None,
            _ => {}
        }
        false
    }

    fn edit_selected_profile(&mut self) {
        let selected = self.selected_profile_name();
        if selected
            .as_deref()
            .is_some_and(|name| name != self.cfg.active)
        {
            self.activate_selected_profile();
            if selected.as_deref() != Some(self.cfg.active.as_str()) {
                return;
            }
        }
        self.page = Page::Config;
        self.config_index = 0;
    }

    fn handle_msg(&mut self, message: Msg) {
        match message {
            Msg::Snapshot(snapshot) => {
                let previous = self.state;
                self.backend = snapshot.backend;
                self.installed = snapshot.installed;
                self.details = snapshot.details;
                self.gpa_active = snapshot.gpa_active;

                let snapshot_state = VpnState::from_snapshot(&snapshot.status);
                let next = reconcile_state(
                    self.state,
                    snapshot_state,
                    snapshot.backend == Backend::Gpclient && self.gpclient_child.is_some(),
                );

                self.state = next;
                if snapshot.error.is_some() {
                    self.error = snapshot.error;
                } else if next != VpnState::Error {
                    self.error = None;
                }
                self.poll_inflight = false;

                if previous != self.state {
                    self.push_log(format!("state: {}", self.state.label()));
                }
            }
            Msg::AuthPid(epoch, pid) => {
                if epoch == self.auth_epoch {
                    self.auth_pid = Some(pid);
                    self.auth_started = Some(Instant::now());
                    self.push_log(format!("gpauth started (pid {pid})"));
                }
            }
            Msg::GpclientAuth(epoch, result) => {
                if epoch != self.auth_epoch {
                    return;
                }
                self.auth_pid = None;
                self.auth_started = None;
                match result {
                    Ok(auth) => {
                        for line in auth.logs {
                            self.push_log(line);
                        }
                        self.push_log("SSO complete".to_string());
                        self.state = VpnState::WaitingSudo;
                        self.pending_cookie = Some(auth.cookie);
                    }
                    Err(error) => {
                        self.busy = false;
                        self.push_log(format!("gpauth failed: {error}"));
                        self.error = Some(error);
                        self.state = VpnState::Error;
                    }
                }
            }
            Msg::SudoAuth(epoch, result) => {
                if epoch != self.sudo_epoch {
                    return;
                }
                match result {
                    Ok(()) => self.sudo_authenticated(),
                    Err(error) => {
                        if let Some(prompt) = self.sudo_prompt.as_mut() {
                            prompt.inflight = false;
                            prompt.error = Some(error.clone());
                        }
                        self.push_log(format!("sudo authentication failed: {error}"));
                    }
                }
            }
            Msg::GpclientDisconnected(result) => {
                self.busy = false;
                match result {
                    Ok(()) => {
                        self.push_log("gpclient: disconnected".to_string());
                        self.state = VpnState::Disconnected;
                        self.set_notice("Disconnected");
                    }
                    Err(error) if client::sudo_needs_password(&error) => {
                        self.busy = true;
                        self.state = VpnState::WaitingSudo;
                        self.push_log(
                            "sudo credentials expired; enter the password again".to_string(),
                        );
                        self.open_sudo_prompt(SudoAction::Disconnect);
                    }
                    Err(error) => {
                        self.push_log(format!("gpclient disconnect failed: {error}"));
                        self.error = Some(error);
                        self.state = VpnState::Error;
                    }
                }
                self.spawn_refresh();
            }
            Msg::ConnectDone(result) => {
                self.busy = false;
                match result {
                    Ok(lines) => {
                        for line in lines {
                            self.push_log(line);
                        }
                        self.state = VpnState::Connected;
                        self.set_notice("Connected");
                    }
                    Err(error) => {
                        self.push_log(format!("connect failed: {error}"));
                        self.error = Some(error);
                        self.state = VpnState::Error;
                    }
                }
                self.spawn_refresh();
            }
            Msg::DisconnectDone(result) => {
                self.busy = false;
                match result {
                    Ok(lines) => {
                        for line in lines {
                            self.push_log(line);
                        }
                        self.state = VpnState::Disconnected;
                        self.set_notice("Disconnected");
                    }
                    Err(error) => {
                        self.push_log(format!("disconnect failed: {error}"));
                        self.error = Some(error);
                        self.state = VpnState::Error;
                    }
                }
                self.spawn_refresh();
            }
            Msg::GpaStarted(result) => {
                match result {
                    Ok(()) => {
                        self.push_log("gpa.service started".to_string());
                        self.set_notice("gpa.service started");
                    }
                    Err(error) => self.push_log(format!("could not start gpa: {error}")),
                }
                self.spawn_refresh();
            }
            Msg::LogCollected(result) => {
                self.busy = false;
                match result {
                    Ok(message) => {
                        self.push_log(message);
                        self.set_notice("Logs loaded");
                    }
                    Err(error) => self.push_log(format!("collect-log failed: {error}")),
                }
            }
            Msg::SslOnlyLoaded(result) => {
                self.ssl_only_inflight = false;
                match result {
                    Ok(value) => self.ssl_only = Some(value),
                    Err(error) => self.push_log(format!("SSL-only status unavailable: {error}")),
                }
            }
            Msg::SslOnlySet(result) => {
                self.ssl_only_inflight = false;
                match result {
                    Ok(value) => {
                        self.ssl_only = Some(value);
                        let label = if value { "on" } else { "off" };
                        self.push_log(format!("SSL-only: {label}"));
                        self.set_notice(format!("SSL-only {label}"));
                    }
                    Err(error) => {
                        self.push_log(format!("SSL-only change failed: {error}"));
                        self.set_notice("SSL-only change failed");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn does_not_overwrite_waiting_or_disconnecting() {
        assert_eq!(
            reconcile_state(VpnState::WaitingBrowser, VpnState::Disconnected, false),
            VpnState::WaitingBrowser
        );
        assert_eq!(
            reconcile_state(VpnState::WaitingSudo, VpnState::Disconnected, false),
            VpnState::WaitingSudo
        );
        assert_eq!(
            reconcile_state(VpnState::Disconnecting, VpnState::Connected, false),
            VpnState::Disconnecting
        );
        assert_eq!(
            reconcile_state(VpnState::Error, VpnState::Disconnected, false),
            VpnState::Error
        );
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
        app.state = VpnState::Error;
        app.error = Some("connect failed".to_string());

        app.handle_msg(Msg::Snapshot(Snapshot {
            backend: Backend::Official,
            installed: true,
            status: "Disconnected".to_string(),
            details: Details::default(),
            gpa_active: false,
            error: None,
        }));

        assert_eq!(app.state, VpnState::Error);
        assert_eq!(app.error.as_deref(), Some("connect failed"));
    }

    #[test]
    fn running_gpclient_wins_over_disconnected() {
        assert_eq!(
            reconcile_state(VpnState::Disconnected, VpnState::Disconnected, true),
            VpnState::Connecting
        );
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
        app.open_sudo_prompt(SudoAction::Connect);

        for character in "pä".chars() {
            app.handle_sudo_key(KeyEvent::from(KeyCode::Char(character)));
        }
        app.handle_sudo_key(KeyEvent::from(KeyCode::Backspace));

        let prompt = app.sudo_prompt.as_ref().unwrap();
        assert_eq!(prompt.password, b"p");
        assert!(prompt.error.is_none());
    }

    #[test]
    fn empty_sudo_password_is_rejected_inline() {
        let mut app = test_app();
        app.open_sudo_prompt(SudoAction::Connect);
        app.submit_sudo();

        let prompt = app.sudo_prompt.as_ref().unwrap();
        assert_eq!(prompt.error.as_deref(), Some("password cannot be empty"));
        assert!(!prompt.inflight);
    }

    #[test]
    fn cancelling_connect_prompt_drops_the_cookie_and_resets_state() {
        let mut app = test_app();
        app.state = VpnState::WaitingSudo;
        app.busy = true;
        app.begin_sudo_connect("cookie".to_string());

        app.handle_sudo_key(KeyEvent::from(KeyCode::Esc));

        assert!(app.sudo_prompt.is_none());
        assert!(app.pending_cookie.is_none());
        assert!(app.sudo_cookie.is_none());
        assert_eq!(app.state, VpnState::Disconnected);
        assert!(!app.busy);
    }

    #[test]
    fn reentering_sudo_connect_keeps_the_typed_password() {
        let mut app = test_app();
        app.begin_sudo_connect("first".to_string());
        app.handle_sudo_key(KeyEvent::from(KeyCode::Char('x')));

        // The run loop used to call this again on every tick.
        app.begin_sudo_connect("second".to_string());

        let prompt = app.sudo_prompt.as_ref().unwrap();
        assert_eq!(prompt.password, b"x");
        assert_eq!(app.sudo_cookie.as_deref(), Some("second"));
        assert!(app.pending_cookie.is_none());
    }

    #[test]
    fn expired_sudo_on_disconnect_opens_the_password_prompt() {
        let mut app = test_app();
        app.state = VpnState::Disconnecting;
        app.busy = true;

        app.handle_msg(Msg::GpclientDisconnected(Err(
            "sudo: a password is required".to_string(),
        )));

        assert_eq!(app.state, VpnState::WaitingSudo);
        assert!(app.busy);
        let prompt = app.sudo_prompt.as_ref().unwrap();
        assert_eq!(prompt.action, SudoAction::Disconnect);
    }

    #[test]
    fn cancelling_disconnect_prompt_keeps_the_tunnel_state() {
        let mut app = test_app();
        app.state = VpnState::WaitingSudo;
        app.busy = true;
        app.open_sudo_prompt(SudoAction::Disconnect);

        app.handle_sudo_key(KeyEvent::from(KeyCode::Esc));

        assert_eq!(app.state, VpnState::Connected);
        assert!(!app.busy);
    }
}
