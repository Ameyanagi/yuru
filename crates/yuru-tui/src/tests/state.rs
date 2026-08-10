use yuru_core::{KeyKind, ScoredCandidate};

use crate::state::{SelectionTarget, TuiAction, TuiState};

use super::helpers::scored_with_id;

/// `count` results whose ids are their positions, for tests that only care about length.
fn rows(count: usize) -> Vec<ScoredCandidate> {
    (0..count)
        .map(|id| scored_with_id(id, &format!("row{id}"), KeyKind::Original))
        .collect()
}

/// Results with explicit ids, so a replacement can share candidates with the old set.
fn rows_with_ids(ids: &[usize]) -> Vec<ScoredCandidate> {
    ids.iter()
        .map(|id| scored_with_id(*id, &format!("row{id}"), KeyKind::Original))
        .collect()
}

#[test]
fn editing_actions_update_query_and_cursor() {
    let results = rows(3);
    let mut state = TuiState::new("ab");

    state.apply(TuiAction::MoveCursorLeft, &results, false);
    state.apply(TuiAction::Insert('x'), &results, false);
    assert_eq!(state.query(), "axb");
    assert_eq!(state.cursor(), 2);

    state.apply(TuiAction::Backspace, &results, false);
    assert_eq!(state.query(), "ab");
    assert_eq!(state.cursor(), 1);

    state.apply(TuiAction::Delete, &results, false);
    assert_eq!(state.query(), "a");
    assert_eq!(state.cursor(), 1);
}

#[test]
fn editing_actions_handle_utf8_boundaries() {
    let results = rows(3);
    let mut state = TuiState::new("あb");

    state.apply(TuiAction::MoveCursorLeft, &results, false);
    state.apply(TuiAction::Backspace, &results, false);

    assert_eq!(state.query(), "b");
    assert_eq!(state.cursor(), 0);
}

#[test]
fn readline_word_actions_handle_separators_and_utf8() {
    let results = rows(3);
    let mut state = TuiState::new("hello 世界");

    state.apply(TuiAction::MoveCursorWordLeft, &results, false);
    assert_eq!(state.cursor(), "hello ".len());

    let mut state = TuiState::new("hello 世界");
    state.apply(TuiAction::DeleteWord, &results, false);
    assert_eq!(state.query(), "hello ");
    assert_eq!(state.cursor(), "hello ".len());

    state.apply(TuiAction::DeleteWord, &results, false);
    assert_eq!(state.query(), "");
    assert_eq!(state.cursor(), 0);

    let mut state = TuiState::new("hello 世界");
    state.apply(TuiAction::MoveCursorStart, &results, false);
    state.apply(TuiAction::MoveCursorWordRight, &results, false);
    assert_eq!(state.cursor(), "hello".len());
    state.apply(TuiAction::MoveCursorWordRight, &results, false);
    assert_eq!(state.cursor(), "hello 世界".len());
}

#[test]
fn readline_delete_actions_preserve_cursor_invariants() {
    let results = rows(3);
    let mut state = TuiState::new("hello world");

    state.apply(TuiAction::MoveCursorWordLeft, &results, false);
    state.apply(TuiAction::DeleteToEnd, &results, false);
    assert_eq!(state.query(), "hello ");
    assert_eq!(state.cursor(), "hello ".len());

    state.apply(TuiAction::DeleteOrExit, &results, false);
    assert_eq!(state.query(), "hello ");
    assert_eq!(state.cursor(), "hello ".len());
}

#[test]
fn selection_clamps_without_cycle() {
    let results = rows(2);
    let mut state = TuiState::new("");

    state.apply(TuiAction::MoveSelectionDown, &results, false);
    state.apply(TuiAction::MoveSelectionDown, &results, false);
    assert_eq!(state.selected(), 1);

    state.apply(TuiAction::MoveSelectionUp, &results, false);
    state.apply(TuiAction::MoveSelectionUp, &results, false);
    assert_eq!(state.selected(), 0);
}

#[test]
fn selection_wraps_with_cycle() {
    let results = rows(3);
    let mut state = TuiState::new("");

    state.apply(TuiAction::MoveSelectionUp, &results, true);
    assert_eq!(state.selected(), 2);

    state.apply(TuiAction::MoveSelectionDown, &results, true);
    assert_eq!(state.selected(), 0);
}

#[test]
fn paging_and_first_last_move_within_the_result_list() {
    let results = rows(10);
    let mut state = TuiState::new("");

    state.apply(TuiAction::PageDown(4), &results, false);
    assert_eq!(state.selected(), 4);
    assert_eq!(state.target(), SelectionTarget::Row(4));

    state.apply(TuiAction::PageDown(100), &results, false);
    assert_eq!(state.selected(), 9);

    state.apply(TuiAction::PageUp(3), &results, false);
    assert_eq!(state.selected(), 6);

    state.apply(TuiAction::MoveSelectionFirst, &results, false);
    assert_eq!(state.selected(), 0);
    assert_eq!(state.target(), SelectionTarget::Row(0));

    state.apply(TuiAction::MoveSelectionLast, &results, false);
    assert_eq!(state.selected(), 9);
    assert_eq!(state.target(), SelectionTarget::Row(9));
}

#[test]
fn moving_the_selection_anchors_it_to_that_candidate() {
    let results = rows_with_ids(&[7, 3, 9]);
    let mut state = TuiState::new("");

    assert_eq!(state.target(), SelectionTarget::Top);
    state.apply(TuiAction::MoveSelectionDown, &results, false);

    assert_eq!(state.selected(), 1);
    assert_eq!(state.target(), SelectionTarget::Row(3));
}

#[test]
fn a_replacement_result_set_keeps_the_selection_on_the_same_candidate() {
    // The selected row moves from position 1 to position 0 in the replacement. An index
    // carried across unchanged would land on a different candidate, which is the whole
    // bug this is here to prevent.
    let stale = rows_with_ids(&[7, 3, 9]);
    let mut state = TuiState::new("");
    state.apply(TuiAction::MoveSelectionDown, &stale, false);
    assert_eq!(state.selected(), 1);

    let live = rows_with_ids(&[3, 9]);
    state.reselect(&live);

    assert_eq!(state.selected(), 0);
    assert_eq!(state.target(), SelectionTarget::Row(3));
    assert_eq!(state.accepted_ids(state.target(), &live, false), vec![3]);
}

#[test]
fn a_selection_that_did_not_survive_the_replacement_resets_to_the_top() {
    let stale = rows_with_ids(&[7, 3, 9]);
    let mut state = TuiState::new("");
    state.apply(TuiAction::MoveSelectionDown, &stale, false);

    let live = rows_with_ids(&[7, 9]);
    state.reselect(&live);

    assert_eq!(state.selected(), 0);
    // Back to following the top, not silently re-aimed at whatever took the row's place.
    assert_eq!(state.target(), SelectionTarget::Top);
}

#[test]
fn an_accept_aimed_at_a_row_that_is_gone_selects_nothing() {
    let stale = rows_with_ids(&[7, 3, 9]);
    let mut state = TuiState::new("");
    state.apply(TuiAction::MoveSelectionDown, &stale, false);
    let committed = state.target();

    // The replacement drops the selected candidate. Resolving the accept the user
    // already made must refuse rather than hand back row 0 of the new set.
    let live = rows_with_ids(&[7, 9]);
    state.reselect(&live);

    assert!(state.accepted_ids(committed, &live, false).is_empty());
}

#[test]
fn an_unmoved_selection_follows_the_top_of_the_replacement() {
    let stale = rows_with_ids(&[7, 3, 9]);
    let mut state = TuiState::new("a");
    state.reselect(&stale);
    assert_eq!(state.target(), SelectionTarget::Top);

    let live = rows_with_ids(&[4, 7]);
    state.reselect(&live);

    assert_eq!(state.selected(), 0);
    assert_eq!(state.accepted_ids(state.target(), &live, false), vec![4]);
    // ...and an empty replacement leaves nothing to accept.
    state.reselect(&[]);
    assert_eq!(state.selected(), 0);
    assert!(state.accepted_ids(state.target(), &[], false).is_empty());
}

#[test]
fn editing_the_query_releases_the_anchor() {
    let stale = rows_with_ids(&[7, 3, 9]);
    let mut state = TuiState::new("ab");
    state.apply(TuiAction::MoveSelectionDown, &stale, false);
    assert_eq!(state.target(), SelectionTarget::Row(3));

    // Typing starts a new search, and the row the user was on is not what they asked
    // for any more: the selection goes back to the top of whatever lands next.
    state.apply(TuiAction::Insert('c'), &stale, false);
    assert_eq!(state.target(), SelectionTarget::Top);

    let live = rows_with_ids(&[3, 9]);
    state.reselect(&live);
    assert_eq!(state.accepted_ids(state.target(), &live, false), vec![3]);
}

#[test]
fn selecting_in_an_empty_result_list_leaves_nothing_selected() {
    let mut state = TuiState::new("");

    state.apply(TuiAction::MoveSelectionDown, &[], false);
    state.apply(TuiAction::MoveSelectionLast, &[], false);

    assert_eq!(state.selected(), 0);
    assert_eq!(state.target(), SelectionTarget::Top);
    assert!(state.accepted_ids(state.target(), &[], false).is_empty());
}

#[test]
fn multi_select_marks_rows_and_accepts_marked_ids() {
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
    assert_eq!(
        state.accepted_ids(state.target(), &results, true),
        vec![0, 1]
    );
}

#[test]
fn marked_ids_are_accepted_in_the_order_they_were_marked() {
    let results = vec![
        scored_with_id(0, "alpha", KeyKind::Original),
        scored_with_id(1, "beta", KeyKind::Original),
        scored_with_id(2, "gamma", KeyKind::Original),
    ];
    let mut state = TuiState::new("");

    state.apply_with_results(TuiAction::MoveSelectionLast, &results, false, true, None);
    state.apply_with_results(TuiAction::ToggleMarkAndUp, &results, false, true, None);
    state.apply_with_results(TuiAction::ToggleMark, &results, false, true, None);

    assert_eq!(state.selected(), 1);
    assert!(state.is_marked(2));
    assert!(state.is_marked(1));
    assert_eq!(state.marked(), [2, 1]);
    assert_eq!(
        state.accepted_ids(state.target(), &results, true),
        vec![2, 1]
    );
}

#[test]
fn marks_survive_a_result_set_that_no_longer_contains_them() {
    // fzf keeps what you marked when you refine the query; dropping the marks that fell
    // out of the current result set would silently discard them on accept.
    let results = rows_with_ids(&[0, 1, 2]);
    let mut state = TuiState::new("");
    state.apply_with_results(TuiAction::ToggleMarkAndDown, &results, false, true, None);
    state.apply_with_results(TuiAction::ToggleMark, &results, false, true, None);
    assert_eq!(state.marked(), [0, 1]);

    let refined = rows_with_ids(&[1]);
    state.reselect(&refined);

    assert_eq!(
        state.accepted_ids(state.target(), &refined, true),
        vec![0, 1]
    );
}

#[test]
fn unmarking_removes_the_id_and_keeps_the_order_of_the_rest() {
    let results = rows_with_ids(&[0, 1, 2]);
    let mut state = TuiState::new("");
    for _ in 0..3 {
        state.apply_with_results(TuiAction::ToggleMarkAndDown, &results, false, true, None);
    }
    assert_eq!(state.marked(), [0, 1, 2]);

    state.apply_with_results(TuiAction::MoveSelectionFirst, &results, false, true, None);
    state.apply_with_results(TuiAction::ToggleMarkAndDown, &results, false, true, None);

    assert!(!state.is_marked(0));
    assert_eq!(state.marked(), [1, 2]);
}

#[test]
fn the_mark_limit_counts_marks_not_rows() {
    let results = rows_with_ids(&[0, 1, 2]);
    let mut state = TuiState::new("");
    for _ in 0..3 {
        state.apply_with_results(TuiAction::ToggleMarkAndDown, &results, false, true, Some(2));
    }

    assert_eq!(state.marked(), [0, 1]);
}

#[test]
fn multi_select_toggle_is_ignored_when_multi_is_disabled() {
    let results = vec![scored_with_id(0, "alpha", KeyKind::Original)];
    let mut state = TuiState::new("");

    state.apply_with_results(TuiAction::ToggleMarkAndDown, &results, false, false, None);

    assert!(state.marked().is_empty());
    assert_eq!(state.selected(), 0);
    assert_eq!(state.accepted_ids(state.target(), &results, false), vec![0]);
}
