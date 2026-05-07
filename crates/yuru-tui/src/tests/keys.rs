use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::api::{BindingAction, KeyBinding};
use crate::keys::{classify_key, KeyDecision};
use crate::state::TuiAction;

#[test]
fn expected_key_accepts_with_key_name() {
    let decision = classify_key(
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
        10,
        &["ctrl-y".to_string()],
        &[],
    );

    assert_eq!(decision, KeyDecision::Accept(Some("ctrl-y".to_string())));
}

#[test]
fn enter_accepts_without_expected_key_name() {
    let decision = classify_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        10,
        &[],
        &[],
    );

    assert_eq!(decision, KeyDecision::Accept(None));
}

#[test]
fn bind_key_can_abort() {
    let decision = classify_key(
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
        10,
        &[],
        &[KeyBinding {
            key: "ctrl-x".to_string(),
            action: BindingAction::Abort,
        }],
    );

    assert_eq!(decision, KeyDecision::Abort);
}

#[test]
fn shifted_navigation_keys_scroll_preview() {
    let decision = classify_key(
        KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT),
        10,
        &[],
        &[],
    );
    assert_eq!(decision, KeyDecision::Action(TuiAction::PreviewDown));

    let decision = classify_key(
        KeyEvent::new(KeyCode::PageUp, KeyModifiers::SHIFT),
        10,
        &[],
        &[],
    );
    assert_eq!(decision, KeyDecision::Action(TuiAction::PreviewPageUp(10)));
}

#[test]
fn bind_key_can_scroll_preview() {
    let decision = classify_key(
        KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
        10,
        &[],
        &[KeyBinding {
            key: "ctrl-j".to_string(),
            action: BindingAction::PreviewDown,
        }],
    );

    assert_eq!(decision, KeyDecision::Action(TuiAction::PreviewDown));
}

#[test]
fn bind_key_can_scroll_preview_to_edges() {
    let decision = classify_key(
        KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
        10,
        &[],
        &[KeyBinding {
            key: "end".to_string(),
            action: BindingAction::PreviewBottom,
        }],
    );

    assert_eq!(decision, KeyDecision::Action(TuiAction::PreviewBottom));
}

#[test]
fn bind_key_can_drive_basic_navigation_actions() {
    let decision = classify_key(
        KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
        10,
        &[],
        &[KeyBinding {
            key: "ctrl-j".to_string(),
            action: BindingAction::MoveSelectionDown,
        }],
    );
    assert_eq!(decision, KeyDecision::Action(TuiAction::MoveSelectionDown));

    let decision = classify_key(
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
        10,
        &[],
        &[KeyBinding {
            key: "ctrl-a".to_string(),
            action: BindingAction::MoveCursorStart,
        }],
    );
    assert_eq!(decision, KeyDecision::Action(TuiAction::MoveCursorStart));
}
