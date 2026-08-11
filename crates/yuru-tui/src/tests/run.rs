use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use yuru_core::{KeyKind, ScoredCandidate, SearchConfig};

use crate::api::{TuiLayout, TuiOutcome, TuiStyle};
use crate::render::{render, RenderContext, Viewport};
use crate::run::{
    accept_outcome, classify_terminal_event, live_search_identity, resolve_case_sensitive,
    search_config_for_query, PendingAccept, ResultSet, TerminalEvent,
};
use crate::search_worker::SearchIdentity;
use crate::state::{SelectionTarget, TuiAction, TuiState};

use super::helpers::{force_test_color_output, scored, scored_with_id};

#[test]
fn resize_event_requests_a_redraw() {
    assert_eq!(
        classify_terminal_event(&Event::Resize(80, 24)),
        TerminalEvent::Redraw
    );
}

#[test]
fn key_press_is_actionable_and_other_events_are_ignored() {
    let press = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    assert_eq!(
        classify_terminal_event(&Event::Key(press)),
        TerminalEvent::Key(press)
    );
    assert_eq!(
        classify_terminal_event(&Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))),
        TerminalEvent::Ignore
    );
    assert_eq!(
        classify_terminal_event(&Event::FocusGained),
        TerminalEvent::Ignore
    );
    assert_eq!(
        classify_terminal_event(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })),
        TerminalEvent::Ignore
    );
}

#[test]
fn redraw_paints_a_frame_sized_for_the_current_terminal() {
    let state = TuiState::new("al");
    let results = vec![scored("alphabet-soup", KeyKind::Original)];
    let wide = render_to_string(&state, &results, Viewport { width: 40, rows: 3 });
    let narrow = render_to_string(&state, &results, Viewport { width: 12, rows: 3 });

    // The matched prefix carries highlight sequences, so only the tail is contiguous text.
    assert!(wide.contains("phabet-soup"), "{wide:?}");
    assert!(narrow.contains("phabet-s"), "{narrow:?}");
    assert!(!narrow.contains("phabet-soup"), "{narrow:?}");
}

#[test]
fn smart_case_follows_the_live_query_in_both_directions() {
    let config = SearchConfig::default();
    let mut state = TuiState::new("abc");
    assert!(!resolve_case_sensitive(&config, true, state.query()));

    state.apply(TuiAction::Insert('D'), &[], false);
    assert_eq!(state.query(), "abcD");
    assert!(resolve_case_sensitive(&config, true, state.query()));

    state.apply(TuiAction::Backspace, &[], false);
    assert_eq!(state.query(), "abc");
    assert!(!resolve_case_sensitive(&config, true, state.query()));
}

#[test]
fn explicit_case_settings_never_follow_the_query() {
    let insensitive = SearchConfig {
        case_sensitive: false,
        ..SearchConfig::default()
    };
    let sensitive = SearchConfig {
        case_sensitive: true,
        ..SearchConfig::default()
    };

    // `--ignore-case` stays case-insensitive even once the query has uppercase text.
    assert!(!resolve_case_sensitive(&insensitive, false, "abC"));
    // `--no-ignore-case` stays case-sensitive even with an all-lowercase query.
    assert!(resolve_case_sensitive(&sensitive, false, "abc"));
}

#[test]
fn search_config_for_query_only_changes_case_sensitivity() {
    let config = SearchConfig {
        limit: 42,
        exact: true,
        ..SearchConfig::default()
    };

    let upper = search_config_for_query(&config, true, "abC");
    assert!(upper.case_sensitive);
    assert_eq!(upper.limit, 42);
    assert!(upper.exact);
    assert_eq!(upper.tiebreaks, config.tiebreaks);

    let lower = search_config_for_query(&config, true, "abc");
    assert!(!lower.case_sensitive);
}

#[test]
fn results_are_only_current_for_the_search_they_answer() {
    let config = SearchConfig::default();
    let rows = vec![scored("ABC-match", KeyKind::Original)];
    // Results for `ab`, which smart case searched case-insensitively.
    let applied = ResultSet {
        identity: Some(live_search_identity(&config, true, "ab")),
        rows,
    };

    assert!(applied.is_current(&live_search_identity(&config, true, "ab")));
    // The query grew an uppercase char: different text and a different case policy.
    assert!(!applied.is_current(&live_search_identity(&config, true, "abC")));
    // Same query text under the other case policy is a different search as well; this is
    // the case query-text equality alone cannot see.
    assert!(!applied.is_current(&SearchIdentity {
        query: "ab".to_string(),
        case_sensitive: true,
    }));
}

#[test]
fn a_result_set_that_has_never_been_filled_is_not_current() {
    let config = SearchConfig::default();
    let empty = ResultSet::default();

    assert!(empty.rows().is_empty());
    assert!(!empty.is_current(&live_search_identity(&config, true, "")));
}

#[test]
fn the_worker_tag_and_the_live_identity_describe_the_same_search() {
    // The worker tags a response from the config it was handed, while the loop compares
    // against the live query; both have to resolve the same case policy or a correct
    // result set would be discarded as stale forever.
    let config = SearchConfig::default();
    for query in ["", "ab", "abC", "ÄBC"] {
        let requested = SearchIdentity::new(query, &search_config_for_query(&config, true, query));
        assert_eq!(requested, live_search_identity(&config, true, query));
    }
}

#[test]
fn accepting_with_no_matching_row_reports_no_selection() {
    let state = TuiState::new("abC");

    assert_eq!(
        accept_outcome(&state, SelectionTarget::Top, &[], false, None),
        TuiOutcome::NoSelection
    );
    assert_eq!(
        accept_outcome(
            &state,
            SelectionTarget::Top,
            &[scored("abCdef-ok", KeyKind::Original)],
            false,
            None
        ),
        TuiOutcome::Accepted {
            ids: vec![0],
            query: "abC".to_string(),
            expect: None,
        }
    );
}

#[test]
fn an_accept_held_for_a_slow_search_still_aims_at_the_row_it_was_made_on() {
    // Rows for `ab` are on screen while the search for `abC` runs. The user moves onto
    // the second of them and presses Enter, which cannot be answered yet.
    let stale = vec![
        scored_with_id(0, "ABC", KeyKind::Original),
        scored_with_id(1, "abC-one", KeyKind::Original),
        scored_with_id(2, "abC-two", KeyKind::Original),
    ];
    let mut state = TuiState::new("abC");
    state.apply(TuiAction::MoveSelectionDown, &stale, false);
    let accept = PendingAccept::capture(&state, None);

    // The replacement drops `ABC`, so the row the user was on is now first.
    let live = vec![
        scored_with_id(1, "abC-one", KeyKind::Original),
        scored_with_id(2, "abC-two", KeyKind::Original),
    ];
    state.reselect(&live);

    assert_eq!(state.selected(), 0);
    assert_eq!(
        accept_outcome(&state, accept.target, &live, false, accept.expect),
        TuiOutcome::Accepted {
            ids: vec![1],
            query: "abC".to_string(),
            expect: None,
        }
    );
}

#[test]
fn a_held_accept_whose_row_is_not_in_the_live_results_accepts_nothing() {
    let stale = vec![
        scored_with_id(0, "ABC", KeyKind::Original),
        scored_with_id(1, "abC-one", KeyKind::Original),
    ];
    let mut state = TuiState::new("abC");
    // Down then up leaves the user on `ABC`, which the case-sensitive live query drops.
    state.apply(TuiAction::MoveSelectionDown, &stale, false);
    state.apply(TuiAction::MoveSelectionUp, &stale, false);
    let accept = PendingAccept::capture(&state, None);

    let live = vec![scored_with_id(1, "abC-one", KeyKind::Original)];
    state.reselect(&live);

    assert_eq!(
        accept_outcome(&state, accept.target, &live, false, accept.expect),
        TuiOutcome::NoSelection
    );
}

#[test]
fn an_accept_made_without_moving_takes_the_top_of_the_live_results() {
    let stale = vec![scored_with_id(0, "ABC", KeyKind::Original)];
    let mut state = TuiState::new("ab");
    state.apply(TuiAction::Insert('C'), &stale, false);
    let accept = PendingAccept::capture(&state, Some("ctrl-t".to_string()));
    assert_eq!(accept.target, SelectionTarget::Top);

    let live = vec![scored_with_id(1, "abC-one", KeyKind::Original)];
    state.reselect(&live);

    assert_eq!(
        accept_outcome(&state, accept.target, &live, false, accept.expect),
        TuiOutcome::Accepted {
            ids: vec![1],
            query: "abC".to_string(),
            expect: Some("ctrl-t".to_string()),
        }
    );
}

fn render_to_string(state: &TuiState, results: &[ScoredCandidate], viewport: Viewport) -> String {
    force_test_color_output();
    let mut output = Vec::new();
    render(
        &mut output,
        state,
        results,
        RenderContext {
            candidates: &[],
            prompt: "> ",
            header: None,
            footer: None,
            viewport,
            layout: TuiLayout::Default,
            preview: None,
            style: &TuiStyle::default(),
            highlight_line: true,
            case_sensitive: false,
            multi: false,
            no_input: false,
            pointer: ">",
            marker: "*",
            ellipsis: "..",
            ansi: false,
        },
    )
    .unwrap();

    String::from_utf8(output).unwrap()
}
