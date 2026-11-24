use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn default_backend() -> String {
    "auto".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub portal: String,
    #[serde(default)]
    pub username: String,
    #[serde(default = "default_backend")]
    pub backend: String,
}

impl Profile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            portal: String::new(),
            username: String::new(),
            backend: default_backend(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub active: String,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default, rename = "portal", skip_serializing_if = "String::is_empty")]
    legacy_portal: String,
    #[serde(default, rename = "username", skip_serializing_if = "String::is_empty")]
    legacy_username: String,
    #[serde(default, rename = "backend", skip_serializing_if = "String::is_empty")]
    legacy_backend: String,
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("gp-tui").join("config.toml"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let mut config = match std::fs::read_to_string(&path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        };
        config.migrate();
        config
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path().context("cannot determine config directory")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let mut config = self.clone();
        config.migrate();
        let contents = toml::to_string_pretty(&config)?;
        std::fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    fn migrate(&mut self) {
        if self.profiles.is_empty()
            && (!self.legacy_portal.is_empty()
                || !self.legacy_username.is_empty()
                || !self.legacy_backend.is_empty())
        {
            let backend = if self.legacy_backend.is_empty() {
                default_backend()
            } else {
                self.legacy_backend.clone()
            };
            self.profiles.push(Profile {
                name: "default".to_string(),
                portal: self.legacy_portal.clone(),
                username: self.legacy_username.clone(),
                backend,
            });
            self.active = "default".to_string();
        }

        if !self.profiles.is_empty()
            && !self
                .profiles
                .iter()
                .any(|profile| profile.name == self.active)
        {
            self.active = self.profiles[0].name.clone();
        }
        if self.profiles.is_empty() {
            self.active.clear();
        }

        self.legacy_portal.clear();
        self.legacy_username.clear();
        self.legacy_backend.clear();
    }

    pub fn active_profile(&self) -> Option<&Profile> {
        self.profiles
            .iter()
            .find(|profile| profile.name == self.active)
    }

    pub fn ensure_profile(&mut self) -> &mut Profile {
        if self.active_profile().is_none() {
            if self.profiles.is_empty() {
                self.profiles.push(Profile::new("default"));
                self.active = "default".to_string();
            } else {
                self.active = self.profiles[0].name.clone();
            }
        }
        let active = self.active.clone();
        self.profiles
            .iter_mut()
            .find(|profile| profile.name == active)
            .expect("active profile must exist")
    }

    pub fn portal(&self) -> &str {
        self.active_profile()
            .map(|profile| profile.portal.as_str())
            .unwrap_or("")
    }

    pub fn username(&self) -> &str {
        self.active_profile()
            .map(|profile| profile.username.as_str())
            .unwrap_or("")
    }

    pub fn backend(&self) -> &str {
        self.active_profile()
            .map(|profile| profile.backend.as_str())
            .unwrap_or("auto")
    }

    pub fn set_portal(&mut self, portal: String) {
        self.ensure_profile().portal = portal;
    }

    pub fn set_username(&mut self, username: String) {
        self.ensure_profile().username = username;
    }

    pub fn set_backend(&mut self, backend: String) {
        self.ensure_profile().backend = backend;
    }

    pub fn select_profile(&mut self, name: &str) -> bool {
        if self.profiles.iter().any(|profile| profile.name == name) {
            self.active = name.to_string();
            true
        } else {
            false
        }
    }

    pub fn unique_profile_name(&self) -> String {
        let mut index = 1;
        loop {
            let name = format!("profile-{index}");
            if !self.profiles.iter().any(|profile| profile.name == name) {
                return name;
            }
            index += 1;
        }
    }

    pub fn add_profile(&mut self, name: String, portal: String) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("profile name cannot be empty".to_string());
        }
        if self.profiles.iter().any(|profile| profile.name == name) {
            return Err(format!("profile '{name}' already exists"));
        }
        self.profiles.push(Profile {
            name: name.clone(),
            portal,
            username: String::new(),
            backend: default_backend(),
        });
        self.active = name;
        Ok(())
    }

    pub fn rename_profile(&mut self, old: &str, new: &str) -> Result<(), String> {
        let new = new.trim();
        if new.is_empty() {
            return Err("profile name cannot be empty".to_string());
        }
        if new != old && self.profiles.iter().any(|profile| profile.name == new) {
            return Err(format!("profile '{new}' already exists"));
        }
        let Some(profile) = self.profiles.iter_mut().find(|profile| profile.name == old) else {
            return Err(format!("profile '{old}' not found"));
        };
        profile.name = new.to_string();
        if self.active == old {
            self.active = new.to_string();
        }
        Ok(())
    }

    pub fn delete_profile(&mut self, name: &str) -> bool {
        let before = self.profiles.len();
        self.profiles.retain(|profile| profile.name != name);
        if self.profiles.len() == before {
            return false;
        }
        if self.active == name {
            self.active = self
                .profiles
                .first()
                .map(|profile| profile.name.clone())
                .unwrap_or_default();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_migrates_to_default_profile() {
        let mut config: Config =
            toml::from_str("portal = \"vpn.example.com\"\nusername = \"user\"\n").unwrap();
        config.migrate();
        assert_eq!(config.active, "default");
        assert_eq!(config.portal(), "vpn.example.com");
        assert_eq!(config.username(), "user");
        assert_eq!(config.backend(), "auto");
    }

    #[test]
    fn default_config_has_no_profile() {
        let config = Config::default();
        assert!(config.active_profile().is_none());
        assert!(config.portal().is_empty());
    }

    #[test]
    fn add_select_rename_delete_profile() {
        let mut config = Config::default();
        config
            .add_profile("iscte".to_string(), "vpn.iscte-iul.pt".to_string())
            .unwrap();
        config
            .add_profile("work".to_string(), "vpn.example.com".to_string())
            .unwrap();
        assert_eq!(config.active, "work");
        assert!(config.select_profile("iscte"));
        assert_eq!(config.portal(), "vpn.iscte-iul.pt");
        config.rename_profile("iscte", "school").unwrap();
        assert_eq!(config.active, "school");
        assert!(config.delete_profile("school"));
        assert_eq!(config.active, "work");
    }

    #[test]
    fn rejects_duplicate_profile_names() {
        let mut config = Config::default();
        config
            .add_profile("iscte".to_string(), "a".to_string())
            .unwrap();
        assert!(config
            .add_profile("iscte".to_string(), "b".to_string())
            .is_err());
    }
}
