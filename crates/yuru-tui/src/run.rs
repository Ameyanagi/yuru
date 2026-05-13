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
use yuru_core::{Candidate, LanguageBackend, SearchConfig};

use crate::actions::apply_interactive_action;
use crate::api::{CandidateStreamMessage, TuiOptions, TuiOutcome};
use crate::keys::{classify_key, KeyDecision};
use crate::preview::PreviewCache;
use crate::render::{preview_geometry, render, RenderContext, Viewport};
use crate::search_worker::{
    request_owned_search, request_snapshot_search, SearchWorker, SEARCH_WORKER_POLL,
};
use crate::state::TuiState;
use crate::terminal::TerminalGuard;

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

fn read_actionable_key_event() -> Result<Option<KeyEvent>> {
    match event::read()? {
        Event::Key(key) if is_actionable_key_event(&key) => Ok(Some(key)),
        _ => Ok(None),
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

        let has_prompt = !options.no_input;
        let viewport = Viewport::from_terminal(options.height, has_prompt);
        let preview_geometry = preview_geometry(
            viewport,
            options.layout,
            has_prompt,
            options.preview.is_some() && !results.is_empty(),
        );
        preview_cache.request_for_selection(
            options.preview.as_ref(),
            options.preview_shell.as_deref(),
            &results,
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
            let Some(key) = read_actionable_key_event()? else {
                continue;
            };
            key
        } else {
            loop {
                if let Some(key) = read_actionable_key_event()? {
                    break key;
                }
            }
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

        let has_prompt = !options.no_input;
        let viewport = Viewport::from_terminal(options.height, has_prompt);
        let preview_geometry = preview_geometry(
            viewport,
            options.layout,
            has_prompt,
            options.preview.is_some() && !results.is_empty(),
        );
        preview_cache.request_for_selection(
            options.preview.as_ref(),
            options.preview_shell.as_deref(),
            &results,
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

        let Some(key) = read_actionable_key_event()? else {
            continue;
        };
        let viewport = Viewport::from_terminal(options.height, !options.no_input);
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
