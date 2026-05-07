use yuru_core::KeyKind;

use crate::actions::apply_interactive_action;
use crate::api::{TuiLayout, TuiOptions};
use crate::preview::PreviewCache;
use crate::state::{TuiAction, TuiState};

use super::helpers::scored_with_id;

#[test]
fn bottom_up_layout_maps_selection_actions_to_visual_direction() {
    let results = vec![
        scored_with_id(0, "alpha", KeyKind::Original),
        scored_with_id(1, "beta", KeyKind::Original),
        scored_with_id(2, "gamma", KeyKind::Original),
    ];
    let mut state = TuiState::new("");
    let mut preview_cache = PreviewCache::default();
    let options = TuiOptions {
        layout: TuiLayout::Default,
        ..TuiOptions::default()
    };

    apply_interactive_action(
        TuiAction::MoveSelectionUp,
        &mut state,
        &mut preview_cache,
        &results,
        &options,
        3,
    );
    assert_eq!(state.selected(), 1);

    apply_interactive_action(
        TuiAction::MoveSelectionDown,
        &mut state,
        &mut preview_cache,
        &results,
        &options,
        3,
    );
    assert_eq!(state.selected(), 0);

    apply_interactive_action(
        TuiAction::PageUp(2),
        &mut state,
        &mut preview_cache,
        &results,
        &options,
        3,
    );
    assert_eq!(state.selected(), 2);
}

#[test]
fn top_down_layout_keeps_selection_actions_logical() {
    let results = vec![
        scored_with_id(0, "alpha", KeyKind::Original),
        scored_with_id(1, "beta", KeyKind::Original),
    ];
    let mut state = TuiState::new("");
    let mut preview_cache = PreviewCache::default();
    let options = TuiOptions {
        layout: TuiLayout::Reverse,
        ..TuiOptions::default()
    };

    apply_interactive_action(
        TuiAction::MoveSelectionDown,
        &mut state,
        &mut preview_cache,
        &results,
        &options,
        2,
    );
    assert_eq!(state.selected(), 1);

    apply_interactive_action(
        TuiAction::MoveSelectionUp,
        &mut state,
        &mut preview_cache,
        &results,
        &options,
        2,
    );
    assert_eq!(state.selected(), 0);
}
