use std::io;
use std::sync::{
    mpsc::{Receiver, TryRecvError},
    Arc,
};
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    cursor::Hide,
    event::{self, Event, KeyEvent, KeyEventKind},
    execute,
    terminal::{enable_raw_mode, EnterAlternateScreen},
};
use yuru_core::{Candidate, LanguageBackend, ScoredCandidate, SearchConfig};

use crate::actions::apply_interactive_action;
use crate::api::{CandidateStreamMessage, TuiOptions, TuiOutcome};
use crate::keys::{classify_key, KeyDecision};
use crate::preview::PreviewCache;
use crate::render::{preview_geometry, render, RenderContext, Viewport};
use crate::search_worker::{
    request_owned_search, request_snapshot_search, SearchIdentity, SearchWorker, SEARCH_WORKER_POLL,
};
use crate::state::{SelectionTarget, TuiState};
use crate::terminal::TerminalGuard;
use crate::TuiAction;

const STREAM_DRAIN_BATCH: usize = 2048;

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

pub(crate) fn is_actionable_key_event(key: &KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

/// Meaning a terminal event has for the interaction loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalEvent {
    /// An actionable key press or repeat.
    Key(KeyEvent),
    /// An event that invalidates the painted frame, such as a terminal resize.
    Redraw,
    /// An event the interface does not react to.
    Ignore,
}

/// Maps a terminal event to its interaction-loop meaning.
///
/// A resize invalidates the painted frame, so it has to repaint even though it carries no
/// action; the viewport is recomputed on every loop iteration.
pub(crate) fn classify_terminal_event(event: &Event) -> TerminalEvent {
    match event {
        Event::Key(key) if is_actionable_key_event(key) => TerminalEvent::Key(*key),
        Event::Resize(..) => TerminalEvent::Redraw,
        _ => TerminalEvent::Ignore,
    }
}

fn read_terminal_event() -> Result<TerminalEvent> {
    Ok(classify_terminal_event(&event::read()?))
}

/// Blocks until an event the interaction loop reacts to arrives.
fn read_reactive_terminal_event() -> Result<TerminalEvent> {
    loop {
        match read_terminal_event()? {
            TerminalEvent::Ignore => {}
            event => return Ok(event),
        }
    }
}

/// Resolves the case sensitivity to use for `query`.
///
/// With smart case active the mode follows the live query, matching fzf. The explicit
/// `--ignore-case` / `--no-ignore-case` overrides are baked into `config` instead and
/// never change while the user types.
pub(crate) fn resolve_case_sensitive(config: &SearchConfig, smart_case: bool, query: &str) -> bool {
    if smart_case {
        query.chars().any(char::is_uppercase)
    } else {
        config.case_sensitive
    }
}

/// Returns the search config for `query` with live smart case applied.
pub(crate) fn search_config_for_query(
    config: &SearchConfig,
    smart_case: bool,
    query: &str,
) -> SearchConfig {
    SearchConfig {
        case_sensitive: resolve_case_sensitive(config, smart_case, query),
        ..config.clone()
    }
}

/// Returns the identity of the search the live query asks for.
///
/// This is what an applied result set is compared against: it has to agree on both the
/// query text and the case policy that text resolves to, because smart case makes the
/// policy follow the query.
pub(crate) fn live_search_identity(
    config: &SearchConfig,
    smart_case: bool,
    query: &str,
) -> SearchIdentity {
    SearchIdentity {
        query: query.to_string(),
        case_sensitive: resolve_case_sensitive(config, smart_case, query),
    }
}

/// A result set together with the search it answers.
///
/// Rows whose identity is not the live one are stale: they are still painted, because
/// blanking the list on every keystroke of a slow search would be worse, but they are
/// never turned into an outcome.
#[derive(Default)]
pub(crate) struct ResultSet {
    pub(crate) identity: Option<SearchIdentity>,
    pub(crate) rows: Vec<ScoredCandidate>,
}

impl ResultSet {
    pub(crate) fn rows(&self) -> &[ScoredCandidate] {
        &self.rows
    }

    /// Returns whether these rows answer `live`.
    pub(crate) fn is_current(&self, live: &SearchIdentity) -> bool {
        self.identity.as_ref() == Some(live)
    }
}

/// An accept that has been committed but whose result set has not landed yet.
///
/// It carries the selection as it was when the key was pressed. Resolving it against the
/// live state instead would hand the user whatever row moved into that place.
pub(crate) struct PendingAccept {
    pub(crate) target: SelectionTarget,
    pub(crate) expect: Option<String>,
}

impl PendingAccept {
    /// Captures the accept the user just made.
    pub(crate) fn capture(state: &TuiState, expect: Option<String>) -> Self {
        Self {
            target: state.target(),
            expect,
        }
    }
}

/// Turns a selection into an outcome by resolving it against `rows`.
pub(crate) fn accept_outcome(
    state: &TuiState,
    target: SelectionTarget,
    rows: &[ScoredCandidate],
    multi: bool,
    expect: Option<String>,
) -> TuiOutcome {
    let ids = state.accepted_ids(target, rows, multi);
    if ids.is_empty() {
        return TuiOutcome::NoSelection;
    }
    TuiOutcome::Accepted {
        ids,
        query: state.query().to_string(),
        expect,
    }
}

/// Runs the TUI over a fixed candidate slice.
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
    let mut results = ResultSet::default();
    let mut pending_accept: Option<PendingAccept> = None;
    let mut render_needed = true;
    request_snapshot_search(
        &mut search_worker,
        &mut search_seq,
        &mut latest_requested_seq,
        state.query(),
        candidates.clone(),
        search_config_for_query(&config, options.smart_case, state.query()),
    );

    loop {
        let live = live_search_identity(&config, options.smart_case, state.query());
        while let Some(response) = search_worker.try_recv() {
            if response.seq >= latest_applied_seq && response.identity == live {
                latest_applied_seq = response.seq;
                results = ResultSet {
                    identity: Some(response.identity),
                    rows: response.results,
                };
                // Rows were replaced wholesale, so the cursor is re-resolved from what
                // it is pointing at rather than left on a position that now means a
                // different candidate.
                state.reselect(results.rows());
                render_needed = true;
            }
        }

        // An accept pressed while the rows belonged to an earlier search finishes here,
        // once the search it was meant for has landed.
        if results.is_current(&live) {
            if let Some(accept) = pending_accept.take() {
                return Ok(accept_outcome(
                    &state,
                    accept.target,
                    results.rows(),
                    options.multi,
                    accept.expect,
                ));
            }
        }

        let has_prompt = !options.no_input;
        let viewport = Viewport::from_terminal(options.height, has_prompt);
        let preview_geometry = preview_geometry(
            viewport,
            options.layout,
            has_prompt,
            options.preview.is_some() && !results.rows().is_empty(),
        );
        preview_cache.request_for_selection(
            options.preview.as_ref(),
            options.preview_shell.as_deref(),
            results.rows(),
            &state,
            preview_geometry,
            options.preview_image_protocol,
        );
        render_needed |= preview_cache.poll();
        preview_cache.clamp_scroll(viewport.rows);
        render_needed |= preview_cache.prepare_image(
            options.preview_image_protocol,
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
                highlight_line: options.highlight_line,
                // Highlighting marks the live query text, so it uses that text's own case
                // policy: a stale row that the live query no longer matches then paints
                // without highlights instead of claiming a match it does not have.
                case_sensitive: live.case_sensitive,
                multi: options.multi,
                no_input: options.no_input,
                pointer: &options.pointer,
                marker: &options.marker,
                ellipsis: &options.ellipsis,
                ansi: options.ansi,
            };
            render(&mut output, &state, results.rows(), render_context)?;
            render_needed = false;
        }

        let poll_timeout = interaction_poll_timeout(
            preview_cache.next_poll_timeout(),
            (pending_accept.is_some() || latest_applied_seq < latest_requested_seq)
                .then_some(SEARCH_WORKER_POLL),
            None,
        );
        let terminal_event = if let Some(timeout) = poll_timeout {
            if !event::poll(timeout)? {
                continue;
            }
            read_terminal_event()?
        } else {
            read_reactive_terminal_event()?
        };
        let key = match terminal_event {
            TerminalEvent::Key(key) => key,
            TerminalEvent::Redraw => {
                render_needed = true;
                continue;
            }
            TerminalEvent::Ignore => continue,
        };

        let decision = classify_key(key, viewport.rows, &options.expect_keys, &options.bindings);
        if pending_accept.is_some() {
            // The accept is already committed and only waits for its result set. Abort
            // still applies; anything else would retarget a decision the user has made.
            if matches!(decision, KeyDecision::Abort) {
                return Ok(TuiOutcome::Aborted);
            }
            continue;
        }
        match decision {
            KeyDecision::Accept(expect) => {
                if results.is_current(&live) {
                    return Ok(accept_outcome(
                        &state,
                        state.target(),
                        results.rows(),
                        options.multi,
                        expect,
                    ));
                }
                // The rows on screen answer an earlier query or case policy, so they may
                // not match what is typed. Hold the accept until the live search lands,
                // aimed at the candidate that was selected now.
                pending_accept = Some(PendingAccept::capture(&state, expect));
            }
            KeyDecision::Abort => return Ok(TuiOutcome::Aborted),
            KeyDecision::Action(action) => {
                let old_query = state.query().to_string();
                apply_interactive_action(
                    action,
                    &mut state,
                    &mut preview_cache,
                    results.rows(),
                    &options,
                    viewport.rows,
                );
                if matches!(action, TuiAction::DeleteOrExit) && old_query.is_empty() {
                    return Ok(TuiOutcome::NoSelection);
                }
                if state.query() != old_query {
                    request_snapshot_search(
                        &mut search_worker,
                        &mut search_seq,
                        &mut latest_requested_seq,
                        state.query(),
                        candidates.clone(),
                        search_config_for_query(&config, options.smart_case, state.query()),
                    );
                }
                render_needed = true;
            }
            KeyDecision::Ignore => {}
        }
    }
}

/// Runs the TUI while candidates are received from a stream.
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
    let mut results = ResultSet::default();
    let mut pending_accept: Option<PendingAccept> = None;
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
                search_config_for_query(&config, options.smart_case, state.query()),
            );
            dirty = false;
        }

        let live = live_search_identity(&config, options.smart_case, state.query());
        while let Some(response) = search_worker.try_recv() {
            if response.seq >= latest_applied_seq && response.identity == live {
                latest_applied_seq = response.seq;
                results = ResultSet {
                    identity: Some(response.identity),
                    rows: response.results,
                };
                // Rows were replaced wholesale, so the cursor is re-resolved from what
                // it is pointing at rather than left on a position that now means a
                // different candidate.
                state.reselect(results.rows());
                render_needed = true;
            }
        }

        // An accept pressed while the rows belonged to an earlier search finishes here,
        // once the search it was meant for has landed.
        if results.is_current(&live) {
            if let Some(accept) = pending_accept.take() {
                return Ok(accept_outcome(
                    &state,
                    accept.target,
                    results.rows(),
                    options.multi,
                    accept.expect,
                ));
            }
        }

        let has_prompt = !options.no_input;
        let viewport = Viewport::from_terminal(options.height, has_prompt);
        let preview_geometry = preview_geometry(
            viewport,
            options.layout,
            has_prompt,
            options.preview.is_some() && !results.rows().is_empty(),
        );
        preview_cache.request_for_selection(
            options.preview.as_ref(),
            options.preview_shell.as_deref(),
            results.rows(),
            &state,
            preview_geometry,
            options.preview_image_protocol,
        );
        let preview_changed = preview_cache.poll();
        render_needed |= preview_changed;
        render_needed |= preview_cache.prepare_image(
            options.preview_image_protocol,
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
                highlight_line: options.highlight_line,
                // See the note in `run_interactive`: highlighting marks the live query
                // text, so it uses that text's own case policy.
                case_sensitive: live.case_sensitive,
                multi: options.multi,
                no_input: options.no_input,
                pointer: &options.pointer,
                marker: &options.marker,
                ellipsis: &options.ellipsis,
                ansi: options.ansi,
            };
            render(&mut output, &state, results.rows(), render_context)?;
            render_needed = false;
        }

        let source_poll_interval = if reading {
            Duration::from_millis(25)
        } else {
            Duration::from_millis(250)
        };
        let poll_interval = interaction_poll_timeout(
            preview_cache.next_poll_timeout(),
            (pending_accept.is_some() || latest_applied_seq < latest_requested_seq)
                .then_some(SEARCH_WORKER_POLL),
            Some(source_poll_interval),
        )
        .unwrap_or(source_poll_interval);
        if !event::poll(poll_interval)? {
            continue;
        }

        let key = match read_terminal_event()? {
            TerminalEvent::Key(key) => key,
            TerminalEvent::Redraw => {
                render_needed = true;
                continue;
            }
            TerminalEvent::Ignore => continue,
        };
        let viewport = Viewport::from_terminal(options.height, !options.no_input);
        let decision = classify_key(key, viewport.rows, &options.expect_keys, &options.bindings);
        if pending_accept.is_some() {
            // The accept is already committed and only waits for its result set. Abort
            // still applies; anything else would retarget a decision the user has made.
            if matches!(decision, KeyDecision::Abort) {
                return Ok(TuiOutcome::Aborted);
            }
            continue;
        }
        match decision {
            KeyDecision::Accept(expect) => {
                if results.is_current(&live) {
                    return Ok(accept_outcome(
                        &state,
                        state.target(),
                        results.rows(),
                        options.multi,
                        expect,
                    ));
                }
                // The rows on screen answer an earlier query or case policy, so they may
                // not match what is typed. Hold the accept until the live search lands,
                // aimed at the candidate that was selected now.
                pending_accept = Some(PendingAccept::capture(&state, expect));
            }
            KeyDecision::Abort => return Ok(TuiOutcome::Aborted),
            KeyDecision::Action(action) => {
                let old_query = state.query().to_string();
                apply_interactive_action(
                    action,
                    &mut state,
                    &mut preview_cache,
                    results.rows(),
                    &options,
                    viewport.rows,
                );
                if matches!(action, TuiAction::DeleteOrExit) && old_query.is_empty() {
                    return Ok(TuiOutcome::NoSelection);
                }
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
