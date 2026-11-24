use super::{App, Backend, Modal, Msg, Page, SudoAction, SudoPrompt};
use crate::client;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::process::Command;
use std::thread;
use std::time::Duration;

impl App {
    pub(super) fn spawn_connect(&mut self) {
        if self.backend != Backend::Official {
            self.push_log("official globalprotect CLI not found".to_string());
            return;
        }
        if self.log_collect_inflight || self.connection.state().is_busy() {
            self.push_log("already connecting or disconnecting".to_string());
            return;
        }
        if !self.installed {
            self.push_log(
                "globalprotect not found in PATH; install it with installers/arch/gp_install_arch.sh first"
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

        self.error = None;
        let epoch = self.connection.begin_official_connect();
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
            let _ = tx.send(Msg::ConnectDone(epoch, result));
        });
    }

    pub(super) fn spawn_disconnect(&mut self) {
        if self.backend != Backend::Official {
            self.push_log("official globalprotect CLI not found".to_string());
            return;
        }
        if self.log_collect_inflight || self.connection.state().is_busy() {
            self.push_log("already connecting or disconnecting".to_string());
            return;
        }
        self.error = None;
        let epoch = self.connection.begin_disconnect();
        self.push_log("> globalprotect disconnect".to_string());

        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::disconnect();
            let _ = tx.send(Msg::DisconnectDone(epoch, result));
        });
    }

    pub(super) fn cancel_auth(&mut self, timeout: bool) {
        if let Some(pid) = self.connection.cancel_auth() {
            let _ = Command::new("kill")
                .args(["-INT", &pid.to_string()])
                .status();
        }
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

    pub(super) fn check_auth_timeout(&mut self) {
        if self
            .connection
            .auth_started()
            .is_some_and(|started| started.elapsed() > Duration::from_secs(180))
        {
            self.cancel_auth(true);
        }
    }

    pub(super) fn gpclient_connect(&mut self) {
        if self.log_collect_inflight || self.connection.state().is_busy() {
            self.push_log("already connecting or disconnecting".to_string());
            return;
        }
        if self.connection.has_child() {
            self.push_log("gpclient is already running; press 'd' to disconnect".to_string());
            return;
        }
        if self.cfg.portal().trim().is_empty() {
            self.push_log("portal is empty; press 'p' to configure it".to_string());
            return;
        }

        self.error = None;
        let (epoch, auth_control) = self.connection.begin_auth();
        self.push_log(format!("> gpauth {} --browser", self.cfg.portal().trim()));
        self.push_log("browser will open; complete SSO/MFA. The TUI stays visible.".to_string());

        let tx = self.tx.clone();
        let portal = self.cfg.portal().to_string();
        thread::spawn(move || {
            let result = client::gpclient_auth(&portal, auth_control);
            let _ = tx.send(Msg::GpclientAuth(epoch, result));
        });
    }

    pub(super) fn finish_gpclient_connect(&mut self, cookie: String) {
        self.push_log("SSO done; starting gpclient".to_string());

        if client::sudo_connect_ready() {
            self.start_gpclient(cookie);
            return;
        }

        self.begin_sudo_connect(cookie);
    }

    pub(super) fn begin_sudo_connect(&mut self, cookie: String) {
        self.connection
            .begin_waiting_sudo(SudoAction::Connect, Some(cookie));
        if self.sudo_prompt().is_none() {
            self.push_log("sudo needs a password; enter it in the prompt".to_string());
            self.open_sudo_prompt();
        }
    }

    pub(super) fn start_gpclient(&mut self, cookie: String) {
        self.push_log(format!(
            "> sudo -n gpclient connect {}",
            self.cfg.portal().trim()
        ));
        let result = client::gpclient_start(&cookie, self.cfg.portal(), self.cfg.username());

        match result {
            Ok(child) => {
                self.connection.start_gpclient(child);
                self.gpclient_log = client::gpclient_log_path();
                self.push_log("gpclient started in the background".to_string());
                self.set_notice("gpclient started");
                if let Some(path) = &self.gpclient_log {
                    self.push_log(format!("gpclient log: {}", path.display()));
                }
            }
            Err(error) => {
                self.push_log(format!("gpclient connect failed: {error}"));
                self.error = Some(error);
                self.connection.failed();
            }
        }
        self.spawn_refresh();
    }

    pub(super) fn gpclient_disconnect(&mut self) {
        if self.log_collect_inflight || self.connection.state().is_busy() {
            self.push_log("already connecting or disconnecting".to_string());
            return;
        }

        // Try the cached timestamp first. If it expired, the worker reports
        // it and Msg::GpclientDisconnected opens the password prompt.
        self.begin_gpclient_disconnect();
    }

    pub(super) fn begin_gpclient_disconnect(&mut self) {
        self.error = None;
        let epoch = self.connection.begin_disconnect();
        self.push_log("> sudo -n gpclient disconnect".to_string());

        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::gpclient_disconnect();
            let _ = tx.send(Msg::GpclientDisconnected(epoch, result));
        });
    }

    pub(super) fn open_sudo_prompt(&mut self) {
        self.modal = Modal::Sudo(SudoPrompt::new());
    }

    pub(super) fn sudo_prompt(&self) -> Option<&SudoPrompt> {
        match &self.modal {
            Modal::Sudo(prompt) => Some(prompt),
            _ => None,
        }
    }

    pub(super) fn sudo_prompt_mut(&mut self) -> Option<&mut SudoPrompt> {
        match &mut self.modal {
            Modal::Sudo(prompt) => Some(prompt),
            _ => None,
        }
    }

    pub(super) fn take_sudo_prompt(&mut self) -> Option<SudoPrompt> {
        match std::mem::replace(&mut self.modal, Modal::None) {
            Modal::Sudo(prompt) => Some(prompt),
            modal => {
                self.modal = modal;
                None
            }
        }
    }

    pub(super) fn handle_sudo_key(&mut self, key: KeyEvent) -> bool {
        if self.sudo_prompt().is_none() {
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
                if let Some(prompt) = self.sudo_prompt_mut() {
                    prompt.pop_char();
                }
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(prompt) = self.sudo_prompt_mut() {
                    prompt.push_char(character);
                }
            }
            _ => {}
        }
        false
    }

    pub(super) fn submit_sudo(&mut self) {
        let Some(prompt) = self.sudo_prompt() else {
            return;
        };
        if prompt.inflight {
            return;
        }
        if prompt.password.is_empty() {
            if let Some(prompt) = self.sudo_prompt_mut() {
                prompt.error = Some("password cannot be empty".to_string());
            }
            return;
        }

        let epoch = self.connection.begin_sudo_auth();
        let Some(prompt) = self.sudo_prompt_mut() else {
            return;
        };
        prompt.inflight = true;
        prompt.error = None;
        let mut password = std::mem::take(&mut prompt.password);
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::sudo_authenticate(&password);
            password.fill(0);
            let _ = tx.send(Msg::SudoAuth(epoch, result));
        });
    }

    pub(super) fn cancel_sudo_prompt(&mut self) {
        if self.take_sudo_prompt().is_none() {
            return;
        }
        self.connection.cancel_sudo();
        self.push_log("sudo authentication cancelled".to_string());
        self.set_notice("Sudo cancelled");
    }

    pub(super) fn sudo_authenticated(&mut self) {
        if self.take_sudo_prompt().is_none() {
            return;
        }
        let Some(request) = self.connection.take_sudo_request() else {
            return;
        };
        self.push_log("sudo authenticated".to_string());

        match request.action {
            SudoAction::Connect => {
                if let Some(cookie) = request.cookie {
                    self.start_gpclient(cookie);
                } else {
                    self.connection.disconnected();
                    self.push_log("authentication cookie expired; reconnect".to_string());
                    self.set_notice("Authentication expired");
                }
            }
            SudoAction::Disconnect => self.begin_gpclient_disconnect(),
            SudoAction::Quit => {
                if self.connection.has_child() || self.connection.is_active() {
                    let result = client::gpclient_disconnect();
                    if let Err(error) = result {
                        self.push_log(format!("disconnect before quit failed: {error}"));
                    }
                }
                self.connection.disconnected();
                self.should_quit = true;
            }
        }
    }

    pub(super) fn spawn_start_gpa(&mut self) {
        if self.backend != Backend::Official {
            self.push_log("gpa is only used by the official backend".to_string());
            return;
        }
        self.push_log("> systemctl --user start gpa.service".to_string());
        let epoch = self.connection.epoch();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::start_gpa();
            let _ = tx.send(Msg::GpaStarted(epoch, result));
        });
    }

    pub(super) fn spawn_collect_log(&mut self) {
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
        if self.log_collect_inflight || self.connection.state().is_busy() {
            self.push_log("wait for the current operation to finish".to_string());
            return;
        }
        self.log_collect_inflight = true;
        self.push_log("> globalprotect collect-log".to_string());
        let epoch = self.connection.epoch();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::collect_log();
            let _ = tx.send(Msg::LogCollected(epoch, result));
        });
    }

    pub(super) fn maybe_load_ssl_only(&mut self) {
        if self.page != Page::Config
            || self.backend != Backend::Official
            || self.ssl_only.is_some()
            || self.ssl_only_inflight
        {
            return;
        }
        self.ssl_only_inflight = true;
        let epoch = self.connection.epoch();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::ssl_only_status();
            let _ = tx.send(Msg::SslOnlyLoaded(epoch, result));
        });
    }

    pub(super) fn toggle_ssl_only(&mut self) {
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
        let epoch = self.connection.epoch();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = client::toggle_ssl_only(current);
            let _ = tx.send(Msg::SslOnlySet(epoch, result));
        });
    }
}
