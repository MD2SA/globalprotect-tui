use super::{normalize_portal, App, Field, Modal};

impl App {
    pub(super) fn begin_edit(&mut self, field: Field) {
        self.modal = Modal::Edit(field);
        self.input = match field {
            Field::Portal => self.cfg.portal().to_string(),
            Field::Username => self.cfg.username().to_string(),
        };
    }

    pub(super) fn commit_edit(&mut self) {
        let field = match &self.modal {
            Modal::Edit(field) => *field,
            _ => return,
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
        self.modal = Modal::None;
    }
}
