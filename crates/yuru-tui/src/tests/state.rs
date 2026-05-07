use yuru_core::KeyKind;

use crate::state::{TuiAction, TuiState};

use super::helpers::scored_with_id;

#[test]
fn editing_actions_update_query_and_cursor() {
    let mut state = TuiState::new("ab");

    state.apply(TuiAction::MoveCursorLeft, 3, false);
    state.apply(TuiAction::Insert('x'), 3, false);
    assert_eq!(state.query(), "axb");
    assert_eq!(state.cursor(), 2);

    state.apply(TuiAction::Backspace, 3, false);
    assert_eq!(state.query(), "ab");
    assert_eq!(state.cursor(), 1);

    state.apply(TuiAction::Delete, 3, false);
    assert_eq!(state.query(), "a");
    assert_eq!(state.cursor(), 1);
}

#[test]
fn editing_actions_handle_utf8_boundaries() {
    let mut state = TuiState::new("あb");

    state.apply(TuiAction::MoveCursorLeft, 3, false);
    state.apply(TuiAction::Backspace, 3, false);

    assert_eq!(state.query(), "b");
    assert_eq!(state.cursor(), 0);
}

#[test]
fn selection_clamps_without_cycle() {
    let mut state = TuiState::new("");

    state.apply(TuiAction::MoveSelectionDown, 2, false);
    state.apply(TuiAction::MoveSelectionDown, 2, false);
    assert_eq!(state.selected(), 1);

    state.apply(TuiAction::MoveSelectionUp, 2, false);
    state.apply(TuiAction::MoveSelectionUp, 2, false);
    assert_eq!(state.selected(), 0);
}

#[test]
fn selection_wraps_with_cycle() {
    let mut state = TuiState::new("");

    state.apply(TuiAction::MoveSelectionUp, 3, true);
    assert_eq!(state.selected(), 2);

    state.apply(TuiAction::MoveSelectionDown, 3, true);
    assert_eq!(state.selected(), 0);
}
#[test]
fn multi_select_marks_rows_and_accepts_marked_ids_in_result_order() {
    let results = vec![
        scored_with_id(0, "alpha", KeyKind::Original),
        scored_with_id(1, "beta", KeyKind::Original),
        scored_with_id(2, "gamma", KeyKind::Original),
    ];
    let mut state = TuiState::new("");

    state.apply_with_results(TuiAction::ToggleMarkAndDown, &results, false, true, None);
    state.apply_with_results(TuiAction::ToggleMarkAndDown, &results, false, true, None);

    assert_eq!(state.selected(), 2);
    assert!(state.marked().contains(&0));
    assert!(state.marked().contains(&1));
    assert_eq!(state.accepted_ids(&results, true), vec![0, 1]);
}

#[test]
fn multi_select_toggle_is_ignored_when_multi_is_disabled() {
    let results = vec![scored_with_id(0, "alpha", KeyKind::Original)];
    let mut state = TuiState::new("");

    state.apply_with_results(TuiAction::ToggleMarkAndDown, &results, false, false, None);

    assert!(state.marked().is_empty());
    assert_eq!(state.selected(), 0);
    assert_eq!(state.accepted_ids(&results, false), vec![0]);
}
