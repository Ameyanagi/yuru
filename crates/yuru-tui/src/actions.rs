use yuru_core::ScoredCandidate;

use crate::api::{TuiLayout, TuiOptions};
use crate::preview::PreviewCache;
use crate::state::{TuiAction, TuiState};

pub(crate) fn apply_interactive_action(
    action: TuiAction,
    state: &mut TuiState,
    preview_cache: &mut PreviewCache,
    results: &[ScoredCandidate],
    options: &TuiOptions,
    preview_rows: usize,
) {
    let action = visual_action_for_layout(action, options.layout);
    match action {
        TuiAction::PreviewUp => preview_cache.scroll_up(1, preview_rows),
        TuiAction::PreviewDown => preview_cache.scroll_down(1, preview_rows),
        TuiAction::PreviewPageUp(rows) => preview_cache.scroll_up(rows.max(1), preview_rows),
        TuiAction::PreviewPageDown(rows) => preview_cache.scroll_down(rows.max(1), preview_rows),
        TuiAction::PreviewTop => preview_cache.scroll_top(),
        TuiAction::PreviewBottom => preview_cache.scroll_bottom(preview_rows),
        TuiAction::Insert(_) | TuiAction::Backspace | TuiAction::Delete | TuiAction::ClearQuery
            if options.no_input => {}
        other => state.apply_with_results(
            other,
            results,
            options.cycle,
            options.multi,
            options.multi_limit,
        ),
    }
}

fn visual_action_for_layout(action: TuiAction, layout: TuiLayout) -> TuiAction {
    if !layout.list_bottom_up() {
        return action;
    }

    match action {
        TuiAction::MoveSelectionUp => TuiAction::MoveSelectionDown,
        TuiAction::MoveSelectionDown => TuiAction::MoveSelectionUp,
        TuiAction::PageUp(rows) => TuiAction::PageDown(rows),
        TuiAction::PageDown(rows) => TuiAction::PageUp(rows),
        other => other,
    }
}
