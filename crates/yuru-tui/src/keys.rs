use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::api::{BindingAction, KeyBinding};
use crate::state::TuiAction;

pub(crate) fn classify_key(
    key: KeyEvent,
    page_rows: usize,
    expect_keys: &[String],
    bindings: &[KeyBinding],
) -> KeyDecision {
    if let Some(name) = key_name(key) {
        if expect_keys.iter().any(|expected| expected == &name) {
            return KeyDecision::Accept(Some(name));
        }
        if let Some(binding) = bindings.iter().find(|binding| binding.key == name) {
            return match binding.action {
                BindingAction::Accept => KeyDecision::Accept(None),
                BindingAction::Abort => KeyDecision::Abort,
                BindingAction::ClearQuery => KeyDecision::Action(TuiAction::ClearQuery),
                BindingAction::MoveSelectionUp => KeyDecision::Action(TuiAction::MoveSelectionUp),
                BindingAction::MoveSelectionDown => {
                    KeyDecision::Action(TuiAction::MoveSelectionDown)
                }
                BindingAction::MoveSelectionFirst => {
                    KeyDecision::Action(TuiAction::MoveSelectionFirst)
                }
                BindingAction::MoveSelectionLast => {
                    KeyDecision::Action(TuiAction::MoveSelectionLast)
                }
                BindingAction::PageUp => KeyDecision::Action(TuiAction::PageUp(page_rows)),
                BindingAction::PageDown => KeyDecision::Action(TuiAction::PageDown(page_rows)),
                BindingAction::ToggleMark => KeyDecision::Action(TuiAction::ToggleMark),
                BindingAction::ToggleMarkAndDown => {
                    KeyDecision::Action(TuiAction::ToggleMarkAndDown)
                }
                BindingAction::ToggleMarkAndUp => KeyDecision::Action(TuiAction::ToggleMarkAndUp),
                BindingAction::MoveCursorStart => KeyDecision::Action(TuiAction::MoveCursorStart),
                BindingAction::MoveCursorEnd => KeyDecision::Action(TuiAction::MoveCursorEnd),
                BindingAction::MoveCursorLeft => KeyDecision::Action(TuiAction::MoveCursorLeft),
                BindingAction::MoveCursorRight => KeyDecision::Action(TuiAction::MoveCursorRight),
                BindingAction::Backspace => KeyDecision::Action(TuiAction::Backspace),
                BindingAction::Delete => KeyDecision::Action(TuiAction::Delete),
                BindingAction::PreviewUp => KeyDecision::Action(TuiAction::PreviewUp),
                BindingAction::PreviewDown => KeyDecision::Action(TuiAction::PreviewDown),
                BindingAction::PreviewPageUp => {
                    KeyDecision::Action(TuiAction::PreviewPageUp(page_rows))
                }
                BindingAction::PreviewPageDown => {
                    KeyDecision::Action(TuiAction::PreviewPageDown(page_rows))
                }
                BindingAction::PreviewTop => KeyDecision::Action(TuiAction::PreviewTop),
                BindingAction::PreviewBottom => KeyDecision::Action(TuiAction::PreviewBottom),
            };
        }
    }

    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => KeyDecision::Accept(None),
        (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => KeyDecision::Abort,
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => KeyDecision::Action(TuiAction::ClearQuery),
        (KeyCode::Char('a'), KeyModifiers::CONTROL) | (KeyCode::Home, _) => {
            KeyDecision::Action(TuiAction::MoveCursorStart)
        }
        (KeyCode::Char('e'), KeyModifiers::CONTROL) | (KeyCode::End, _) => {
            KeyDecision::Action(TuiAction::MoveCursorEnd)
        }
        (KeyCode::Up, modifiers) if modifiers.contains(KeyModifiers::SHIFT) => {
            KeyDecision::Action(TuiAction::PreviewUp)
        }
        (KeyCode::Down, modifiers) if modifiers.contains(KeyModifiers::SHIFT) => {
            KeyDecision::Action(TuiAction::PreviewDown)
        }
        (KeyCode::PageUp, modifiers) if modifiers.contains(KeyModifiers::SHIFT) => {
            KeyDecision::Action(TuiAction::PreviewPageUp(page_rows))
        }
        (KeyCode::PageDown, modifiers) if modifiers.contains(KeyModifiers::SHIFT) => {
            KeyDecision::Action(TuiAction::PreviewPageDown(page_rows))
        }
        (KeyCode::Char('n'), KeyModifiers::CONTROL) | (KeyCode::Down, _) => {
            KeyDecision::Action(TuiAction::MoveSelectionDown)
        }
        (KeyCode::Char('p'), KeyModifiers::CONTROL) | (KeyCode::Up, _) => {
            KeyDecision::Action(TuiAction::MoveSelectionUp)
        }
        (KeyCode::PageUp, _) => KeyDecision::Action(TuiAction::PageUp(page_rows)),
        (KeyCode::PageDown, _) => KeyDecision::Action(TuiAction::PageDown(page_rows)),
        (KeyCode::Tab, _) => KeyDecision::Action(TuiAction::ToggleMarkAndDown),
        (KeyCode::BackTab, _) => KeyDecision::Action(TuiAction::ToggleMarkAndUp),
        (KeyCode::Backspace, _) => KeyDecision::Action(TuiAction::Backspace),
        (KeyCode::Delete, _) => KeyDecision::Action(TuiAction::Delete),
        (KeyCode::Left, _) => KeyDecision::Action(TuiAction::MoveCursorLeft),
        (KeyCode::Right, _) => KeyDecision::Action(TuiAction::MoveCursorRight),
        (KeyCode::Char(ch), modifiers)
            if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
        {
            KeyDecision::Action(TuiAction::Insert(ch))
        }
        _ => KeyDecision::Ignore,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum KeyDecision {
    Accept(Option<String>),
    Abort,
    Action(TuiAction),
    Ignore,
}

fn key_name(key: KeyEvent) -> Option<String> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => Some("enter".to_string()),
        (KeyCode::Esc, _) => Some("esc".to_string()),
        (KeyCode::Tab, _) => Some("tab".to_string()),
        (KeyCode::BackTab, _) => Some("shift-tab".to_string()),
        (KeyCode::Up, modifiers) if modifiers.contains(KeyModifiers::SHIFT) => {
            Some("shift-up".to_string())
        }
        (KeyCode::Down, modifiers) if modifiers.contains(KeyModifiers::SHIFT) => {
            Some("shift-down".to_string())
        }
        (KeyCode::PageUp, modifiers) if modifiers.contains(KeyModifiers::SHIFT) => {
            Some("shift-page-up".to_string())
        }
        (KeyCode::PageDown, modifiers) if modifiers.contains(KeyModifiers::SHIFT) => {
            Some("shift-page-down".to_string())
        }
        (KeyCode::Home, _) => Some("home".to_string()),
        (KeyCode::End, _) => Some("end".to_string()),
        (KeyCode::Up, _) => Some("up".to_string()),
        (KeyCode::Down, _) => Some("down".to_string()),
        (KeyCode::PageUp, _) => Some("page-up".to_string()),
        (KeyCode::PageDown, _) => Some("page-down".to_string()),
        (KeyCode::Char(ch), KeyModifiers::CONTROL) => Some(format!("ctrl-{ch}")),
        (KeyCode::Char(ch), KeyModifiers::ALT) => Some(format!("alt-{ch}")),
        (KeyCode::Char(ch), modifiers)
            if modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT =>
        {
            Some(format!("ctrl-{}", ch.to_ascii_lowercase()))
        }
        (KeyCode::Char(ch), modifiers) if modifiers == KeyModifiers::ALT | KeyModifiers::SHIFT => {
            Some(format!("alt-{}", ch.to_ascii_lowercase()))
        }
        _ => None,
    }
}
