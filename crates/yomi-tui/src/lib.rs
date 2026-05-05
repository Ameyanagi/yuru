use std::collections::HashSet;
use std::io::{self, Write};

use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{
        self, disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use yomi_core::{
    match_positions, search, Candidate, KeyKind, LanguageBackend, ScoredCandidate, SearchConfig,
    SearchKey,
};

#[derive(Clone, Debug)]
pub struct TuiOptions {
    pub initial_query: String,
    pub prompt: String,
    pub height: Option<usize>,
    pub cycle: bool,
    pub multi: bool,
}

impl Default for TuiOptions {
    fn default() -> Self {
        Self {
            initial_query: String::new(),
            prompt: "> ".to_string(),
            height: None,
            cycle: false,
            multi: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TuiOutcome {
    Accepted { ids: Vec<usize>, query: String },
    NoSelection,
    Aborted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TuiState {
    query: String,
    cursor: usize,
    selected: usize,
    marked: HashSet<usize>,
}

impl TuiState {
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

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn marked(&self) -> &HashSet<usize> {
        &self.marked
    }

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
            TuiAction::ToggleMark | TuiAction::ToggleMarkAndDown | TuiAction::ToggleMarkAndUp => {}
        }
        self.clamp_selection(result_len);
    }

    fn apply_with_results(
        &mut self,
        action: TuiAction,
        results: &[ScoredCandidate],
        cycle: bool,
        multi: bool,
    ) {
        match action {
            TuiAction::ToggleMark => {
                self.toggle_selected_mark(results, multi);
                self.clamp_selection(results.len());
            }
            TuiAction::ToggleMarkAndDown => {
                self.toggle_selected_mark(results, multi);
                self.move_selection_down(results.len(), cycle);
                self.clamp_selection(results.len());
            }
            TuiAction::ToggleMarkAndUp => {
                self.toggle_selected_mark(results, multi);
                self.move_selection_up(results.len(), cycle);
                self.clamp_selection(results.len());
            }
            other => self.apply(other, results.len(), cycle),
        }
    }

    fn accepted_ids(&self, results: &[ScoredCandidate], multi: bool) -> Vec<usize> {
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

    fn toggle_selected_mark(&mut self, results: &[ScoredCandidate], multi: bool) {
        if !multi {
            return;
        }
        let Some(result) = results.get(self.selected) else {
            return;
        };
        if !self.marked.insert(result.id) {
            self.marked.remove(&result.id);
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

    fn clamp_selection(&mut self, result_len: usize) {
        if result_len == 0 {
            self.selected = 0;
        } else if self.selected >= result_len {
            self.selected = result_len - 1;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TuiAction {
    Insert(char),
    Backspace,
    Delete,
    ClearQuery,
    MoveCursorLeft,
    MoveCursorRight,
    MoveCursorStart,
    MoveCursorEnd,
    MoveSelectionUp,
    MoveSelectionDown,
    MoveSelectionFirst,
    MoveSelectionLast,
    PageUp(usize),
    PageDown(usize),
    ToggleMark,
    ToggleMarkAndDown,
    ToggleMarkAndUp,
}

pub fn run_interactive(
    candidates: &[Candidate],
    backend: &dyn LanguageBackend,
    config: SearchConfig,
    options: TuiOptions,
) -> Result<TuiOutcome> {
    enable_raw_mode()?;
    let mut output = io::stderr();
    execute!(output, EnterAlternateScreen, Hide)?;
    let _guard = TerminalGuard;

    let mut state = TuiState::new(options.initial_query);
    loop {
        let results = search(state.query(), candidates, backend, &config);
        state.clamp_selection(results.len());
        let viewport = Viewport::from_terminal(options.height);
        let render_context = RenderContext {
            candidates,
            prompt: &options.prompt,
            viewport,
            case_sensitive: config.case_sensitive,
            multi: options.multi,
        };
        render(&mut output, &state, &results, render_context)?;

        let Event::Key(key) = event::read()? else {
            continue;
        };

        match classify_key(key, viewport.rows) {
            KeyDecision::Accept => {
                let ids = state.accepted_ids(&results, options.multi);
                if ids.is_empty() {
                    return Ok(TuiOutcome::NoSelection);
                }
                return Ok(TuiOutcome::Accepted {
                    ids,
                    query: state.query().to_string(),
                });
            }
            KeyDecision::Abort => return Ok(TuiOutcome::Aborted),
            KeyDecision::Action(action) => {
                state.apply_with_results(action, &results, options.cycle, options.multi);
            }
            KeyDecision::Ignore => {}
        }
    }
}

fn classify_key(key: KeyEvent, page_rows: usize) -> KeyDecision {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, _) => KeyDecision::Accept,
        (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => KeyDecision::Abort,
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => KeyDecision::Action(TuiAction::ClearQuery),
        (KeyCode::Char('a'), KeyModifiers::CONTROL) | (KeyCode::Home, _) => {
            KeyDecision::Action(TuiAction::MoveCursorStart)
        }
        (KeyCode::Char('e'), KeyModifiers::CONTROL) | (KeyCode::End, _) => {
            KeyDecision::Action(TuiAction::MoveCursorEnd)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KeyDecision {
    Accept,
    Abort,
    Action(TuiAction),
    Ignore,
}

#[derive(Clone, Copy, Debug)]
struct Viewport {
    width: usize,
    rows: usize,
}

impl Viewport {
    fn from_terminal(height: Option<usize>) -> Self {
        let (width, terminal_rows) = terminal::size().unwrap_or((80, 24));
        let max_rows = usize::from(terminal_rows).saturating_sub(1).max(1);
        Self {
            width: usize::from(width).max(1),
            rows: height.unwrap_or(max_rows).clamp(1, max_rows),
        }
    }
}

fn render(
    output: &mut impl Write,
    state: &TuiState,
    results: &[ScoredCandidate],
    context: RenderContext<'_>,
) -> Result<()> {
    queue!(output, MoveTo(0, 0), Clear(ClearType::All))?;
    queue!(
        output,
        Print(truncate_to_width(
            &format!("{}{}", context.prompt, state.query()),
            context.viewport.width
        ))
    )?;

    let offset = scroll_offset(state.selected(), results.len(), context.viewport.rows);
    for (row, result) in results
        .iter()
        .skip(offset)
        .take(context.viewport.rows)
        .enumerate()
    {
        queue!(output, MoveTo(0, (row + 1) as u16))?;
        let mark = if context.multi && state.marked.contains(&result.id) {
            "*"
        } else {
            " "
        };
        if offset + row == state.selected() {
            queue!(
                output,
                SetAttribute(Attribute::Reverse),
                Print(">"),
                Print(mark)
            )?;
        } else {
            queue!(output, Print(" "), Print(mark))?;
        }
        render_highlighted_result(
            output,
            state.query(),
            result,
            context.candidates,
            context.case_sensitive,
            context.viewport.width.saturating_sub(2),
        )?;
        queue!(output, SetAttribute(Attribute::Reset))?;
    }

    let cursor_column =
        context.prompt.chars().count() + state.query()[..state.cursor()].chars().count();
    queue!(
        output,
        MoveTo(cursor_column.min(context.viewport.width - 1) as u16, 0)
    )?;
    output.flush()?;
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct RenderContext<'a> {
    candidates: &'a [Candidate],
    prompt: &'a str,
    viewport: Viewport,
    case_sensitive: bool,
    multi: bool,
}

fn scroll_offset(selected: usize, len: usize, rows: usize) -> usize {
    if len == 0 || selected < rows {
        0
    } else {
        selected + 1 - rows
    }
}

fn truncate_to_width(text: &str, width: usize) -> String {
    text.chars().take(width).collect()
}

fn render_highlighted_result(
    output: &mut impl Write,
    query: &str,
    result: &ScoredCandidate,
    candidates: &[Candidate],
    case_sensitive: bool,
    width: usize,
) -> Result<()> {
    for segment in highlight_segments_for_result(query, result, candidates, case_sensitive, width) {
        if segment.highlighted {
            queue!(
                output,
                SetForegroundColor(Color::Yellow),
                SetAttribute(Attribute::Bold)
            )?;
        }
        queue!(output, Print(segment.text))?;
        if segment.highlighted {
            queue!(output, ResetColor, SetAttribute(Attribute::NormalIntensity))?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HighlightSegment {
    text: String,
    highlighted: bool,
}

fn highlight_segments_for_result(
    query: &str,
    result: &ScoredCandidate,
    candidates: &[Candidate],
    case_sensitive: bool,
    width: usize,
) -> Vec<HighlightSegment> {
    let patterns = highlight_patterns(query);
    let positions = highlight_positions(&patterns, &result.display, case_sensitive);
    if positions.is_empty()
        && !patterns.is_empty()
        && matches!(
            result.key_kind,
            KeyKind::KanaReading
                | KeyKind::RomajiReading
                | KeyKind::PinyinFull
                | KeyKind::PinyinJoined
                | KeyKind::PinyinInitials
                | KeyKind::LearnedAlias
        )
    {
        if let Some(key) = matched_key(candidates, result) {
            let positions = source_map_highlight_positions(&patterns, key, case_sensitive, width);
            if !positions.is_empty() {
                return highlight_segments(&result.display, &positions, width);
            }
        }

        let positions = phonetic_fallback_positions(&result.display, width);
        if !positions.is_empty() {
            return highlight_segments(&result.display, &positions, width);
        }

        return highlight_segments(
            &result.display,
            &(0..result.display.chars().take(width).count()).collect(),
            width,
        );
    }

    highlight_segments(&result.display, &positions, width)
}

fn matched_key<'a>(candidates: &'a [Candidate], result: &ScoredCandidate) -> Option<&'a SearchKey> {
    candidates
        .get(result.id)
        .filter(|candidate| candidate.id == result.id)
        .or_else(|| {
            candidates
                .iter()
                .find(|candidate| candidate.id == result.id)
        })
        .and_then(|candidate| candidate.keys.get(result.key_index as usize))
}

fn highlight_patterns(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .filter_map(|raw| {
            if raw == "|" {
                return None;
            }

            let mut pattern = raw;
            if pattern.starts_with('!') {
                return None;
            }
            if let Some(stripped) = pattern.strip_prefix('\'') {
                pattern = stripped;
            }
            if let Some(stripped) = pattern.strip_prefix('^') {
                pattern = stripped;
            }
            if let Some(stripped) = pattern.strip_suffix('$') {
                pattern = stripped;
            }
            if let Some(stripped) = pattern.strip_suffix('\'') {
                pattern = stripped;
            }

            (!pattern.is_empty()).then(|| pattern.to_string())
        })
        .collect()
}

fn highlight_positions(patterns: &[String], text: &str, case_sensitive: bool) -> HashSet<usize> {
    let mut positions = HashSet::new();
    for pattern in patterns {
        if let Some(matched) = match_positions(pattern, text, case_sensitive) {
            positions.extend(matched.char_indices);
        }
    }
    positions
}

fn source_map_highlight_positions(
    patterns: &[String],
    key: &SearchKey,
    case_sensitive: bool,
    width: usize,
) -> HashSet<usize> {
    let Some(source_map) = &key.source_map else {
        return HashSet::new();
    };

    let mut positions = HashSet::new();
    for pattern in patterns {
        let Some(matched) = match_positions(pattern, &key.text, case_sensitive) else {
            continue;
        };

        for key_char_index in matched.char_indices {
            let Some(Some(span)) = source_map.get(key_char_index) else {
                continue;
            };
            positions.extend((span.start..span.end).filter(|position| *position < width));
        }
    }

    positions
}

fn highlight_segments(
    text: &str,
    highlighted_positions: &HashSet<usize>,
    width: usize,
) -> Vec<HighlightSegment> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_highlighted = None;

    for (char_index, ch) in text.chars().take(width).enumerate() {
        let highlighted = highlighted_positions.contains(&char_index);
        if current_highlighted == Some(highlighted) {
            current.push(ch);
            continue;
        }

        if let Some(highlighted) = current_highlighted {
            segments.push(HighlightSegment {
                text: std::mem::take(&mut current),
                highlighted,
            });
        }
        current.push(ch);
        current_highlighted = Some(highlighted);
    }

    if let Some(highlighted) = current_highlighted {
        segments.push(HighlightSegment {
            text: current,
            highlighted,
        });
    }

    segments
}

fn phonetic_fallback_positions(text: &str, width: usize) -> HashSet<usize> {
    text.chars()
        .take(width)
        .enumerate()
        .filter_map(|(index, ch)| is_visible_phonetic_surface(ch).then_some(index))
        .collect()
}

fn is_visible_phonetic_surface(ch: char) -> bool {
    ('\u{3040}'..='\u{309f}').contains(&ch)
        || ('\u{30a0}'..='\u{30ff}').contains(&ch)
        || ('\u{3400}'..='\u{4dbf}').contains(&ch)
        || ('\u{4e00}'..='\u{9fff}').contains(&ch)
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

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stderr(), Show, LeaveAlternateScreen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yomi_core::SourceSpan;

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

        state.apply_with_results(TuiAction::ToggleMarkAndDown, &results, false, true);
        state.apply_with_results(TuiAction::ToggleMarkAndDown, &results, false, true);

        assert_eq!(state.selected(), 2);
        assert!(state.marked().contains(&0));
        assert!(state.marked().contains(&1));
        assert_eq!(state.accepted_ids(&results, true), vec![0, 1]);
    }

    #[test]
    fn multi_select_toggle_is_ignored_when_multi_is_disabled() {
        let results = vec![scored_with_id(0, "alpha", KeyKind::Original)];
        let mut state = TuiState::new("");

        state.apply_with_results(TuiAction::ToggleMarkAndDown, &results, false, false);

        assert!(state.marked().is_empty());
        assert_eq!(state.selected(), 0);
        assert_eq!(state.accepted_ids(&results, false), vec![0]);
    }

    #[test]
    fn highlight_segments_mark_visible_fuzzy_positions() {
        let result = scored("src/module_42/README.md", KeyKind::Original);
        let segments = highlight_segments_for_result("read", &result, &[], false, 80);

        assert_eq!(
            segments,
            vec![
                HighlightSegment {
                    text: "src/module_42/".to_string(),
                    highlighted: false,
                },
                HighlightSegment {
                    text: "READ".to_string(),
                    highlighted: true,
                },
                HighlightSegment {
                    text: "ME.md".to_string(),
                    highlighted: false,
                },
            ]
        );
    }

    #[test]
    fn highlight_segments_prefer_stronger_later_chunk() {
        let result = scored("benches/search.rs", KeyKind::Original);
        let segments = highlight_segments_for_result("bsea", &result, &[], false, 80);

        assert_eq!(
            segments,
            vec![
                HighlightSegment {
                    text: "b".to_string(),
                    highlighted: true,
                },
                HighlightSegment {
                    text: "enches/".to_string(),
                    highlighted: false,
                },
                HighlightSegment {
                    text: "sea".to_string(),
                    highlighted: true,
                },
                HighlightSegment {
                    text: "rch.rs".to_string(),
                    highlighted: false,
                },
            ]
        );
    }

    #[test]
    fn highlight_segments_skip_negated_terms() {
        let result = scored("src/main.rs", KeyKind::Original);
        let segments = highlight_segments_for_result("src !main", &result, &[], false, 80);

        assert_eq!(
            segments,
            vec![
                HighlightSegment {
                    text: "src".to_string(),
                    highlighted: true,
                },
                HighlightSegment {
                    text: "/main.rs".to_string(),
                    highlighted: false,
                },
            ]
        );
    }

    #[test]
    fn highlight_segments_mark_phonetic_matches_when_reading_is_not_visible() {
        let result = scored("北京大学.txt", KeyKind::PinyinInitials);
        let segments = highlight_segments_for_result("bjdx", &result, &[], false, 80);

        assert_eq!(
            segments,
            vec![
                HighlightSegment {
                    text: "北京大学".to_string(),
                    highlighted: true,
                },
                HighlightSegment {
                    text: ".txt".to_string(),
                    highlighted: false,
                },
            ]
        );
    }

    #[test]
    fn highlight_segments_mark_japanese_surface_in_mixed_path() {
        let result = scored("tests/日本語.txt", KeyKind::RomajiReading);
        let segments = highlight_segments_for_result("ni", &result, &[], false, 80);

        assert_eq!(
            segments,
            vec![
                HighlightSegment {
                    text: "tests/".to_string(),
                    highlighted: false,
                },
                HighlightSegment {
                    text: "日本語".to_string(),
                    highlighted: true,
                },
                HighlightSegment {
                    text: ".txt".to_string(),
                    highlighted: false,
                },
            ]
        );
    }

    #[test]
    fn highlight_segments_mark_kana_surface_for_romaji_query() {
        let result = scored("カメラ.txt", KeyKind::RomajiReading);
        let segments = highlight_segments_for_result("kamera", &result, &[], false, 80);

        assert_eq!(
            segments,
            vec![
                HighlightSegment {
                    text: "カメラ".to_string(),
                    highlighted: true,
                },
                HighlightSegment {
                    text: ".txt".to_string(),
                    highlighted: false,
                },
            ]
        );
    }

    #[test]
    fn highlight_segments_use_source_map_for_japanese_reading() {
        let display = "tests/日本人の.txt";
        let key = SearchKey::romaji_reading("tests/nihonjinno.txt")
            .with_source_map(japanese_romaji_source_map());
        let candidates = vec![Candidate {
            id: 0,
            display: display.to_string(),
            keys: vec![key],
        }];
        let result = scored(display, KeyKind::RomajiReading);

        let segments = highlight_segments_for_result("ni", &result, &candidates, false, 80);
        assert_eq!(
            segments,
            vec![
                HighlightSegment {
                    text: "tests/".to_string(),
                    highlighted: false,
                },
                HighlightSegment {
                    text: "日本人".to_string(),
                    highlighted: true,
                },
                HighlightSegment {
                    text: "の.txt".to_string(),
                    highlighted: false,
                },
            ]
        );

        let segments = highlight_segments_for_result("no", &result, &candidates, false, 80);
        assert_eq!(
            segments,
            vec![
                HighlightSegment {
                    text: "tests/日本人".to_string(),
                    highlighted: false,
                },
                HighlightSegment {
                    text: "の".to_string(),
                    highlighted: true,
                },
                HighlightSegment {
                    text: ".txt".to_string(),
                    highlighted: false,
                },
            ]
        );
    }

    #[test]
    fn highlight_segments_use_source_map_for_chinese_initials() {
        let display = "北京大学.txt";
        let key = SearchKey::pinyin_initials("bjdx").with_source_map(vec![
            Some(SourceSpan { start: 0, end: 1 }),
            Some(SourceSpan { start: 1, end: 2 }),
            Some(SourceSpan { start: 2, end: 3 }),
            Some(SourceSpan { start: 3, end: 4 }),
        ]);
        let candidates = vec![Candidate {
            id: 0,
            display: display.to_string(),
            keys: vec![key],
        }];
        let result = scored(display, KeyKind::PinyinInitials);

        let segments = highlight_segments_for_result("bj", &result, &candidates, false, 80);

        assert_eq!(
            segments,
            vec![
                HighlightSegment {
                    text: "北京".to_string(),
                    highlighted: true,
                },
                HighlightSegment {
                    text: "大学.txt".to_string(),
                    highlighted: false,
                },
            ]
        );
    }

    fn scored(display: &str, key_kind: KeyKind) -> ScoredCandidate {
        scored_with_id(0, display, key_kind)
    }

    fn scored_with_id(id: usize, display: &str, key_kind: KeyKind) -> ScoredCandidate {
        ScoredCandidate {
            id,
            display: display.to_string(),
            score: 0,
            key_kind,
            key_index: 0,
        }
    }

    fn japanese_romaji_source_map() -> Vec<Option<SourceSpan>> {
        [
            Some(SourceSpan { start: 0, end: 1 }),
            Some(SourceSpan { start: 1, end: 2 }),
            Some(SourceSpan { start: 2, end: 3 }),
            Some(SourceSpan { start: 3, end: 4 }),
            Some(SourceSpan { start: 4, end: 5 }),
            Some(SourceSpan { start: 5, end: 6 }),
            Some(SourceSpan { start: 6, end: 9 }),
            Some(SourceSpan { start: 6, end: 9 }),
            Some(SourceSpan { start: 6, end: 9 }),
            Some(SourceSpan { start: 6, end: 9 }),
            Some(SourceSpan { start: 6, end: 9 }),
            Some(SourceSpan { start: 6, end: 9 }),
            Some(SourceSpan { start: 6, end: 9 }),
            Some(SourceSpan { start: 6, end: 9 }),
            Some(SourceSpan { start: 9, end: 10 }),
            Some(SourceSpan { start: 9, end: 10 }),
            Some(SourceSpan { start: 10, end: 11 }),
            Some(SourceSpan { start: 11, end: 12 }),
            Some(SourceSpan { start: 12, end: 13 }),
            Some(SourceSpan { start: 13, end: 14 }),
        ]
        .to_vec()
    }
}
