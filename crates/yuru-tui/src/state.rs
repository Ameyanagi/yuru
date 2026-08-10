use yuru_core::ScoredCandidate;

/// What the selection points at, independently of where that lands in a result list.
///
/// A result list is replaced wholesale every time a search finishes, so a bare row index
/// stops meaning the same row the moment that happens. Recording *what* is selected
/// rather than *where* keeps the meaning across the replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionTarget {
    /// No row has been chosen since the query last changed, so the selection follows the
    /// top of whatever the live search returns. This is the state right after typing.
    Top,
    /// A specific candidate the user moved to, identified by id and never by position.
    Row(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Mutable query, cursor, selection, and marking state.
pub struct TuiState {
    query: String,
    cursor: usize,
    /// Row the cursor is drawn on. This is a cache of where [`Self::target`] currently
    /// sits, maintained by [`Self::reselect`] and by the selection-moving actions; it is
    /// never the authority on what is selected.
    selected: usize,
    target: SelectionTarget,
    /// Marked candidate ids, in the order they were marked. Marks are identities too, so
    /// they survive a query change; the order is the one fzf prints them in. A list
    /// rather than a set because it is only as long as the user has pressed the mark
    /// key, and the order is part of the contract.
    marked: Vec<usize>,
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
            target: SelectionTarget::Top,
            marked: Vec::new(),
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

    /// Returns the selected result index, for drawing the cursor.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Returns what the selection points at.
    ///
    /// This is what an accept has to be resolved against: it stays meaningful while a
    /// search is outstanding, whereas [`Self::selected`] does not.
    pub fn target(&self) -> SelectionTarget {
        self.target
    }

    /// Returns the marked candidate ids, in the order they were marked.
    pub fn marked(&self) -> &[usize] {
        &self.marked
    }

    /// Returns whether `id` is marked.
    pub fn is_marked(&self, id: usize) -> bool {
        self.marked.contains(&id)
    }

    /// Applies a state action against the result list the user is looking at.
    ///
    /// `results` is needed rather than just its length because every selection move
    /// re-anchors [`Self::target`] to the row it lands on.
    pub fn apply(&mut self, action: TuiAction, results: &[ScoredCandidate], cycle: bool) {
        self.apply_with_results(action, results, cycle, false, None);
    }

    pub(crate) fn apply_with_results(
        &mut self,
        action: TuiAction,
        results: &[ScoredCandidate],
        cycle: bool,
        multi: bool,
        multi_limit: Option<usize>,
    ) {
        let result_len = results.len();
        match action {
            TuiAction::Insert(ch) => self.insert(ch),
            TuiAction::Backspace => self.backspace(),
            TuiAction::Delete => self.delete(),
            TuiAction::DeleteOrExit => self.delete(),
            TuiAction::DeleteToEnd => self.delete_to_end(),
            TuiAction::DeleteWord => self.delete_word(),
            TuiAction::ClearQuery => self.clear_query(),
            TuiAction::MoveCursorLeft => self.move_cursor_left(),
            TuiAction::MoveCursorRight => self.move_cursor_right(),
            TuiAction::MoveCursorStart => self.cursor = 0,
            TuiAction::MoveCursorEnd => self.cursor = self.query.len(),
            TuiAction::MoveCursorWordLeft => self.move_cursor_word_left(),
            TuiAction::MoveCursorWordRight => self.move_cursor_word_right(),
            TuiAction::MoveSelectionUp => {
                self.move_selection_up(result_len, cycle);
                self.anchor_to_selected(results);
            }
            TuiAction::MoveSelectionDown => {
                self.move_selection_down(result_len, cycle);
                self.anchor_to_selected(results);
            }
            TuiAction::MoveSelectionFirst => {
                self.selected = 0;
                self.anchor_to_selected(results);
            }
            TuiAction::MoveSelectionLast => {
                self.selected = result_len.saturating_sub(1);
                self.anchor_to_selected(results);
            }
            TuiAction::PageUp(rows) => {
                self.selected = self.selected.saturating_sub(rows.max(1));
                self.anchor_to_selected(results);
            }
            TuiAction::PageDown(rows) => {
                if result_len > 0 {
                    self.selected = (self.selected + rows.max(1)).min(result_len - 1);
                }
                self.anchor_to_selected(results);
            }
            TuiAction::ToggleMark => {
                self.toggle_selected_mark(results, multi, multi_limit);
            }
            TuiAction::ToggleMarkAndDown => {
                self.toggle_selected_mark(results, multi, multi_limit);
                self.move_selection_down(result_len, cycle);
                self.anchor_to_selected(results);
            }
            TuiAction::ToggleMarkAndUp => {
                self.toggle_selected_mark(results, multi, multi_limit);
                self.move_selection_up(result_len, cycle);
                self.anchor_to_selected(results);
            }
            TuiAction::PreviewUp
            | TuiAction::PreviewDown
            | TuiAction::PreviewPageUp(_)
            | TuiAction::PreviewPageDown(_)
            | TuiAction::PreviewTop
            | TuiAction::PreviewBottom => {}
        }
    }

    /// Re-resolves the selection against a freshly landed result list.
    ///
    /// This is the only place a result list replacement is allowed to move the cursor.
    /// A [`SelectionTarget::Row`] that is still present keeps the selection on that same
    /// candidate wherever it now sits. A row that is gone resets to the top and to
    /// following the top, which is what fzf does and the least surprising of the
    /// alternatives; an accept that was already committed against the lost row is
    /// resolved from its own captured target and so is never redirected here.
    pub(crate) fn reselect(&mut self, results: &[ScoredCandidate]) {
        match self.target {
            SelectionTarget::Top => self.selected = 0,
            SelectionTarget::Row(id) => match results.iter().position(|row| row.id == id) {
                Some(index) => self.selected = index,
                None => self.reset_selection(),
            },
        }
    }

    /// Resolves `target` and the marks against `results` into accepted candidate ids.
    ///
    /// `target` is passed in rather than read from `self` because an accept made while a
    /// search was outstanding has to resolve the selection as it was when the key was
    /// pressed, not as it is once the replacement rows arrive.
    pub(crate) fn accepted_ids(
        &self,
        target: SelectionTarget,
        results: &[ScoredCandidate],
        multi: bool,
    ) -> Vec<usize> {
        if multi && !self.marked.is_empty() {
            return self.marked.clone();
        }

        match target {
            // The row the user chose has to still be in the live results; if it is not,
            // there is nothing to accept, and picking whatever took its place would
            // return a row the user never selected.
            SelectionTarget::Row(id) => {
                if results.iter().any(|result| result.id == id) {
                    vec![id]
                } else {
                    Vec::new()
                }
            }
            SelectionTarget::Top => results
                .first()
                .map(|result| vec![result.id])
                .unwrap_or_default(),
        }
    }

    /// Points the selection back at the top of the list and at following the top.
    fn reset_selection(&mut self) {
        self.selected = 0;
        self.target = SelectionTarget::Top;
    }

    /// Binds the target to the row the cursor now sits on.
    fn anchor_to_selected(&mut self, results: &[ScoredCandidate]) {
        if self.selected >= results.len() {
            self.selected = results.len().saturating_sub(1);
        }
        self.target = match results.get(self.selected) {
            Some(result) => SelectionTarget::Row(result.id),
            None => SelectionTarget::Top,
        };
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
            self.marked.retain(|marked| *marked != result.id);
        } else if multi_limit.is_none_or(|limit| self.marked.len() < limit) {
            self.marked.push(result.id);
        }
    }

    fn insert(&mut self, ch: char) {
        self.query.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        self.reset_selection();
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let previous = previous_boundary(&self.query, self.cursor);
        self.query.drain(previous..self.cursor);
        self.cursor = previous;
        self.reset_selection();
    }

    fn delete(&mut self) {
        if self.cursor == self.query.len() {
            return;
        }
        let next = next_boundary(&self.query, self.cursor);
        self.query.drain(self.cursor..next);
        self.reset_selection();
    }

    fn delete_to_end(&mut self) {
        self.query.truncate(self.cursor);
        self.reset_selection();
    }

    fn delete_word(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let word_start = previous_word_boundary(&self.query, self.cursor);
        self.query.drain(word_start..self.cursor);
        self.cursor = word_start;
        self.reset_selection();
    }

    fn clear_query(&mut self) {
        self.query.clear();
        self.cursor = 0;
        self.reset_selection();
    }

    fn move_cursor_left(&mut self) {
        self.cursor = previous_boundary(&self.query, self.cursor);
    }

    fn move_cursor_right(&mut self) {
        self.cursor = next_boundary(&self.query, self.cursor);
    }

    fn move_cursor_word_left(&mut self) {
        self.cursor = previous_word_boundary(&self.query, self.cursor);
    }

    fn move_cursor_word_right(&mut self) {
        self.cursor = next_word_boundary(&self.query, self.cursor);
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
    /// Delete from cursor to end of line.
    DeleteToEnd,
    /// Delete word before cursor.
    DeleteWord,
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
    /// Move the query cursor to the start of the previous word.
    MoveCursorWordLeft,
    /// Move the query cursor to the end of the next word.
    MoveCursorWordRight,
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
    /// Delete character and exit if query becomes empty (Ctrl+D).
    DeleteOrExit,
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

fn previous_word_boundary(text: &str, cursor: usize) -> usize {
    let mut iter = text[..cursor].char_indices().rev().peekable();

    // Skip any trailing boundary characters (cursor may sit right after whitespace).
    while let Some(&(_, ch)) = iter.peek() {
        if !is_word_boundary(ch) {
            break;
        }
        iter.next();
    }

    // Skip word characters; the first boundary we peek at marks the word start.
    while let Some(&(index, ch)) = iter.peek() {
        if is_word_boundary(ch) {
            return index + ch.len_utf8();
        }
        iter.next();
    }

    0
}

fn next_word_boundary(text: &str, cursor: usize) -> usize {
    let mut iter = text[cursor..].char_indices().peekable();

    let first_is_word = iter
        .peek()
        .map(|(_, ch)| !is_word_boundary(*ch))
        .unwrap_or(false);

    if first_is_word {
        return iter
            .find(|(_, ch)| is_word_boundary(*ch))
            .map(|(index, _)| cursor + index)
            .unwrap_or(text.len());
    }

    for (_, ch) in iter.by_ref() {
        if !is_word_boundary(ch) {
            return iter
                .find(|(_, next_ch)| is_word_boundary(*next_ch))
                .map(|(index, _)| cursor + index)
                .unwrap_or(text.len());
        }
    }

    text.len()
}

fn is_word_boundary(ch: char) -> bool {
    ch.is_whitespace() || ch == '/' || ch == '-' || ch == '_' || ch == '.'
}
