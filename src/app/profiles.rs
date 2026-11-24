use super::{App, Backend, BackendPreference, Details, Field, Modal, Page, VpnState};
use crate::client;
use crossterm::event::{KeyCode, KeyEvent};

impl App {
    pub(super) fn selected_profile_name(&self) -> Option<String> {
        self.cfg
            .profiles
            .get(self.profile_index)
            .map(|profile| profile.name.clone())
    }

    pub(super) fn next_profile(&mut self) {
        if self.cfg.profiles.is_empty() {
            return;
        }
        self.profile_index = (self.profile_index + 1) % self.cfg.profiles.len();
    }

    pub(super) fn previous_profile(&mut self) {
        if self.cfg.profiles.is_empty() {
            return;
        }
        self.profile_index =
            (self.profile_index + self.cfg.profiles.len() - 1) % self.cfg.profiles.len();
    }

    pub(super) fn refresh_active_profile(&mut self) {
        self.preference = BackendPreference::parse(self.cfg.backend());
        self.backend = client::detect_backend(self.preference);
        self.installed = self.backend != Backend::None;
        self.connection.reset(if self.installed {
            VpnState::Unknown
        } else {
            VpnState::NoBackend
        });
        self.ssl_only = None;
        self.ssl_only_inflight = false;
        self.error = None;
        self.details = Details::default();
        self.poll_epoch = self.poll_epoch.wrapping_add(1);
        self.poll_inflight = false;
        self.spawn_refresh();
    }

    pub(super) fn block_profile_change_if_active(&mut self, action: &str) -> bool {
        if self.connection.is_active() || self.log_collect_inflight || self.connection.has_child() {
            self.set_notice(format!("Disconnect before {action}"));
            self.push_log(format!("cannot {action} while connected/busy"));
            return true;
        }
        false
    }

    pub(super) fn activate_selected_profile(&mut self) {
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

    pub(super) fn new_profile(&mut self) {
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

    pub(super) fn begin_rename_profile(&mut self) {
        let Some(name) = self.selected_profile_name() else {
            return;
        };
        self.input = name;
        self.modal = Modal::RenameProfile;
        self.page = Page::Profiles;
    }

    pub(super) fn commit_profile_rename(&mut self) {
        let Some(old) = self.selected_profile_name() else {
            self.modal = Modal::None;
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
        self.modal = Modal::None;
    }

    pub(super) fn request_delete_profile(&mut self) {
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
        self.modal = Modal::DeleteProfile(name);
    }

    pub(super) fn handle_profile_delete(&mut self, key: KeyEvent) -> bool {
        let name = match &self.modal {
            Modal::DeleteProfile(name) => name.clone(),
            _ => return false,
        };
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                self.modal = Modal::None;
                if self.cfg.delete_profile(&name) {
                    let _ = self.cfg.save();
                    self.profile_index = self
                        .profile_index
                        .min(self.cfg.profiles.len().saturating_sub(1));
                    self.refresh_active_profile();
                    self.set_notice(format!("Profile '{name}' deleted"));
                }
            }
            KeyCode::Char('n') | KeyCode::Esc => self.modal = Modal::None,
            _ => {}
        }
        false
    }

    pub(super) fn edit_selected_profile(&mut self) {
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
}
