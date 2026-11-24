use super::{App, Backend, Msg, SudoAction, VpnState};
use crate::client;

impl App {
    pub(super) fn handle_msg(&mut self, message: Msg) {
        match message {
            Msg::Snapshot(epoch, connection_epoch, snapshot) => {
                if epoch != self.poll_epoch {
                    return;
                }
                if connection_epoch != self.connection.epoch() {
                    self.poll_inflight = false;
                    return;
                }
                let previous = self.state();
                self.backend = snapshot.backend;
                self.installed = snapshot.installed;
                self.details = snapshot.details;
                self.gpa_active = snapshot.gpa_active;
                self.connection.reconcile(
                    VpnState::from_snapshot(&snapshot.status),
                    snapshot.backend == Backend::Gpclient && self.connection.has_child(),
                );
                if snapshot.error.is_some() {
                    self.error = snapshot.error;
                } else if self.state() != VpnState::Error {
                    self.error = None;
                }
                self.poll_inflight = false;

                if previous != self.state() {
                    self.push_log(format!("state: {}", self.state().label()));
                }
            }
            Msg::GpclientAuth(epoch, result) => {
                if !self.connection.is_authenticating(epoch) {
                    return;
                }
                match result {
                    Ok(auth) => {
                        for line in auth.logs {
                            self.push_log(line);
                        }
                        self.push_log("SSO complete".to_string());
                        self.finish_gpclient_connect(auth.cookie);
                    }
                    Err(error) => {
                        self.connection.failed();
                        self.push_log(format!("gpauth failed: {error}"));
                        self.error = Some(error);
                    }
                }
            }
            Msg::SudoAuth(epoch, result) => {
                if !self.connection.is_current_sudo_auth(epoch) {
                    return;
                }
                match result {
                    Ok(()) => self.sudo_authenticated(),
                    Err(error) => {
                        if let Some(prompt) = self.sudo_prompt_mut() {
                            prompt.inflight = false;
                            prompt.error = Some(error.clone());
                        }
                        self.push_log(format!("sudo authentication failed: {error}"));
                    }
                }
            }
            Msg::GpclientDisconnected(epoch, result) => {
                if !self.connection.is_current_operation(epoch) {
                    return;
                }
                match result {
                    Ok(()) => {
                        self.connection.disconnected();
                        self.push_log("gpclient: disconnected".to_string());
                        self.set_notice("Disconnected");
                    }
                    Err(error) if client::sudo_needs_password(&error) => {
                        self.connection
                            .begin_waiting_sudo(SudoAction::Disconnect, None);
                        self.push_log(
                            "sudo credentials expired; enter the password again".to_string(),
                        );
                        self.open_sudo_prompt();
                    }
                    Err(error) => {
                        self.connection.failed();
                        self.push_log(format!("gpclient disconnect failed: {error}"));
                        self.error = Some(error);
                    }
                }
                self.spawn_refresh();
            }
            Msg::ConnectDone(epoch, result) => {
                if !self.connection.is_current_operation(epoch) {
                    return;
                }
                match result {
                    Ok(lines) => {
                        for line in lines {
                            self.push_log(line);
                        }
                        self.connection.connected();
                        self.set_notice("Connected");
                    }
                    Err(error) => {
                        self.connection.failed();
                        self.push_log(format!("connect failed: {error}"));
                        self.error = Some(error);
                    }
                }
                self.spawn_refresh();
            }
            Msg::DisconnectDone(epoch, result) => {
                if !self.connection.is_current_operation(epoch) {
                    return;
                }
                match result {
                    Ok(lines) => {
                        for line in lines {
                            self.push_log(line);
                        }
                        self.connection.disconnected();
                        self.set_notice("Disconnected");
                    }
                    Err(error) => {
                        self.connection.failed();
                        self.push_log(format!("disconnect failed: {error}"));
                        self.error = Some(error);
                    }
                }
                self.spawn_refresh();
            }
            Msg::GpaStarted(epoch, result) => {
                if !self.connection.is_current_operation(epoch) {
                    return;
                }
                match result {
                    Ok(()) => {
                        self.push_log("gpa.service started".to_string());
                        self.set_notice("gpa.service started");
                    }
                    Err(error) => self.push_log(format!("could not start gpa: {error}")),
                }
                self.spawn_refresh();
            }
            Msg::LogCollected(epoch, result) => {
                if !self.connection.is_current_operation(epoch) {
                    self.log_collect_inflight = false;
                    return;
                }
                self.log_collect_inflight = false;
                match result {
                    Ok(message) => {
                        self.push_log(message);
                        self.set_notice("Logs loaded");
                    }
                    Err(error) => self.push_log(format!("collect-log failed: {error}")),
                }
            }
            Msg::SslOnlyLoaded(epoch, result) => {
                if !self.connection.is_current_operation(epoch) {
                    return;
                }
                self.ssl_only_inflight = false;
                match result {
                    Ok(value) => self.ssl_only = Some(value),
                    Err(error) => self.push_log(format!("SSL-only status unavailable: {error}")),
                }
            }
            Msg::SslOnlySet(epoch, result) => {
                if !self.connection.is_current_operation(epoch) {
                    return;
                }
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
