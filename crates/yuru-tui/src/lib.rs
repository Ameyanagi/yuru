//! Terminal user interface for Yuru.
//!
//! The TUI provides fzf-like navigation, multi-select, expected keys, a small
//! supported binding subset, and source-span-aware match highlighting.

use std::collections::HashSet;
use std::io::{self, Write};
#[cfg(feature = "image")]
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    mpsc::{self, Receiver, TryRecvError},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

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
#[cfg(feature = "image")]
use image::DynamicImage;
#[cfg(feature = "image")]
use ratatui::{buffer::Buffer, layout::Rect};
#[cfg(feature = "image")]
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
    Resize, ResizeEncodeRender,
};
use yuru_core::{
    match_positions, search, Candidate, KeyKind, LanguageBackend, ScoredCandidate, SearchConfig,
    SearchKey,
};

const STREAM_DRAIN_BATCH: usize = 2048;
const PREVIEW_DEBOUNCE: Duration = Duration::from_millis(50);
const PREVIEW_WORKER_POLL: Duration = Duration::from_millis(25);
const SEARCH_WORKER_POLL: Duration = Duration::from_millis(16);
const PREVIEW_LOADING: &str = "loading preview...";
#[cfg(feature = "image")]
const IMAGE_PREVIEW_LOADING: &str = "loading image preview...";

#[derive(Clone, Debug)]
pub struct TuiOptions {
    pub initial_query: String,
    pub prompt: String,
    pub header: Option<String>,
    pub footer: Option<String>,
    pub expect_keys: Vec<String>,
    pub bindings: Vec<KeyBinding>,
    pub height: Option<usize>,
    pub layout: TuiLayout,
    pub preview: Option<String>,
    pub preview_shell: Option<String>,
    pub style: TuiStyle,
    pub cycle: bool,
    pub multi: bool,
    pub multi_limit: Option<usize>,
    pub no_input: bool,
    pub pointer: String,
    pub marker: String,
    pub ellipsis: String,
}

impl Default for TuiOptions {
    fn default() -> Self {
        Self {
            initial_query: String::new(),
            prompt: "> ".to_string(),
            header: None,
            footer: None,
            expect_keys: Vec::new(),
            bindings: Vec::new(),
            height: None,
            layout: TuiLayout::default(),
            preview: None,
            preview_shell: None,
            style: TuiStyle::default(),
            cycle: false,
            multi: false,
            multi_limit: None,
            no_input: false,
            pointer: ">".to_string(),
            marker: "*".to_string(),
            ellipsis: "..".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TuiLayout {
    #[default]
    Default,
    Reverse,
    ReverseList,
}

impl TuiLayout {
    fn prompt_at_bottom(self) -> bool {
        matches!(self, Self::Default | Self::ReverseList)
    }

    fn list_bottom_up(self) -> bool {
        matches!(self, Self::Default)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TuiRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<TuiRgb> for Color {
    fn from(color: TuiRgb) -> Self {
        Self::Rgb {
            r: color.r,
            g: color.g,
            b: color.b,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TuiStyle {
    pub pointer: Option<TuiRgb>,
    pub highlight: Option<TuiRgb>,
    pub highlight_selected: Option<TuiRgb>,
}

impl TuiStyle {
    fn pointer_color(&self) -> Option<Color> {
        self.pointer.map(Color::from)
    }

    fn highlight_color(&self, selected: bool) -> Color {
        if selected {
            self.highlight_selected
                .or(self.highlight)
                .map(Color::from)
                .unwrap_or(Color::Yellow)
        } else {
            self.highlight.map(Color::from).unwrap_or(Color::Yellow)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyBinding {
    pub key: String,
    pub action: BindingAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingAction {
    Accept,
    Abort,
    ClearQuery,
    MoveSelectionUp,
    MoveSelectionDown,
    MoveSelectionFirst,
    MoveSelectionLast,
    PageUp,
    PageDown,
    ToggleMark,
    ToggleMarkAndDown,
    ToggleMarkAndUp,
    MoveCursorStart,
    MoveCursorEnd,
    MoveCursorLeft,
    MoveCursorRight,
    Backspace,
    Delete,
    PreviewUp,
    PreviewDown,
    PreviewPageUp,
    PreviewPageDown,
    PreviewTop,
    PreviewBottom,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TuiOutcome {
    Accepted {
        ids: Vec<usize>,
        query: String,
        expect: Option<String>,
    },
    NoSelection,
    Aborted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CandidateStreamMessage {
    Candidate(Candidate),
    Finished,
    Error(String),
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

    fn apply_with_results(
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
    PreviewUp,
    PreviewDown,
    PreviewPageUp(usize),
    PreviewPageDown(usize),
    PreviewTop,
    PreviewBottom,
}

struct SearchRequest {
    seq: u64,
    query: String,
    candidates: Option<Arc<Vec<Candidate>>>,
    config: SearchConfig,
}

struct SearchResponse {
    seq: u64,
    query: String,
    results: Vec<ScoredCandidate>,
}

enum SearchCommand {
    Append(Vec<Candidate>),
    Search(SearchRequest),
}

struct SearchWorker {
    sender: mpsc::Sender<SearchCommand>,
    receiver: Receiver<SearchResponse>,
}

impl SearchWorker {
    fn new(backend: Arc<dyn LanguageBackend>) -> Self {
        let (request_sender, request_receiver) = mpsc::channel::<SearchCommand>();
        let (response_sender, response_receiver) = mpsc::channel::<SearchResponse>();

        thread::spawn(move || {
            let mut owned_candidates = Vec::new();
            while let Ok(command) = request_receiver.recv() {
                let mut request = None;
                apply_search_command(command, &mut owned_candidates, &mut request);
                while let Ok(command) = request_receiver.try_recv() {
                    apply_search_command(command, &mut owned_candidates, &mut request);
                }

                let Some(request) = request else {
                    continue;
                };

                let results = if let Some(candidates) = &request.candidates {
                    search(
                        &request.query,
                        candidates.as_ref(),
                        backend.as_ref(),
                        &request.config,
                    )
                } else {
                    search(
                        &request.query,
                        &owned_candidates,
                        backend.as_ref(),
                        &request.config,
                    )
                };

                if response_sender
                    .send(SearchResponse {
                        seq: request.seq,
                        query: request.query,
                        results,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Self {
            sender: request_sender,
            receiver: response_receiver,
        }
    }

    fn request(
        &mut self,
        seq: u64,
        query: String,
        candidates: Arc<Vec<Candidate>>,
        config: SearchConfig,
    ) {
        let _ = self.sender.send(SearchCommand::Search(SearchRequest {
            seq,
            query,
            candidates: Some(candidates),
            config,
        }));
    }

    fn request_owned(&mut self, seq: u64, query: String, config: SearchConfig) {
        let _ = self.sender.send(SearchCommand::Search(SearchRequest {
            seq,
            query,
            candidates: None,
            config,
        }));
    }

    fn append(&mut self, candidates: Vec<Candidate>) {
        if !candidates.is_empty() {
            let _ = self.sender.send(SearchCommand::Append(candidates));
        }
    }

    fn try_recv(&mut self) -> Option<SearchResponse> {
        self.receiver.try_recv().ok()
    }
}

fn apply_search_command(
    command: SearchCommand,
    owned_candidates: &mut Vec<Candidate>,
    request: &mut Option<SearchRequest>,
) {
    match command {
        SearchCommand::Append(mut candidates) => owned_candidates.append(&mut candidates),
        SearchCommand::Search(search_request) => *request = Some(search_request),
    }
}

fn request_snapshot_search(
    worker: &mut SearchWorker,
    search_seq: &mut u64,
    latest_requested_seq: &mut u64,
    query: &str,
    candidates: Arc<Vec<Candidate>>,
    config: SearchConfig,
) {
    *search_seq = search_seq.saturating_add(1);
    *latest_requested_seq = *search_seq;
    worker.request(*search_seq, query.to_string(), candidates, config);
}

fn request_owned_search(
    worker: &mut SearchWorker,
    search_seq: &mut u64,
    latest_requested_seq: &mut u64,
    query: &str,
    config: SearchConfig,
) {
    *search_seq = search_seq.saturating_add(1);
    *latest_requested_seq = *search_seq;
    worker.request_owned(*search_seq, query.to_string(), config);
}

fn interaction_poll_timeout(
    preview_timeout: Option<Duration>,
    search_timeout: Option<Duration>,
    source_timeout: Option<Duration>,
) -> Option<Duration> {
    [preview_timeout, search_timeout, source_timeout]
        .into_iter()
        .flatten()
        .min()
}

pub fn run_interactive(
    candidates: &[Candidate],
    backend: Arc<dyn LanguageBackend>,
    config: SearchConfig,
    options: TuiOptions,
) -> Result<TuiOutcome> {
    enable_raw_mode()?;
    let mut output = io::stderr();
    execute!(output, EnterAlternateScreen, Hide)?;
    let _guard = TerminalGuard;

    let mut state = TuiState::new(options.initial_query.clone());
    let mut preview_cache = PreviewCache::default();
    let candidates = Arc::new(candidates.to_vec());
    let mut search_worker = SearchWorker::new(backend);
    let mut search_seq = 0;
    let mut latest_requested_seq = 0;
    let mut latest_applied_seq = 0;
    let mut results = Vec::new();
    let mut render_needed = true;
    request_snapshot_search(
        &mut search_worker,
        &mut search_seq,
        &mut latest_requested_seq,
        state.query(),
        candidates.clone(),
        config.clone(),
    );

    loop {
        while let Some(response) = search_worker.try_recv() {
            if response.seq >= latest_applied_seq && response.query == state.query() {
                latest_applied_seq = response.seq;
                results = response.results;
                state.clamp_selection(results.len());
                render_needed = true;
            }
        }

        let viewport = Viewport::from_terminal(options.height);
        let preview_geometry = preview_geometry(
            viewport,
            options.layout,
            options.preview.is_some() && !results.is_empty(),
        );
        preview_cache.request_for_selection(
            options.preview.as_deref(),
            options.preview_shell.as_deref(),
            &results,
            &state,
            preview_geometry,
        );
        render_needed |= preview_cache.poll();
        preview_cache.clamp_scroll(viewport.rows);
        render_needed |= preview_cache.prepare_image(
            preview_geometry
                .map(|geometry| geometry.columns)
                .unwrap_or(0),
            viewport.rows,
        );
        if render_needed {
            let render_context = RenderContext {
                candidates: candidates.as_ref(),
                prompt: &options.prompt,
                header: options.header.as_deref(),
                footer: options.footer.as_deref(),
                viewport,
                layout: options.layout,
                preview: preview_cache.render(),
                style: &options.style,
                case_sensitive: config.case_sensitive,
                multi: options.multi,
                no_input: options.no_input,
                pointer: &options.pointer,
                marker: &options.marker,
                ellipsis: &options.ellipsis,
            };
            render(&mut output, &state, &results, render_context)?;
            render_needed = false;
        }

        let poll_timeout = interaction_poll_timeout(
            preview_cache.next_poll_timeout(),
            (latest_applied_seq < latest_requested_seq).then_some(SEARCH_WORKER_POLL),
            None,
        );
        let key = if let Some(timeout) = poll_timeout {
            if !event::poll(timeout)? {
                continue;
            }
            match event::read()? {
                Event::Key(key) => key,
                _ => continue,
            }
        } else {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            key
        };

        match classify_key(key, viewport.rows, &options.expect_keys, &options.bindings) {
            KeyDecision::Accept(expect) => {
                let ids = state.accepted_ids(&results, options.multi);
                if ids.is_empty() {
                    return Ok(TuiOutcome::NoSelection);
                }
                return Ok(TuiOutcome::Accepted {
                    ids,
                    query: state.query().to_string(),
                    expect,
                });
            }
            KeyDecision::Abort => return Ok(TuiOutcome::Aborted),
            KeyDecision::Action(action) => {
                let old_query = state.query().to_string();
                apply_interactive_action(
                    action,
                    &mut state,
                    &mut preview_cache,
                    &results,
                    &options,
                    viewport.rows,
                );
                if state.query() != old_query {
                    request_snapshot_search(
                        &mut search_worker,
                        &mut search_seq,
                        &mut latest_requested_seq,
                        state.query(),
                        candidates.clone(),
                        config.clone(),
                    );
                }
                render_needed = true;
            }
            KeyDecision::Ignore => {}
        }
    }
}

pub fn run_interactive_streaming(
    receiver: Receiver<CandidateStreamMessage>,
    backend: Arc<dyn LanguageBackend>,
    config: SearchConfig,
    options: TuiOptions,
) -> Result<TuiOutcome> {
    enable_raw_mode()?;
    let mut output = io::stderr();
    execute!(output, EnterAlternateScreen, Hide)?;
    let _guard = TerminalGuard;

    let mut state = TuiState::new(options.initial_query.clone());
    let mut candidates = Vec::new();
    let mut results = Vec::new();
    let mut reading = true;
    let mut dirty = true;
    let mut preview_cache = PreviewCache::default();
    let mut search_worker = SearchWorker::new(backend);
    let mut search_seq = 0;
    let mut latest_requested_seq = 0;
    let mut latest_applied_seq = 0;
    let mut render_needed = true;

    loop {
        let streamed_candidates = drain_stream(&receiver, &mut candidates, &mut reading)?;
        if !streamed_candidates.is_empty() {
            search_worker.append(streamed_candidates);
            dirty = true;
        }

        if dirty {
            request_owned_search(
                &mut search_worker,
                &mut search_seq,
                &mut latest_requested_seq,
                state.query(),
                config.clone(),
            );
            dirty = false;
        }

        while let Some(response) = search_worker.try_recv() {
            if response.seq >= latest_applied_seq && response.query == state.query() {
                latest_applied_seq = response.seq;
                results = response.results;
                state.clamp_selection(results.len());
                render_needed = true;
            }
        }

        let viewport = Viewport::from_terminal(options.height);
        let preview_geometry = preview_geometry(
            viewport,
            options.layout,
            options.preview.is_some() && !results.is_empty(),
        );
        preview_cache.request_for_selection(
            options.preview.as_deref(),
            options.preview_shell.as_deref(),
            &results,
            &state,
            preview_geometry,
        );
        let preview_changed = preview_cache.poll();
        render_needed |= preview_changed;
        render_needed |= preview_cache.prepare_image(
            preview_geometry
                .map(|geometry| geometry.columns)
                .unwrap_or(0),
            viewport.rows,
        );
        if render_needed {
            preview_cache.clamp_scroll(viewport.rows);
            let render_context = RenderContext {
                candidates: &candidates,
                prompt: &options.prompt,
                header: options.header.as_deref(),
                footer: options.footer.as_deref(),
                viewport,
                layout: options.layout,
                preview: preview_cache.render(),
                style: &options.style,
                case_sensitive: config.case_sensitive,
                multi: options.multi,
                no_input: options.no_input,
                pointer: &options.pointer,
                marker: &options.marker,
                ellipsis: &options.ellipsis,
            };
            render(&mut output, &state, &results, render_context)?;
            render_needed = false;
        }

        let source_poll_interval = if reading {
            Duration::from_millis(25)
        } else {
            Duration::from_millis(250)
        };
        let poll_interval = interaction_poll_timeout(
            preview_cache.next_poll_timeout(),
            (latest_applied_seq < latest_requested_seq).then_some(SEARCH_WORKER_POLL),
            Some(source_poll_interval),
        )
        .unwrap_or(source_poll_interval);
        if !event::poll(poll_interval)? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };
        let viewport = Viewport::from_terminal(options.height);
        match classify_key(key, viewport.rows, &options.expect_keys, &options.bindings) {
            KeyDecision::Accept(expect) => {
                let ids = state.accepted_ids(&results, options.multi);
                if ids.is_empty() {
                    return Ok(TuiOutcome::NoSelection);
                }
                return Ok(TuiOutcome::Accepted {
                    ids,
                    query: state.query().to_string(),
                    expect,
                });
            }
            KeyDecision::Abort => return Ok(TuiOutcome::Aborted),
            KeyDecision::Action(action) => {
                let old_query = state.query().to_string();
                apply_interactive_action(
                    action,
                    &mut state,
                    &mut preview_cache,
                    &results,
                    &options,
                    viewport.rows,
                );
                if state.query() != old_query {
                    dirty = true;
                }
                render_needed = true;
            }
            KeyDecision::Ignore => {}
        }
    }
}

fn drain_stream(
    receiver: &Receiver<CandidateStreamMessage>,
    candidates: &mut Vec<Candidate>,
    reading: &mut bool,
) -> Result<Vec<Candidate>> {
    let mut streamed = Vec::new();
    for _ in 0..STREAM_DRAIN_BATCH {
        match receiver.try_recv() {
            Ok(CandidateStreamMessage::Candidate(candidate)) => {
                streamed.push(candidate.clone());
                candidates.push(candidate);
            }
            Ok(CandidateStreamMessage::Finished) => {
                *reading = false;
            }
            Ok(CandidateStreamMessage::Error(error)) => anyhow::bail!(error),
            Err(TryRecvError::Empty) => return Ok(streamed),
            Err(TryRecvError::Disconnected) => {
                *reading = false;
                return Ok(streamed);
            }
        }
    }
    Ok(streamed)
}

fn apply_interactive_action(
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PreviewGeometry {
    columns: usize,
    lines: usize,
    left: usize,
    top: usize,
}

#[derive(Default)]
struct PreviewCache {
    key: Option<PreviewKey>,
    content: Option<PreviewContent>,
    pending: Option<PreviewRequest>,
    worker: Option<PreviewWorker>,
    scroll: usize,
    #[cfg(feature = "image")]
    image_picker: Option<Picker>,
}

type PreviewKey = (String, String, usize, String, PreviewGeometry);

struct PreviewRequest {
    key: PreviewKey,
    command: String,
    shell: Option<String>,
    item: String,
    geometry: PreviewGeometry,
    requested_at: Instant,
}

struct PreviewWorker {
    key: PreviewKey,
    receiver: Receiver<(PreviewKey, PreviewPayload)>,
}

enum PreviewContent {
    Text(String),
    #[cfg(feature = "image")]
    Image(ImagePreview),
}

enum PreviewPayload {
    Text(String),
    #[cfg(feature = "image")]
    Image(DynamicImage),
}

#[cfg(feature = "image")]
struct ImagePreview {
    image: DynamicImage,
    state: Option<Box<StatefulProtocol>>,
    worker: Option<ImageEncodeWorker>,
    area: Option<(u16, u16)>,
    error: Option<String>,
}

#[cfg(feature = "image")]
struct ImageEncodeWorker {
    area: (u16, u16),
    receiver: Receiver<ImageEncodeResult>,
}

#[cfg(feature = "image")]
enum ImageEncodeResult {
    Ready {
        area: (u16, u16),
        state: Box<StatefulProtocol>,
    },
    Error {
        area: (u16, u16),
        message: String,
    },
}

impl PreviewCache {
    fn request_for_selection(
        &mut self,
        command: Option<&str>,
        shell: Option<&str>,
        results: &[ScoredCandidate],
        state: &TuiState,
        geometry: Option<PreviewGeometry>,
    ) {
        let Some(command) = command else {
            self.clear();
            return;
        };
        let Some(selected) = results.get(state.selected()) else {
            self.clear();
            return;
        };
        let Some(geometry) = geometry else {
            self.clear();
            return;
        };
        let key = (
            command.to_string(),
            shell.unwrap_or_default().to_string(),
            selected.id,
            selected.display.clone(),
            geometry,
        );

        if self.key.as_ref() != Some(&key) {
            if self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.key == key)
                || self.worker.as_ref().is_some_and(|worker| worker.key == key)
            {
                return;
            }

            self.key = None;
            self.content = None;
            self.scroll = 0;
            self.worker = None;
            self.pending = Some(PreviewRequest {
                key,
                command: command.to_string(),
                shell: shell.map(str::to_string),
                item: selected.display.clone(),
                geometry,
                requested_at: Instant::now(),
            });
        }
    }

    fn poll(&mut self) -> bool {
        self.start_ready_worker();
        let changed = self.receive_worker_result();
        #[cfg(feature = "image")]
        {
            changed | self.receive_image_result()
        }
        #[cfg(not(feature = "image"))]
        {
            changed
        }
    }

    fn start_ready_worker(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let Some(pending) = &self.pending else {
            return;
        };
        if pending.requested_at.elapsed() < PREVIEW_DEBOUNCE {
            return;
        }

        let pending = self.pending.take().expect("pending preview exists");
        let (sender, receiver) = mpsc::channel();
        let key = pending.key.clone();
        let worker_key = pending.key.clone();
        thread::spawn(move || {
            let payload = run_preview_command(
                &pending.command,
                pending.shell.as_deref(),
                &pending.item,
                pending.geometry,
            );
            let _ = sender.send((key, payload));
        });
        self.worker = Some(PreviewWorker {
            key: worker_key,
            receiver,
        });
    }

    fn receive_worker_result(&mut self) -> bool {
        let Some(worker) = &self.worker else {
            return false;
        };
        match worker.receiver.try_recv() {
            Ok((key, payload)) => {
                self.worker = None;
                self.replace(key, payload);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.worker = None;
                false
            }
        }
    }

    fn next_poll_timeout(&self) -> Option<Duration> {
        if self.worker.is_some() {
            return Some(PREVIEW_WORKER_POLL);
        }
        #[cfg(feature = "image")]
        if matches!(
            &self.content,
            Some(PreviewContent::Image(ImagePreview {
                worker: Some(_),
                ..
            }))
        ) {
            return Some(PREVIEW_WORKER_POLL);
        }
        let pending = self.pending.as_ref()?;
        Some(PREVIEW_DEBOUNCE.saturating_sub(pending.requested_at.elapsed()))
    }

    fn replace(&mut self, key: PreviewKey, payload: PreviewPayload) {
        self.key = Some(key);
        self.content = Some(match payload {
            PreviewPayload::Text(text) => PreviewContent::Text(text),
            #[cfg(feature = "image")]
            PreviewPayload::Image(image) => PreviewContent::Image(ImagePreview {
                image,
                state: None,
                worker: None,
                area: None,
                error: None,
            }),
        });
        self.scroll = 0;
    }

    fn clear(&mut self) {
        self.key = None;
        self.content = None;
        self.pending = None;
        self.worker = None;
        self.scroll = 0;
    }

    fn render(&mut self) -> Option<PreviewRender<'_>> {
        let Some(content) = self.content.as_mut() else {
            if self.pending.is_some() || self.worker.is_some() {
                return Some(PreviewRender::Text {
                    text: PREVIEW_LOADING,
                    scroll: 0,
                });
            }
            return None;
        };
        match content {
            PreviewContent::Text(text) => Some(PreviewRender::Text {
                text,
                scroll: self.scroll,
            }),
            #[cfg(feature = "image")]
            PreviewContent::Image(image) => {
                if let Some(state) = image.state.as_mut() {
                    Some(PreviewRender::Image {
                        state: state.as_mut(),
                    })
                } else if let Some(error) = image.error.as_deref() {
                    Some(PreviewRender::Text {
                        text: error,
                        scroll: 0,
                    })
                } else {
                    Some(PreviewRender::Text {
                        text: IMAGE_PREVIEW_LOADING,
                        scroll: 0,
                    })
                }
            }
        }
    }

    fn scroll_up(&mut self, rows: usize, visible_rows: usize) {
        self.scroll = self.scroll.saturating_sub(rows);
        self.clamp_scroll(visible_rows);
    }

    fn scroll_down(&mut self, rows: usize, visible_rows: usize) {
        self.scroll = self.scroll.saturating_add(rows);
        self.clamp_scroll(visible_rows);
    }

    fn scroll_top(&mut self) {
        self.scroll = 0;
    }

    fn scroll_bottom(&mut self, visible_rows: usize) {
        self.scroll = self.max_scroll(visible_rows);
    }

    fn clamp_scroll(&mut self, visible_rows: usize) {
        self.scroll = self.scroll.min(self.max_scroll(visible_rows));
    }

    fn max_scroll(&self, visible_rows: usize) -> usize {
        self.line_count().saturating_sub(visible_rows.max(1))
    }

    fn line_count(&self) -> usize {
        match &self.content {
            Some(PreviewContent::Text(text)) => text.lines().count(),
            #[cfg(feature = "image")]
            Some(PreviewContent::Image(image)) => {
                usize::from(image.error.is_some() || image.state.is_none())
            }
            None => 0,
        }
    }

    #[cfg(feature = "image")]
    fn image_picker(&mut self) -> &Picker {
        self.image_picker.get_or_insert_with(image_picker_from_env)
    }

    #[cfg(feature = "image")]
    fn prepare_image(&mut self, width: usize, rows: usize) -> bool {
        let mut changed = self.receive_image_result();
        let area = (width as u16, rows as u16);
        if area.0 == 0 || area.1 == 0 {
            return changed;
        }

        let picker = self.image_picker().clone();
        let Some(PreviewContent::Image(image)) = self.content.as_mut() else {
            return changed;
        };
        if image.error.is_some() {
            return changed;
        }
        if image.state.is_some() && image.area == Some(area) {
            return changed;
        }
        if image
            .worker
            .as_ref()
            .is_some_and(|worker| worker.area == area)
        {
            return changed;
        }

        let source = image.image.clone();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = encode_image_preview(source, picker, area);
            let _ = sender.send(result);
        });
        image.worker = Some(ImageEncodeWorker { area, receiver });
        image.state = None;
        image.area = None;
        image.error = None;
        changed = true;
        changed
    }

    #[cfg(not(feature = "image"))]
    fn prepare_image(&mut self, _width: usize, _rows: usize) -> bool {
        false
    }

    #[cfg(feature = "image")]
    fn receive_image_result(&mut self) -> bool {
        let Some(PreviewContent::Image(image)) = self.content.as_mut() else {
            return false;
        };
        let Some(worker) = &image.worker else {
            return false;
        };
        match worker.receiver.try_recv() {
            Ok(ImageEncodeResult::Ready { area, state }) => {
                if image
                    .worker
                    .as_ref()
                    .is_some_and(|worker| worker.area == area)
                {
                    image.worker = None;
                    image.state = Some(state);
                    image.area = Some(area);
                    image.error = None;
                    return true;
                }
                false
            }
            Ok(ImageEncodeResult::Error { area, message }) => {
                if image
                    .worker
                    .as_ref()
                    .is_some_and(|worker| worker.area == area)
                {
                    image.worker = None;
                    image.state = None;
                    image.area = None;
                    image.error = Some(message);
                    return true;
                }
                false
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                image.worker = None;
                image.error = Some("image preview worker stopped".to_string());
                true
            }
        }
    }
}

enum PreviewRender<'a> {
    Text {
        text: &'a str,
        scroll: usize,
    },
    #[cfg(feature = "image")]
    Image {
        state: &'a mut StatefulProtocol,
    },
}

fn run_preview_command(
    template: &str,
    shell: Option<&str>,
    item: &str,
    geometry: PreviewGeometry,
) -> PreviewPayload {
    #[cfg(feature = "image")]
    if let Some(image) = preview_image_from_path_text(item) {
        return PreviewPayload::Image(image);
    }

    let command = expand_preview_template(template, item);
    let output = preview_shell_command(&command, shell, geometry).output();

    match output {
        Ok(output) => {
            if !output.stdout.is_empty() {
                #[cfg(feature = "image")]
                if let Some(image) = preview_image_from_output(&output.stdout) {
                    return PreviewPayload::Image(image);
                }
                #[cfg(feature = "image")]
                if output.status.success() {
                    if let Some(image) = preview_image_from_path_text(item) {
                        return PreviewPayload::Image(image);
                    }
                }
                let stdout = String::from_utf8_lossy(&output.stdout);
                return PreviewPayload::Text(stdout.into_owned());
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                return PreviewPayload::Text(stderr.into_owned());
            }
            if output.status.success() {
                #[cfg(feature = "image")]
                if let Some(image) = preview_image_from_path_text(item) {
                    return PreviewPayload::Image(image);
                }
                PreviewPayload::Text(String::new())
            } else {
                PreviewPayload::Text(format!("preview exited with {}", output.status))
            }
        }
        Err(error) => PreviewPayload::Text(format!("preview failed: {error}")),
    }
}

#[cfg(feature = "image")]
fn encode_image_preview(
    image: DynamicImage,
    picker: Picker,
    area: (u16, u16),
) -> ImageEncodeResult {
    let mut state = picker.new_resize_protocol(image);
    let resize = Resize::Fit(None);
    let available = Rect::new(0, 0, area.0, area.1);
    let encode_area = state.needs_resize(&resize, available).unwrap_or(available);
    state.resize_encode(&resize, encode_area);
    match state.last_encoding_result() {
        Some(Err(error)) => ImageEncodeResult::Error {
            area,
            message: format!("image preview failed: {error}"),
        },
        _ => ImageEncodeResult::Ready {
            area,
            state: Box::new(state),
        },
    }
}

#[cfg(feature = "image")]
fn preview_image_from_output(bytes: &[u8]) -> Option<DynamicImage> {
    preview_image_from_bytes(bytes, None).or_else(|| {
        std::str::from_utf8(bytes)
            .ok()
            .and_then(preview_image_from_path_text)
    })
}

#[cfg(feature = "image")]
fn preview_image_from_path_text(text: &str) -> Option<DynamicImage> {
    let path = preview_image_path(text)?;
    preview_image_from_path(&path)
}

#[cfg(feature = "image")]
fn preview_image_from_path(path: &Path) -> Option<DynamicImage> {
    let bytes = std::fs::read(path).ok()?;
    preview_image_from_bytes(&bytes, path.parent())
}

#[cfg(feature = "image")]
fn preview_image_from_bytes(bytes: &[u8], resources_dir: Option<&Path>) -> Option<DynamicImage> {
    image::load_from_memory(bytes)
        .ok()
        .or_else(|| preview_svg_from_bytes(bytes, resources_dir))
}

#[cfg(feature = "image")]
fn preview_svg_from_bytes(bytes: &[u8], resources_dir: Option<&Path>) -> Option<DynamicImage> {
    let mut options = resvg::usvg::Options {
        resources_dir: resources_dir.map(Path::to_path_buf),
        ..resvg::usvg::Options::default()
    };
    #[cfg(feature = "image")]
    options.fontdb_mut().load_system_fonts();

    let tree = resvg::usvg::Tree::from_data(bytes, &options).ok()?;
    let size = tree.size();
    let scale = (2048.0 / size.width()).min(2048.0 / size.height()).min(1.0);
    let width = (size.width() * scale).ceil().clamp(1.0, 2048.0) as u32;
    let height = (size.height() * scale).ceil().clamp(1.0, 2048.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    let mut pixmap_mut = pixmap.as_mut();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap_mut,
    );
    image::RgbaImage::from_raw(width, height, pixmap.data().to_vec()).map(DynamicImage::ImageRgba8)
}

#[cfg(feature = "image")]
fn preview_image_path(text: &str) -> Option<PathBuf> {
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(path) = preview_image_path_candidate(line) {
            return Some(path);
        }
        if let Some((left, _)) = line.split_once(':') {
            if let Some(path) = preview_image_path_candidate(left) {
                return Some(path);
            }
        }
        if let Some((_, right)) = line.rsplit_once('|') {
            if let Some(path) = preview_image_path_candidate(right) {
                return Some(path);
            }
            if let Some((left, _)) = right.split_once(':') {
                if let Some(path) = preview_image_path_candidate(left) {
                    return Some(path);
                }
            }
        }
    }
    None
}

#[cfg(feature = "image")]
fn preview_image_path_candidate(raw: &str) -> Option<PathBuf> {
    let raw = raw.trim_matches(['"', '\'']);
    let path = Path::new(raw);
    path.is_file()
        .then(|| path.to_path_buf())
        .filter(|path| is_image_path(path))
}

#[cfg(feature = "image")]
fn is_image_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some(
            "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "bmp"
                | "ico"
                | "tif"
                | "tiff"
                | "webp"
                | "svg"
                | "svgz"
        )
    )
}

#[cfg(feature = "image")]
fn image_picker_from_env() -> Picker {
    let mut picker = image_picker_from_terminal_size().unwrap_or_else(Picker::halfblocks);
    if let Some(protocol) = image_protocol_from_env() {
        picker.set_protocol_type(protocol);
    }
    picker
}

#[cfg(feature = "image")]
fn image_picker_from_terminal_size() -> Option<Picker> {
    let size = terminal::window_size().ok()?;
    if size.columns == 0 || size.rows == 0 || size.width == 0 || size.height == 0 {
        return None;
    }

    let cell_width = (size.width / size.columns).max(1);
    let cell_height = (size.height / size.rows).max(1);
    #[allow(deprecated)]
    Some(Picker::from_fontsize((cell_width, cell_height)))
}

#[cfg(feature = "image")]
fn image_protocol_from_env() -> Option<ProtocolType> {
    if let Ok(protocol) = std::env::var("YURU_PREVIEW_IMAGE_PROTOCOL") {
        return match protocol.to_ascii_lowercase().as_str() {
            "halfblocks" | "halfblock" | "unicode" => Some(ProtocolType::Halfblocks),
            "sixel" => Some(ProtocolType::Sixel),
            "kitty" => Some(ProtocolType::Kitty),
            "iterm2" | "iterm" => Some(ProtocolType::Iterm2),
            _ => None,
        };
    }
    if std::env::var("KITTY_WINDOW_ID").is_ok_and(|value| !value.is_empty())
        || std::env::var("TERM_PROGRAM").is_ok_and(|value| value.eq_ignore_ascii_case("ghostty"))
        || std::env::var("GHOSTTY_RESOURCES_DIR").is_ok_and(|value| !value.is_empty())
        || std::env::var("GHOSTTY_BIN_DIR").is_ok_and(|value| !value.is_empty())
    {
        return Some(ProtocolType::Kitty);
    }
    if std::env::var("TERM_PROGRAM").is_ok_and(|value| {
        value.contains("iTerm") || value.contains("WezTerm") || value.contains("rio")
    }) {
        return Some(ProtocolType::Iterm2);
    }
    if std::env::var("TERM").is_ok_and(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("sixel") || value.contains("mlterm")
    }) {
        return Some(ProtocolType::Sixel);
    }
    None
}

fn expand_preview_template(template: &str, item: &str) -> String {
    if template.contains("{}") {
        template.replace("{}", &shell_quote(item))
    } else {
        template.to_string()
    }
}

#[cfg(not(windows))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(windows)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(not(windows))]
fn preview_shell_command(command: &str, shell: Option<&str>, geometry: PreviewGeometry) -> Command {
    let shell = shell
        .map(str::to_string)
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(|| "sh".to_string());
    let mut parts = shell.split_whitespace();
    let program = parts.next().unwrap_or("sh");
    let mut process = Command::new(program);
    let shell_args: Vec<_> = parts.collect();
    if shell_args.is_empty() {
        process.arg("-c");
    } else {
        process.args(shell_args);
    }
    process.arg(command);
    apply_preview_env(&mut process, geometry);
    process
}

#[cfg(windows)]
fn preview_shell_command(command: &str, shell: Option<&str>, geometry: PreviewGeometry) -> Command {
    let shell = shell
        .map(str::to_string)
        .or_else(|| std::env::var("YURU_WINDOWS_SHELL").ok())
        .unwrap_or_else(|| "powershell.exe".to_string());
    let mut parts = shell.split_whitespace();
    let program = parts.next().unwrap_or("powershell.exe");
    let mut process = Command::new(program);
    let shell_args: Vec<_> = parts.collect();
    if shell_args.is_empty() {
        process.args(["-NoLogo", "-NoProfile", "-Command"]);
    } else {
        process.args(shell_args);
    }
    process.arg(command);
    apply_preview_env(&mut process, geometry);
    process
}

fn apply_preview_env(process: &mut Command, geometry: PreviewGeometry) {
    process
        .env("FZF_PREVIEW_COLUMNS", geometry.columns.to_string())
        .env("FZF_PREVIEW_LINES", geometry.lines.to_string())
        .env("FZF_PREVIEW_LEFT", geometry.left.to_string())
        .env("FZF_PREVIEW_TOP", geometry.top.to_string());
}

fn classify_key(
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
enum KeyDecision {
    Accept(Option<String>),
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
    mut context: RenderContext<'_>,
) -> Result<()> {
    queue!(output, MoveTo(0, 0), Clear(ClearType::All))?;
    let prompt_row = if context.no_input {
        None
    } else if context.layout.prompt_at_bottom() {
        Some(context.viewport.rows)
    } else {
        Some(0)
    };
    if let Some(prompt_row) = prompt_row {
        let input = if state.query().is_empty() {
            format!("{}{}", context.prompt, "")
        } else {
            format!("{}{}", context.prompt, state.query())
        };
        queue!(
            output,
            MoveTo(0, prompt_row as u16),
            Print(truncate_to_width_with_ellipsis(
                &input,
                context.viewport.width,
                context.ellipsis
            ))
        )?;
    }

    let header_rows = visible_line_count(context.header);
    if let Some(header) = context.header {
        let header_start = if context.layout.prompt_at_bottom() {
            0
        } else {
            1
        };
        for (row, line) in header.lines().enumerate() {
            queue!(
                output,
                MoveTo(0, (header_start + row) as u16),
                Print(truncate_to_width_with_ellipsis(
                    line,
                    context.viewport.width,
                    context.ellipsis
                ))
            )?;
        }
    }
    let footer_rows = visible_line_count(context.footer);
    if let Some(footer) = context.footer {
        let footer_start = footer_start_row(context.layout, context.viewport.rows, footer_rows);
        for (row, line) in footer.lines().enumerate() {
            queue!(
                output,
                MoveTo(0, (footer_start + row) as u16),
                Print(truncate_to_width_with_ellipsis(
                    line,
                    context.viewport.width,
                    context.ellipsis
                ))
            )?;
        }
    }

    let preview_width = preview_width(context.viewport.width, context.preview.as_ref());
    let list_width = if preview_width == 0 {
        context.viewport.width
    } else {
        context
            .viewport
            .width
            .saturating_sub(preview_width)
            .saturating_sub(1)
            .max(1)
    };
    let result_rows = context
        .viewport
        .rows
        .saturating_sub(header_rows + footer_rows)
        .max(1);
    let result_bottom = context.viewport.rows.saturating_sub(footer_rows + 1);
    let offset = scroll_offset(state.selected(), results.len(), result_rows);
    for (row, result) in results.iter().skip(offset).take(result_rows).enumerate() {
        let result_row = if context.layout.list_bottom_up() {
            result_bottom.saturating_sub(row)
        } else if context.layout.prompt_at_bottom() {
            row + header_rows
        } else {
            row + 1 + header_rows
        };
        queue!(output, MoveTo(0, result_row as u16))?;
        let mark = if context.multi && state.marked.contains(&result.id) {
            context.marker
        } else {
            " "
        };
        let selected = offset + row == state.selected();
        if selected {
            queue!(output, SetAttribute(Attribute::Reverse))?;
            if let Some(color) = context.style.pointer_color() {
                queue!(output, SetForegroundColor(color))?;
            }
            queue!(output, Print(context.pointer), Print(mark))?;
            queue!(output, ResetColor)?;
        } else {
            queue!(output, Print(" "), Print(mark))?;
        }
        render_highlighted_result(
            output,
            state.query(),
            result,
            &context,
            list_width
                .saturating_sub(context.pointer.chars().count() + context.marker.chars().count()),
            selected,
        )?;
        queue!(output, SetAttribute(Attribute::Reset))?;
    }

    render_preview(output, &mut context, preview_width)?;

    if let Some(prompt_row) = prompt_row {
        let cursor_column =
            context.prompt.chars().count() + state.query()[..state.cursor()].chars().count();
        queue!(
            output,
            MoveTo(
                cursor_column.min(context.viewport.width - 1) as u16,
                prompt_row as u16
            )
        )?;
    }
    output.flush()?;
    Ok(())
}

fn visible_line_count(text: Option<&str>) -> usize {
    text.map(|text| text.lines().count()).unwrap_or(0)
}

fn footer_start_row(layout: TuiLayout, viewport_rows: usize, footer_rows: usize) -> usize {
    if footer_rows == 0 {
        return 0;
    }

    if layout.prompt_at_bottom() {
        viewport_rows.saturating_sub(footer_rows)
    } else {
        viewport_rows.saturating_add(1).saturating_sub(footer_rows)
    }
}

struct RenderContext<'a> {
    candidates: &'a [Candidate],
    prompt: &'a str,
    header: Option<&'a str>,
    footer: Option<&'a str>,
    viewport: Viewport,
    layout: TuiLayout,
    preview: Option<PreviewRender<'a>>,
    style: &'a TuiStyle,
    case_sensitive: bool,
    multi: bool,
    no_input: bool,
    pointer: &'a str,
    marker: &'a str,
    ellipsis: &'a str,
}

fn preview_width(total_width: usize, preview: Option<&PreviewRender<'_>>) -> usize {
    preview_width_for_presence(total_width, preview.is_some())
}

fn preview_width_for_presence(total_width: usize, has_preview: bool) -> usize {
    if !has_preview || total_width < 30 {
        0
    } else {
        (total_width / 2).clamp(12, total_width.saturating_sub(12))
    }
}

fn preview_geometry(
    viewport: Viewport,
    layout: TuiLayout,
    has_preview: bool,
) -> Option<PreviewGeometry> {
    let columns = preview_width_for_presence(viewport.width, has_preview);
    if columns == 0 {
        return None;
    }
    let left = viewport.width.saturating_sub(columns);
    let top = if layout.prompt_at_bottom() { 0 } else { 1 };
    Some(PreviewGeometry {
        columns,
        lines: viewport.rows,
        left,
        top,
    })
}

fn render_preview(
    output: &mut impl Write,
    context: &mut RenderContext<'_>,
    preview_width: usize,
) -> Result<()> {
    if context.preview.is_none() {
        return Ok(());
    }
    if preview_width == 0 {
        return Ok(());
    }

    let x = context.viewport.width.saturating_sub(preview_width);
    let start_row = if context.layout.prompt_at_bottom() {
        0
    } else {
        1
    };
    let max_rows = context.viewport.rows;

    for row in 0..max_rows {
        queue!(
            output,
            MoveTo(x.saturating_sub(1) as u16, (start_row + row) as u16),
            Print("|")
        )?;
    }

    match context.preview.as_mut().expect("preview exists") {
        PreviewRender::Text { text, scroll } => {
            for (row, line) in text.lines().skip(*scroll).take(max_rows).enumerate() {
                queue!(
                    output,
                    MoveTo(x as u16, (start_row + row) as u16),
                    Print(truncate_to_width_with_ellipsis(
                        line,
                        preview_width,
                        context.ellipsis
                    ))
                )?;
            }
        }
        #[cfg(feature = "image")]
        PreviewRender::Image { state } => {
            render_image_preview(output, x, start_row, preview_width, max_rows, state)?;
        }
    }

    Ok(())
}

#[cfg(feature = "image")]
fn render_image_preview(
    output: &mut impl Write,
    x: usize,
    y: usize,
    width: usize,
    rows: usize,
    state: &mut StatefulProtocol,
) -> Result<()> {
    let area = Rect::new(0, 0, width as u16, rows as u16);
    let mut buffer = Buffer::empty(area);
    state.render(area, &mut buffer);
    for row in 0..rows {
        for col in 0..width {
            let Some(cell) = buffer.cell((col as u16, row as u16)) else {
                continue;
            };
            if cell.skip || cell.symbol().is_empty() {
                continue;
            }
            queue!(
                output,
                MoveTo((x + col) as u16, (y + row) as u16),
                Print(cell.symbol())
            )?;
        }
    }
    Ok(())
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

fn scroll_offset(selected: usize, len: usize, rows: usize) -> usize {
    if len == 0 || selected < rows {
        0
    } else {
        selected + 1 - rows
    }
}

fn truncate_to_width_with_ellipsis(text: &str, width: usize, ellipsis: &str) -> String {
    let char_count = text.chars().count();
    if char_count <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }

    let ellipsis_width = ellipsis.chars().count().min(width);
    let mut out: String = text
        .chars()
        .take(width.saturating_sub(ellipsis_width))
        .collect();
    out.extend(ellipsis.chars().take(ellipsis_width));
    out
}

fn render_highlighted_result(
    output: &mut impl Write,
    query: &str,
    result: &ScoredCandidate,
    context: &RenderContext<'_>,
    width: usize,
    selected: bool,
) -> Result<()> {
    for segment in highlight_segments_for_result(
        query,
        result,
        context.candidates,
        context.case_sensitive,
        width,
    ) {
        if segment.highlighted {
            queue!(
                output,
                SetForegroundColor(context.style.highlight_color(selected)),
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
    use yuru_core::SourceSpan;

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

    #[test]
    fn search_worker_searches_owned_streamed_candidates() {
        let backend: Arc<dyn LanguageBackend> = Arc::new(yuru_core::PlainBackend);
        let config = SearchConfig::default();
        let candidate = yuru_core::build_candidate(0, "alpha.txt", backend.as_ref(), &config);
        let mut worker = SearchWorker::new(backend);

        worker.append(vec![candidate]);
        worker.request_owned(1, "alp".to_string(), config);

        let response = wait_for_search_response(&mut worker);
        assert_eq!(response.seq, 1);
        assert_eq!(response.query, "alp");
        assert_eq!(
            response
                .results
                .first()
                .map(|result| result.display.as_str()),
            Some("alpha.txt")
        );
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

    #[test]
    fn preview_scroll_clamps_to_visible_lines() {
        let mut cache = PreviewCache::default();
        cache.replace(
            preview_key("cat {}", 0, "alpha"),
            PreviewPayload::Text("one\ntwo\nthree".to_string()),
        );

        cache.scroll_down(10, 2);
        assert_eq!(cache.scroll, 1);

        cache.scroll_up(10, 2);
        assert_eq!(cache.scroll, 0);
    }

    #[test]
    fn preview_scroll_resets_when_preview_key_changes() {
        let mut cache = PreviewCache::default();
        cache.replace(
            preview_key("cat {}", 0, "alpha"),
            PreviewPayload::Text("one\ntwo\nthree".to_string()),
        );
        cache.scroll_down(1, 2);

        cache.replace(
            preview_key("cat {}", 1, "beta"),
            PreviewPayload::Text("four\nfive\nsix".to_string()),
        );

        assert_eq!(cache.scroll, 0);
    }

    #[test]
    fn render_default_layout_places_prompt_at_bottom() {
        let mut output = Vec::new();
        let state = TuiState::new("al");
        let results = vec![scored("alpha", KeyKind::Original)];
        render(
            &mut output,
            &state,
            &results,
            RenderContext {
                candidates: &[],
                prompt: "> ",
                header: None,
                footer: None,
                viewport: Viewport { width: 40, rows: 3 },
                layout: TuiLayout::Default,
                preview: None,
                style: &TuiStyle::default(),
                case_sensitive: false,
                multi: false,
                no_input: false,
                pointer: ">",
                marker: "*",
                ellipsis: "..",
            },
        )
        .unwrap();

        let rendered = String::from_utf8(output).unwrap();
        assert!(rendered.contains("\u{1b}[4;1H> al"), "{rendered:?}");
        assert!(rendered.contains("\u{1b}[3;1H\u{1b}[7m> "), "{rendered:?}");
    }

    #[test]
    fn render_default_layout_paints_results_bottom_up() {
        let mut output = Vec::new();
        let state = TuiState::new("");
        let results = vec![
            scored("alpha", KeyKind::Original),
            scored_with_id(1, "beta", KeyKind::Original),
            scored_with_id(2, "gamma", KeyKind::Original),
        ];
        render(
            &mut output,
            &state,
            &results,
            RenderContext {
                candidates: &[],
                prompt: "> ",
                header: None,
                footer: None,
                viewport: Viewport { width: 40, rows: 3 },
                layout: TuiLayout::Default,
                preview: None,
                style: &TuiStyle::default(),
                case_sensitive: false,
                multi: false,
                no_input: false,
                pointer: ">",
                marker: "*",
                ellipsis: "..",
            },
        )
        .unwrap();

        let rendered = String::from_utf8(output).unwrap();
        assert!(
            rendered.contains("\u{1b}[3;1H\u{1b}[7m> \u{1b}[0malpha"),
            "{rendered:?}"
        );
        assert!(rendered.contains("\u{1b}[2;1H  beta"), "{rendered:?}");
        assert!(rendered.contains("\u{1b}[1;1H  gamma"), "{rendered:?}");
    }

    #[test]
    fn render_reverse_layout_places_prompt_at_top() {
        let mut output = Vec::new();
        let state = TuiState::new("al");
        let results = vec![scored("alpha", KeyKind::Original)];
        render(
            &mut output,
            &state,
            &results,
            RenderContext {
                candidates: &[],
                prompt: "> ",
                header: None,
                footer: None,
                viewport: Viewport { width: 40, rows: 3 },
                layout: TuiLayout::Reverse,
                preview: None,
                style: &TuiStyle::default(),
                case_sensitive: false,
                multi: false,
                no_input: false,
                pointer: ">",
                marker: "*",
                ellipsis: "..",
            },
        )
        .unwrap();

        let rendered = String::from_utf8(output).unwrap();
        assert!(rendered.contains("\u{1b}[1;1H> al"), "{rendered:?}");
    }

    #[test]
    fn render_preview_pane_prints_preview_text() {
        let mut output = Vec::new();
        let state = TuiState::new("");
        let results = vec![scored("alpha", KeyKind::Original)];
        render(
            &mut output,
            &state,
            &results,
            RenderContext {
                candidates: &[],
                prompt: "> ",
                header: None,
                footer: None,
                viewport: Viewport { width: 80, rows: 3 },
                layout: TuiLayout::Default,
                preview: Some(PreviewRender::Text {
                    text: "preview alpha\nsecond line",
                    scroll: 0,
                }),
                style: &TuiStyle::default(),
                case_sensitive: false,
                multi: false,
                no_input: false,
                pointer: ">",
                marker: "*",
                ellipsis: "..",
            },
        )
        .unwrap();

        let rendered = String::from_utf8(output).unwrap();
        assert!(
            rendered.contains("\u{1b}[1;41Hpreview alpha"),
            "{rendered:?}"
        );
        assert!(rendered.contains("\u{1b}[2;41Hsecond line"), "{rendered:?}");
    }

    #[test]
    fn render_preview_pane_uses_scroll_offset() {
        let mut output = Vec::new();
        let state = TuiState::new("");
        let results = vec![scored("alpha", KeyKind::Original)];
        render(
            &mut output,
            &state,
            &results,
            RenderContext {
                candidates: &[],
                prompt: "> ",
                header: None,
                footer: None,
                viewport: Viewport { width: 80, rows: 2 },
                layout: TuiLayout::Default,
                preview: Some(PreviewRender::Text {
                    text: "first\nsecond\nthird",
                    scroll: 1,
                }),
                style: &TuiStyle::default(),
                case_sensitive: false,
                multi: false,
                no_input: false,
                pointer: ">",
                marker: "*",
                ellipsis: "..",
            },
        )
        .unwrap();

        let rendered = String::from_utf8(output).unwrap();
        assert!(!rendered.contains("first"), "{rendered:?}");
        assert!(rendered.contains("\u{1b}[1;41Hsecond"), "{rendered:?}");
        assert!(rendered.contains("\u{1b}[2;41Hthird"), "{rendered:?}");
    }

    #[cfg(unix)]
    #[test]
    fn preview_command_returns_stderr_when_stdout_is_empty() {
        let preview =
            run_preview_command("printf preview-error >&2", None, "alpha", test_geometry());

        assert!(matches!(preview, PreviewPayload::Text(text) if text == "preview-error"));
    }

    #[cfg(unix)]
    #[test]
    fn preview_command_gets_fzf_preview_geometry_env() {
        let preview = run_preview_command(
            "printf '%s,%s,%s,%s' \"$FZF_PREVIEW_COLUMNS\" \"$FZF_PREVIEW_LINES\" \"$FZF_PREVIEW_LEFT\" \"$FZF_PREVIEW_TOP\"",
            None,
            "alpha",
            PreviewGeometry {
                columns: 40,
                lines: 12,
                left: 41,
                top: 1,
            },
        );

        assert!(matches!(preview, PreviewPayload::Text(text) if text == "40,12,41,1"));
    }

    #[cfg(feature = "image")]
    #[test]
    fn preview_output_detects_inline_image_bytes() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1,
            1,
            image::Rgba([255, 0, 0, 255]),
        ));
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();

        assert!(preview_image_from_output(&bytes).is_some());
    }

    #[cfg(feature = "image")]
    #[test]
    fn preview_output_detects_inline_svg_bytes() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="8"><rect width="10" height="8" fill="#ff0000"/></svg>"##;
        let image = preview_image_from_output(svg).expect("svg should rasterize");

        assert_eq!(image.width(), 10);
        assert_eq!(image.height(), 8);
    }

    #[cfg(all(feature = "image", unix))]
    #[test]
    fn preview_command_prefers_selected_image_path_over_text_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ame.png");
        std::fs::write(&path, tiny_png_bytes()).unwrap();

        let preview = run_preview_command(
            "printf 'text preview'",
            None,
            path.to_str().unwrap(),
            test_geometry(),
        );

        assert!(matches!(preview, PreviewPayload::Image(_)));
    }

    #[cfg(feature = "image")]
    #[test]
    fn preview_output_detects_file_command_image_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ame image.png");
        std::fs::write(&path, tiny_png_bytes()).unwrap();
        let output = format!("{}: PNG image data, 1 x 1", path.display());

        assert!(preview_image_from_output(output.as_bytes()).is_some());
    }

    #[cfg(feature = "image")]
    #[test]
    fn image_preview_encoding_happens_before_render() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([255, 0, 0, 255]),
        ));
        let picker = Picker::halfblocks();
        let result = encode_image_preview(image, picker, (4, 2));

        assert!(matches!(result, ImageEncodeResult::Ready { .. }));
    }

    #[cfg(feature = "image")]
    #[test]
    fn encoded_image_preview_renders_terminal_output() {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([255, 0, 0, 255]),
        ));
        let picker = Picker::halfblocks();
        let ImageEncodeResult::Ready { mut state, .. } =
            encode_image_preview(image, picker, (4, 2))
        else {
            panic!("image preview should encode");
        };
        let mut output = Vec::new();

        render_image_preview(&mut output, 10, 0, 4, 2, state.as_mut()).unwrap();

        assert!(!output.is_empty());
    }

    #[cfg(feature = "image")]
    #[test]
    fn image_encode_worker_keeps_preview_polling() {
        let mut cache = PreviewCache::default();
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([255, 0, 0, 255]),
        ));
        cache.replace(
            preview_key("file {}", 0, "ame.png"),
            PreviewPayload::Image(image),
        );
        let (_sender, receiver) = mpsc::channel();
        if let Some(PreviewContent::Image(image)) = cache.content.as_mut() {
            image.worker = Some(ImageEncodeWorker {
                area: (4, 2),
                receiver,
            });
        }

        assert_eq!(cache.next_poll_timeout(), Some(PREVIEW_WORKER_POLL));
    }

    #[cfg(feature = "image")]
    #[test]
    fn ghostty_tmux_env_prefers_kitty_protocol() {
        let _guard = EnvGuard::set("GHOSTTY_RESOURCES_DIR", "/Applications/Ghostty.app");
        let _term_program = EnvGuard::set("TERM_PROGRAM", "tmux");
        let _protocol = EnvGuard::unset("YURU_PREVIEW_IMAGE_PROTOCOL");

        assert_eq!(image_protocol_from_env(), Some(ProtocolType::Kitty));
    }

    fn preview_key(command: &str, id: usize, item: &str) -> PreviewKey {
        (
            command.to_string(),
            String::new(),
            id,
            item.to_string(),
            test_geometry(),
        )
    }

    fn test_geometry() -> PreviewGeometry {
        PreviewGeometry {
            columns: 40,
            lines: 10,
            left: 40,
            top: 0,
        }
    }

    #[cfg(feature = "image")]
    fn tiny_png_bytes() -> Vec<u8> {
        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1,
            1,
            image::Rgba([255, 0, 0, 255]),
        ));
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[cfg(feature = "image")]
    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    #[cfg(feature = "image")]
    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }

        fn unset(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, previous }
        }
    }

    #[cfg(feature = "image")]
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
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
    fn plain_mode_highlight_marks_direct_matches_from_normalized_key() {
        let result = scored("README.md", KeyKind::Normalized);
        let segments = highlight_segments_for_result("read", &result, &[], false, 80);

        assert_eq!(
            segments,
            vec![
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

    fn wait_for_search_response(worker: &mut SearchWorker) -> SearchResponse {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(response) = worker.try_recv() {
                return response;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for search worker"
            );
            thread::sleep(Duration::from_millis(5));
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
