use std::collections::HashSet;

use yuru_core::ScoredCandidate;

#[derive(Clone, Debug, Eq, PartialEq)]
/// Mutable query, cursor, selection, and marking state.
pub struct TuiState {
    query: String,
    cursor: usize,
    selected: usize,
    marked: HashSet<usize>,
}

impl TuiState {
    /// Creates TUI state with the given initial query.
    pub fn new(query: impl Into<String>) -> Self {
        let query = query.into();
        let cursor = query.len();
        Self {
            query,
            cursor,
            selected: 0,
            marked: HashSet::new(),
        }
    }

    /// Returns the current query text.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Returns the byte index of the query cursor.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Returns the selected result index.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Returns the marked candidate ids.
    pub fn marked(&self) -> &HashSet<usize> {
        &self.marked
    }

    /// Applies a state action for a result list of `result_len`.
    pub fn apply(&mut self, action: TuiAction, result_len: usize, cycle: bool) {
        match action {
            TuiAction::Insert(ch) => self.insert(ch),
            TuiAction::Backspace => self.backspace(),
            TuiAction::Delete => self.delete(),
            TuiAction::ClearQuery => self.clear_query(),
            TuiAction::MoveCursorLeft => self.move_cursor_left(),
            TuiAction::MoveCursorRight => self.move_cursor_right(),
            TuiAction::MoveCursorStart => self.cursor = 0,
            TuiAction::MoveCursorEnd => self.cursor = self.query.len(),
            TuiAction::MoveSelectionUp => self.move_selection_up(result_len, cycle),
            TuiAction::MoveSelectionDown => self.move_selection_down(result_len, cycle),
            TuiAction::MoveSelectionFirst => self.selected = 0,
            TuiAction::MoveSelectionLast => {
                self.selected = result_len.saturating_sub(1);
            }
            TuiAction::PageUp(rows) => {
                self.selected = self.selected.saturating_sub(rows.max(1));
            }
            TuiAction::PageDown(rows) => {
                if result_len > 0 {
                    self.selected = (self.selected + rows.max(1)).min(result_len - 1);
                }
            }
            TuiAction::ToggleMark
            | TuiAction::ToggleMarkAndDown
            | TuiAction::ToggleMarkAndUp
            | TuiAction::PreviewUp
            | TuiAction::PreviewDown
            | TuiAction::PreviewPageUp(_)
            | TuiAction::PreviewPageDown(_)
            | TuiAction::PreviewTop
            | TuiAction::PreviewBottom => {}
        }
        self.clamp_selection(result_len);
    }

    pub(crate) fn apply_with_results(
        &mut self,
        action: TuiAction,
        results: &[ScoredCandidate],
        cycle: bool,
        multi: bool,
        multi_limit: Option<usize>,
    ) {
        match action {
            TuiAction::ToggleMark => {
                self.toggle_selected_mark(results, multi, multi_limit);
                self.clamp_selection(results.len());
            }
            TuiAction::ToggleMarkAndDown => {
                self.toggle_selected_mark(results, multi, multi_limit);
                self.move_selection_down(results.len(), cycle);
                self.clamp_selection(results.len());
            }
            TuiAction::ToggleMarkAndUp => {
                self.toggle_selected_mark(results, multi, multi_limit);
                self.move_selection_up(results.len(), cycle);
                self.clamp_selection(results.len());
            }
            other => self.apply(other, results.len(), cycle),
        }
    }

    pub(crate) fn accepted_ids(&self, results: &[ScoredCandidate], multi: bool) -> Vec<usize> {
        if multi && !self.marked.is_empty() {
            return results
                .iter()
                .filter_map(|result| self.marked.contains(&result.id).then_some(result.id))
                .collect();
        }

        results
            .get(self.selected)
            .map(|result| vec![result.id])
            .unwrap_or_default()
    }

    fn toggle_selected_mark(
        &mut self,
        results: &[ScoredCandidate],
        multi: bool,
        multi_limit: Option<usize>,
    ) {
        if !multi {
            return;
        }
        let Some(result) = results.get(self.selected) else {
            return;
        };
        if self.marked.contains(&result.id) {
            self.marked.remove(&result.id);
        } else if multi_limit.is_none_or(|limit| self.marked.len() < limit) {
            self.marked.insert(result.id);
        }
    }

    fn insert(&mut self, ch: char) {
        self.query.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        self.selected = 0;
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let previous = previous_boundary(&self.query, self.cursor);
        self.query.drain(previous..self.cursor);
        self.cursor = previous;
        self.selected = 0;
    }

    fn delete(&mut self) {
        if self.cursor == self.query.len() {
            return;
        }
        let next = next_boundary(&self.query, self.cursor);
        self.query.drain(self.cursor..next);
        self.selected = 0;
    }

    fn clear_query(&mut self) {
        self.query.clear();
        self.cursor = 0;
        self.selected = 0;
    }

    fn move_cursor_left(&mut self) {
        self.cursor = previous_boundary(&self.query, self.cursor);
    }

    fn move_cursor_right(&mut self) {
        self.cursor = next_boundary(&self.query, self.cursor);
    }

    fn move_selection_up(&mut self, result_len: usize, cycle: bool) {
        if result_len == 0 {
            self.selected = 0;
        } else if self.selected == 0 {
            self.selected = if cycle { result_len - 1 } else { 0 };
        } else {
            self.selected -= 1;
        }
    }

    fn move_selection_down(&mut self, result_len: usize, cycle: bool) {
        if result_len == 0 {
            self.selected = 0;
        } else if self.selected + 1 >= result_len {
            self.selected = if cycle { 0 } else { result_len - 1 };
        } else {
            self.selected += 1;
        }
    }

    pub(crate) fn clamp_selection(&mut self, result_len: usize) {
        if result_len == 0 {
            self.selected = 0;
        } else if self.selected >= result_len {
            self.selected = result_len - 1;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// State transition used by the TUI event loop.
pub enum TuiAction {
    /// Insert a character at the query cursor.
    Insert(char),
    /// Delete the character before the cursor.
    Backspace,
    /// Delete the character at the cursor.
    Delete,
    /// Clear the query text.
    ClearQuery,
    /// Move the query cursor left.
    MoveCursorLeft,
    /// Move the query cursor right.
    MoveCursorRight,
    /// Move the query cursor to the start.
    MoveCursorStart,
    /// Move the query cursor to the end.
    MoveCursorEnd,
    /// Move the selected row up.
    MoveSelectionUp,
    /// Move the selected row down.
    MoveSelectionDown,
    /// Move to the first row.
    MoveSelectionFirst,
    /// Move to the last row.
    MoveSelectionLast,
    /// Move selection up by the given number of rows.
    PageUp(usize),
    /// Move selection down by the given number of rows.
    PageDown(usize),
    /// Toggle the selected row mark.
    ToggleMark,
    /// Toggle the selected row mark and move down.
    ToggleMarkAndDown,
    /// Toggle the selected row mark and move up.
    ToggleMarkAndUp,
    /// Scroll preview up.
    PreviewUp,
    /// Scroll preview down.
    PreviewDown,
    /// Scroll preview up by the given number of rows.
    PreviewPageUp(usize),
    /// Scroll preview down by the given number of rows.
    PreviewPageDown(usize),
    /// Scroll preview to the top.
    PreviewTop,
    /// Scroll preview to the bottom.
    PreviewBottom,
}

fn previous_boundary(text: &str, cursor: usize) -> usize {
    text[..cursor]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_boundary(text: &str, cursor: usize) -> usize {
    text[cursor..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| cursor + index)
        .unwrap_or(text.len())
}
