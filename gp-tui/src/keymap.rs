use crate::model::Page;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Help,
    NextPage,
    PreviousPage,
    StatusPage,
    ProfilesPage,
    LogsPage,
    Quit,
    Escape,
    NewProfile,
    RenameProfile,
    EditProfile,
    DeleteProfile,
    ActivateProfile,
    Connect,
    Disconnect,
    Refresh,
    Diagnostics,
    CycleBackend,
    StartGpa,
    SearchLogs,
    CollectLogs,
    EditPortal,
    EditUsername,
    MoveDown,
    MoveUp,
    LogsTop,
    LogsBottom,
    ToggleLogFollow,
    ToggleLogWrap,
    LogPageUp,
    LogPageDown,
    ConfigActivate,
    LogHalfPageDown,
    LogHalfPageUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpGroup {
    Pages,
    Status,
    Profiles,
    Settings,
    Logs,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterScope {
    None,
    All,
    Page(Page),
    Pages(&'static [Page]),
}

#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub code: KeyCode,
    pub control: bool,
    pub page: Option<Page>,
    pub action: Action,
    pub key_label: &'static str,
    pub help_group: Option<HelpGroup>,
    pub help: Option<&'static str>,
    pub footer_scope: FooterScope,
    pub footer: Option<&'static str>,
}

use Action as A;
use FooterScope as F;
use HelpGroup as H;
use Page as P;

pub const BINDINGS: &[Binding] = &[
    Binding {
        code: KeyCode::Char('?'),
        control: false,
        page: None,
        action: A::Help,
        key_label: "?",
        help_group: Some(H::Global),
        help: Some("help"),
        footer_scope: F::All,
        footer: Some("? all keys"),
    },
    Binding {
        code: KeyCode::Tab,
        control: false,
        page: None,
        action: A::NextPage,
        key_label: "Tab",
        help_group: Some(H::Pages),
        help: Some("next page"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::BackTab,
        control: false,
        page: None,
        action: A::PreviousPage,
        key_label: "Shift+Tab",
        help_group: Some(H::Pages),
        help: Some("previous page"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Char('1'),
        control: false,
        page: None,
        action: A::StatusPage,
        key_label: "1",
        help_group: Some(H::Pages),
        help: Some("Status"),
        footer_scope: F::All,
        footer: Some("1/2/3 pages"),
    },
    Binding {
        code: KeyCode::Char('2'),
        control: false,
        page: None,
        action: A::ProfilesPage,
        key_label: "2",
        help_group: Some(H::Pages),
        help: Some("Profiles"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Char('3'),
        control: false,
        page: None,
        action: A::LogsPage,
        key_label: "3",
        help_group: Some(H::Pages),
        help: Some("Logs"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Char('h'),
        control: false,
        page: None,
        action: A::PreviousPage,
        key_label: "h",
        help_group: Some(H::Pages),
        help: Some("previous page"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Char('l'),
        control: false,
        page: None,
        action: A::NextPage,
        key_label: "l",
        help_group: Some(H::Pages),
        help: Some("next page"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Char('q'),
        control: false,
        page: None,
        action: A::Quit,
        key_label: "q",
        help_group: Some(H::Global),
        help: Some("quit"),
        footer_scope: F::All,
        footer: Some("q quit"),
    },
    Binding {
        code: KeyCode::Esc,
        control: false,
        page: None,
        action: A::Escape,
        key_label: "Esc",
        help_group: Some(H::Global),
        help: Some("back, cancel, or quit"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Char('n'),
        control: false,
        page: Some(P::Profiles),
        action: A::NewProfile,
        key_label: "n",
        help_group: Some(H::Profiles),
        help: Some("new profile"),
        footer_scope: F::Page(P::Profiles),
        footer: Some("n/R/D profiles"),
    },
    Binding {
        code: KeyCode::Char('R'),
        control: false,
        page: Some(P::Profiles),
        action: A::RenameProfile,
        key_label: "R",
        help_group: Some(H::Profiles),
        help: Some("rename profile"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Char('e'),
        control: false,
        page: Some(P::Profiles),
        action: A::EditProfile,
        key_label: "e",
        help_group: Some(H::Profiles),
        help: Some("settings"),
        footer_scope: F::Page(P::Profiles),
        footer: Some("e settings"),
    },
    Binding {
        code: KeyCode::Char('D'),
        control: false,
        page: Some(P::Profiles),
        action: A::DeleteProfile,
        key_label: "D",
        help_group: Some(H::Profiles),
        help: Some("delete profile"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Enter,
        control: false,
        page: Some(P::Profiles),
        action: A::ActivateProfile,
        key_label: "Enter",
        help_group: Some(H::Profiles),
        help: Some("activate"),
        footer_scope: F::Page(P::Profiles),
        footer: Some("Enter activate"),
    },
    Binding {
        code: KeyCode::Char('c'),
        control: false,
        page: None,
        action: A::Connect,
        key_label: "c",
        help_group: Some(H::Status),
        help: Some("connect"),
        footer_scope: F::Page(P::Status),
        footer: Some("c connect"),
    },
    Binding {
        code: KeyCode::Char('d'),
        control: false,
        page: None,
        action: A::Disconnect,
        key_label: "d",
        help_group: Some(H::Status),
        help: Some("disconnect"),
        footer_scope: F::Page(P::Status),
        footer: Some("d disconnect"),
    },
    Binding {
        code: KeyCode::Char('r'),
        control: false,
        page: None,
        action: A::Refresh,
        key_label: "r",
        help_group: Some(H::Status),
        help: Some("refresh"),
        footer_scope: F::Page(P::Status),
        footer: Some("r refresh"),
    },
    Binding {
        code: KeyCode::Char('y'),
        control: false,
        page: None,
        action: A::Diagnostics,
        key_label: "y",
        help_group: Some(H::Global),
        help: Some("diagnostics"),
        footer_scope: F::Page(P::Status),
        footer: Some("y diagnostics"),
    },
    Binding {
        code: KeyCode::Char('b'),
        control: false,
        page: None,
        action: A::CycleBackend,
        key_label: "b",
        help_group: Some(H::Settings),
        help: Some("backend"),
        footer_scope: F::Page(P::Config),
        footer: Some("b backend"),
    },
    Binding {
        code: KeyCode::Char('g'),
        control: false,
        page: None,
        action: A::StartGpa,
        key_label: "g",
        help_group: Some(H::Status),
        help: Some("start gpa (official)"),
        footer_scope: F::Page(P::Status),
        footer: Some("g gpa"),
    },
    Binding {
        code: KeyCode::Char('/'),
        control: false,
        page: None,
        action: A::SearchLogs,
        key_label: "/",
        help_group: Some(H::Logs),
        help: Some("search"),
        footer_scope: F::Page(P::Logs),
        footer: Some("/ search"),
    },
    Binding {
        code: KeyCode::Char('L'),
        control: false,
        page: None,
        action: A::CollectLogs,
        key_label: "L",
        help_group: Some(H::Logs),
        help: Some("load"),
        footer_scope: F::Page(P::Logs),
        footer: Some("L load"),
    },
    Binding {
        code: KeyCode::Char('p'),
        control: false,
        page: None,
        action: A::EditPortal,
        key_label: "p",
        help_group: Some(H::Settings),
        help: Some("portal"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Char('u'),
        control: false,
        page: None,
        action: A::EditUsername,
        key_label: "u",
        help_group: Some(H::Settings),
        help: Some("user"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Char('j'),
        control: false,
        page: None,
        action: A::MoveDown,
        key_label: "j",
        help_group: Some(H::Pages),
        help: Some("move or scroll"),
        footer_scope: F::Pages(&[P::Profiles, P::Config, P::Logs]),
        footer: Some("j/k move/scroll"),
    },
    Binding {
        code: KeyCode::Char('k'),
        control: false,
        page: None,
        action: A::MoveUp,
        key_label: "k",
        help_group: Some(H::Pages),
        help: Some("move or scroll"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Up,
        control: false,
        page: None,
        action: A::MoveUp,
        key_label: "Up",
        help_group: Some(H::Logs),
        help: Some("scroll up"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Down,
        control: false,
        page: None,
        action: A::MoveDown,
        key_label: "Down",
        help_group: Some(H::Logs),
        help: Some("scroll down"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Char('G'),
        control: false,
        page: Some(P::Logs),
        action: A::LogsBottom,
        key_label: "G",
        help_group: Some(H::Logs),
        help: Some("end"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Home,
        control: false,
        page: Some(P::Logs),
        action: A::LogsTop,
        key_label: "Home",
        help_group: Some(H::Logs),
        help: Some("start"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::End,
        control: false,
        page: Some(P::Logs),
        action: A::LogsBottom,
        key_label: "End",
        help_group: Some(H::Logs),
        help: Some("end"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::PageUp,
        control: false,
        page: Some(P::Logs),
        action: A::LogPageUp,
        key_label: "PgUp",
        help_group: Some(H::Logs),
        help: Some("page up"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::PageDown,
        control: false,
        page: Some(P::Logs),
        action: A::LogPageDown,
        key_label: "PgDn",
        help_group: Some(H::Logs),
        help: Some("page down"),
        footer_scope: F::None,
        footer: None,
    },
    Binding {
        code: KeyCode::Char('f'),
        control: false,
        page: Some(P::Logs),
        action: A::ToggleLogFollow,
        key_label: "f",
        help_group: Some(H::Logs),
        help: Some("follow"),
        footer_scope: F::Page(P::Logs),
        footer: Some("f follow"),
    },
    Binding {
        code: KeyCode::Char('w'),
        control: false,
        page: Some(P::Logs),
        action: A::ToggleLogWrap,
        key_label: "w",
        help_group: Some(H::Logs),
        help: Some("wrap"),
        footer_scope: F::Page(P::Logs),
        footer: Some("w wrap"),
    },
    Binding {
        code: KeyCode::Enter,
        control: false,
        page: Some(P::Config),
        action: A::ConfigActivate,
        key_label: "Enter",
        help_group: Some(H::Settings),
        help: Some("edit/cycle"),
        footer_scope: F::Page(P::Config),
        footer: Some("Enter edit/cycle"),
    },
    Binding {
        code: KeyCode::Char('d'),
        control: true,
        page: Some(P::Logs),
        action: A::LogHalfPageDown,
        key_label: "Ctrl+d",
        help_group: Some(H::Logs),
        help: Some("half-page scroll"),
        footer_scope: F::Page(P::Logs),
        footer: Some("Ctrl+d/u"),
    },
    Binding {
        code: KeyCode::Char('u'),
        control: true,
        page: Some(P::Logs),
        action: A::LogHalfPageUp,
        key_label: "Ctrl+u",
        help_group: Some(H::Logs),
        help: Some("half-page scroll"),
        footer_scope: F::None,
        footer: None,
    },
];

pub fn action_for(key: KeyEvent, page: Page) -> Option<Action> {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    BINDINGS
        .iter()
        .find(|binding| {
            binding.code == key.code
                && binding.control == control
                && binding.page.is_none_or(|scope| scope == page)
        })
        .map(|binding| binding.action)
}

pub fn footer_for(page: Page) -> Vec<&'static str> {
    BINDINGS
        .iter()
        .filter(|binding| {
            binding.footer.is_some()
                && match binding.footer_scope {
                    FooterScope::None => false,
                    FooterScope::All => true,
                    FooterScope::Page(scope) => scope == page,
                    FooterScope::Pages(scopes) => scopes.contains(&page),
                }
        })
        .filter_map(|binding| binding.footer)
        .collect()
}

pub fn help_groups() -> [(HelpGroup, &'static str); 6] {
    [
        (HelpGroup::Pages, "Pages"),
        (HelpGroup::Status, "Status"),
        (HelpGroup::Profiles, "Profiles"),
        (HelpGroup::Settings, "Settings"),
        (HelpGroup::Logs, "Logs"),
        (HelpGroup::Global, "Global"),
    ]
}

pub fn help_items(group: HelpGroup) -> Vec<(Action, String, &'static str)> {
    let mut items: Vec<(Action, String, &'static str)> = Vec::new();
    for binding in BINDINGS
        .iter()
        .filter(|binding| binding.help_group == Some(group))
    {
        let Some(help) = binding.help else {
            continue;
        };
        if let Some((_, keys, _)) = items.iter_mut().find(|(_, _, label)| *label == help) {
            keys.push('/');
            keys.push_str(binding.key_label);
        } else {
            items.push((binding.action, binding.key_label.to_string(), help));
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyModifiers};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new_with_kind(code, modifiers, KeyEventKind::Press)
    }

    #[test]
    fn page_scoped_bindings_do_not_leak_to_other_pages() {
        assert_eq!(
            action_for(key(KeyCode::Char('n'), KeyModifiers::NONE), Page::Profiles),
            Some(Action::NewProfile)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('n'), KeyModifiers::NONE), Page::Logs),
            None
        );
    }

    #[test]
    fn control_bindings_are_distinct_from_plain_keys() {
        assert_eq!(
            action_for(key(KeyCode::Char('d'), KeyModifiers::CONTROL), Page::Logs),
            Some(Action::LogHalfPageDown)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('d'), KeyModifiers::NONE), Page::Logs),
            Some(Action::Disconnect)
        );
    }

    #[test]
    fn help_and_footer_entries_come_from_bindings() {
        assert!(help_items(HelpGroup::Logs)
            .iter()
            .any(|(_, key, help)| key == "L" && *help == "load"));
        assert!(footer_for(Page::Logs).contains(&"L load"));
    }
}
