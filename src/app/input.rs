use super::{systemctl_state, App, Backend, Field, Modal, Page, SudoAction, VpnState};
use crate::client;
use crate::keymap::{self, Action as KeyAction};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

impl App {
    pub(super) fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.sudo_prompt().is_some() {
            return self.handle_sudo_key(key);
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            return self.request_quit();
        }

        if matches!(&self.modal, Modal::Help) {
            if matches!(key.code, KeyCode::Char('?') | KeyCode::Esc) {
                self.modal = Modal::None;
            }
            return false;
        }

        if matches!(&self.modal, Modal::Quit) {
            return self.handle_quit_modal(key);
        }

        if matches!(&self.modal, Modal::DeleteProfile(_)) {
            return self.handle_profile_delete(key);
        }

        if matches!(&self.modal, Modal::Search) {
            match key.code {
                KeyCode::Enter => {
                    let query = self.input.trim().to_string();
                    self.log_search = if query.is_empty() { None } else { Some(query) };
                    self.log_follow = true;
                    self.log_from_bottom = 0;
                    self.modal = Modal::None;
                }
                KeyCode::Esc => {
                    self.modal = Modal::None;
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Char(character) => self.input.push(character),
                _ => {}
            }
            return false;
        }

        if matches!(&self.modal, Modal::RenameProfile) {
            match key.code {
                KeyCode::Enter => self.commit_profile_rename(),
                KeyCode::Esc => {
                    self.modal = Modal::None;
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Char(character) => self.input.push(character),
                _ => {}
            }
            return false;
        }

        if matches!(&self.modal, Modal::Edit(_)) {
            match key.code {
                KeyCode::Enter => self.commit_edit(),
                KeyCode::Esc => {
                    self.modal = Modal::None;
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                KeyCode::Char(character) => self.input.push(character),
                _ => {}
            }
            return false;
        }

        let Some(action) = keymap::action_for(key, self.page) else {
            return false;
        };

        match action {
            KeyAction::Help => self.modal = Modal::Help,
            KeyAction::NextPage => self.page = self.page.next_visible(),
            KeyAction::PreviousPage => self.page = self.page.previous_visible(),
            KeyAction::StatusPage => self.page = Page::Status,
            KeyAction::ProfilesPage => self.page = Page::Profiles,
            KeyAction::LogsPage => self.page = Page::Logs,
            KeyAction::Quit => return self.request_quit(),
            KeyAction::Escape => self.handle_escape(),
            KeyAction::NewProfile => self.new_profile(),
            KeyAction::RenameProfile => self.begin_rename_profile(),
            KeyAction::EditProfile => self.edit_selected_profile(),
            KeyAction::DeleteProfile => self.request_delete_profile(),
            KeyAction::ActivateProfile => self.activate_selected_profile(),
            KeyAction::Connect => self.connect_active_backend(),
            KeyAction::Disconnect => self.disconnect_active_backend(),
            KeyAction::Refresh => self.spawn_refresh(),
            KeyAction::Diagnostics => self.write_diagnostics(),
            KeyAction::CycleBackend => self.cycle_backend(),
            KeyAction::StartGpa => self.spawn_start_gpa(),
            KeyAction::SearchLogs => {
                self.page = Page::Logs;
                self.modal = Modal::Search;
                self.input = self.log_search.clone().unwrap_or_default();
            }
            KeyAction::CollectLogs => {
                self.page = Page::Logs;
                self.spawn_collect_log();
            }
            KeyAction::EditPortal => {
                self.page = Page::Config;
                self.config_index = 0;
                self.begin_edit(Field::Portal);
            }
            KeyAction::EditUsername => {
                self.page = Page::Config;
                self.config_index = 1;
                self.begin_edit(Field::Username);
            }
            KeyAction::MoveDown => self.move_down(),
            KeyAction::MoveUp => self.move_up(),
            KeyAction::LogsTop => {
                self.log_follow = false;
                self.log_from_bottom = self.logs.len();
            }
            KeyAction::LogsBottom => {
                self.log_follow = true;
                self.log_from_bottom = 0;
            }
            KeyAction::ToggleLogFollow => {
                self.log_follow = !self.log_follow;
                if self.log_follow {
                    self.log_from_bottom = 0;
                }
            }
            KeyAction::ToggleLogWrap => {
                self.log_wrap = !self.log_wrap;
                self.set_notice(if self.log_wrap {
                    "Log wrap on"
                } else {
                    "Log wrap off"
                });
            }
            KeyAction::LogPageUp => self.scroll_logs(10),
            KeyAction::LogPageDown => self.scroll_logs(-10),
            KeyAction::ConfigActivate => match self.config_index {
                0 => self.begin_edit(Field::Portal),
                1 => self.begin_edit(Field::Username),
                2 => self.cycle_backend(),
                _ => self.toggle_ssl_only(),
            },
            KeyAction::LogHalfPageDown => self.scroll_logs(-10),
            KeyAction::LogHalfPageUp => self.scroll_logs(10),
        }
        false
    }

    pub(super) fn handle_escape(&mut self) {
        if self.state() == VpnState::WaitingBrowser {
            self.cancel_auth(false);
        } else if self.page == Page::Logs && self.log_search.is_some() {
            self.log_search = None;
            self.log_follow = true;
            self.log_from_bottom = 0;
        } else if self.page == Page::Config {
            self.page = Page::Profiles;
        } else if self.page == Page::Status {
            self.should_quit = self.request_quit();
        } else {
            self.page = Page::Status;
        }
    }

    pub(super) fn connect_active_backend(&mut self) {
        match self.backend {
            Backend::Gpclient => self.gpclient_connect(),
            Backend::Official => self.spawn_connect(),
            Backend::None => self.push_log(
                "no backend installed: install globalprotect or globalprotect-openconnect",
            ),
        }
    }

    pub(super) fn disconnect_active_backend(&mut self) {
        match self.backend {
            Backend::Gpclient => self.gpclient_disconnect(),
            Backend::Official => self.spawn_disconnect(),
            Backend::None => self.push_log(
                "no backend installed: install globalprotect or globalprotect-openconnect",
            ),
        }
    }

    pub(super) fn move_down(&mut self) {
        match self.page {
            Page::Profiles => self.next_profile(),
            Page::Config => self.config_index = (self.config_index + 1) % 4,
            Page::Logs => self.scroll_logs(-1),
            Page::Status => {}
        }
    }

    pub(super) fn move_up(&mut self) {
        match self.page {
            Page::Profiles => self.previous_profile(),
            Page::Config => self.config_index = (self.config_index + 3) % 4,
            Page::Logs => self.scroll_logs(1),
            Page::Status => {}
        }
    }

    pub(super) fn scroll_logs(&mut self, delta: isize) {
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
    pub(super) fn shutdown_from_signal(&mut self) {
        let connection_was_active = self.connection.is_active();
        let auth_running = self
            .connection
            .auth_control()
            .is_some_and(|control| control.pid() > 0);
        if auth_running || self.state() == VpnState::WaitingBrowser {
            self.cancel_auth(false);
        }

        match self.backend {
            Backend::Gpclient if self.connection.has_child() => {
                let _ = client::gpclient_disconnect();
                self.connection.disconnected();
            }
            Backend::Official if connection_was_active => {
                let _ = client::disconnect();
            }
            _ => {}
        }
    }

    pub(super) fn request_quit(&mut self) -> bool {
        if self.connection.is_active() || self.connection.has_child() || self.log_collect_inflight {
            self.modal = Modal::Quit;
            return false;
        }
        true
    }

    pub(super) fn handle_quit_modal(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('d') => {
                self.modal = Modal::None;
                self.quit_and_disconnect();
                self.should_quit
            }
            KeyCode::Char('q') => {
                self.modal = Modal::None;
                true
            }
            KeyCode::Esc => {
                self.modal = Modal::None;
                false
            }
            _ => false,
        }
    }

    pub(super) fn quit_and_disconnect(&mut self) {
        match self.backend {
            Backend::Gpclient => {
                if self.connection.has_child() || self.connection.is_active() {
                    match client::gpclient_disconnect() {
                        Ok(()) => {
                            self.connection.disconnected();
                            self.should_quit = true;
                        }
                        Err(error) if client::sudo_needs_password(&error) => {
                            self.error = None;
                            self.connection.begin_waiting_sudo(SudoAction::Quit, None);
                            self.push_log(
                                "sudo needs a password; enter it in the prompt".to_string(),
                            );
                            self.open_sudo_prompt();
                        }
                        Err(error) => {
                            self.push_log(format!("disconnect before quit failed: {error}"));
                            self.connection.failed();
                            self.should_quit = true;
                        }
                    }
                } else {
                    self.should_quit = true;
                }
            }
            Backend::Official => {
                if self.connection.is_active() {
                    if let Err(error) = client::disconnect() {
                        self.push_log(format!("disconnect before quit failed: {error}"));
                    }
                }
                self.should_quit = true;
            }
            Backend::None => self.should_quit = true,
        }
    }

    pub(super) fn set_notice(&mut self, message: impl Into<String>) {
        self.notice = Some((message.into(), Instant::now() + Duration::from_secs(3)));
    }

    pub(super) fn write_diagnostics(&mut self) {
        let mut text = format!(
            "gp-tui {}\nstate: {}\nbackend: {}\npreference: {}\nportal: {}\nuser: {}\noverall: {}\n",
            env!("CARGO_PKG_VERSION"),
            self.state().label(),
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

    pub(super) fn clear_notice(&mut self) {
        if let Some((_, until)) = &self.notice {
            if Instant::now() >= *until {
                self.notice = None;
            }
        }
    }

    pub(super) fn cycle_backend(&mut self) {
        if self.log_collect_inflight
            || self.connection.state().is_busy()
            || self.connection.has_child()
        {
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
        self.connection.reset(if self.installed {
            VpnState::Unknown
        } else {
            VpnState::NoBackend
        });
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
        // Ignore a poll that started with the old preference and allow the
        // replacement poll to run immediately.
        self.poll_epoch = self.poll_epoch.wrapping_add(1);
        self.poll_inflight = false;
        self.spawn_refresh();
    }
}
