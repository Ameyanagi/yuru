mod aliases;
mod backend;
mod cli;
mod compat;
mod config;
mod configure;
mod doctor;
mod fields;
mod input;
mod options;
mod shell;
mod shell_words;

use std::io::{self, BufRead, BufReader, IsTerminal, Write};
use std::process::{ExitCode, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use aliases::{apply_aliases, apply_aliases_to_candidate};
use anyhow::{bail, Context, Result};
use backend::create_backend;
use clap::Parser;
use cli::{shell_script_kind, Args, CommandArg};
use compat::{enforce_fzf_compat, warn_reserved_zh_options};
use config::{expanded_args, walker_flags_present};
use configure::configure_interactive;
use doctor::print_doctor_report;
use fields::{
    accept_output, prepare_item, prepare_items, FieldConfig, InputItem, InputRecord, OutputRecord,
};
use input::*;
use options::*;
use shell::print_shell_script;
use shell_words::print_split_shell_words;
use yuru_core::{
    build_index, dedup_and_limit_variants, key_kind_allowed, match_positions, search, Candidate,
    LanguageBackend, QueryVariant, ScoredCandidate, SearchConfig, SearchKey, SourceSpan,
};

#[cfg(windows)]
const WINDOWS_MAIN_STACK_SIZE: usize = 8 * 1024 * 1024;

type SharedInputItems = Arc<Mutex<Vec<InputItem>>>;
type CandidateStreamReceiver = mpsc::Receiver<yuru_tui::CandidateStreamMessage>;

#[cfg(windows)]
fn main() -> ExitCode {
    // Windows debug binaries get a smaller main-thread stack than Unix-like
    // platforms. Clap's generated parser for the fzf-compatible option surface
    // can overflow that stack before command dispatch.
    let handle = match thread::Builder::new()
        .name("yuru-main".to_string())
        .stack_size(WINDOWS_MAIN_STACK_SIZE)
        .spawn(run_main)
    {
        Ok(handle) => handle,
        Err(error) => {
            eprintln!("yuru: failed to start main thread: {error}");
            return ExitCode::from(2);
        }
    };

    match handle.join() {
        Ok(code) => code,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    run_main()
}

fn run_main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("yuru: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    let expanded_args = expanded_args()?;
    let walker_requested = walker_flags_present(&expanded_args);
    let args = Args::parse_from(expanded_args);
    if let Some(kind) = shell_script_kind(&args)? {
        print_shell_script(kind)?;
        return Ok(ExitCode::SUCCESS);
    }
    match &args.command {
        Some(CommandArg::Configure) => {
            configure_interactive()?;
            return Ok(ExitCode::SUCCESS);
        }
        Some(CommandArg::Doctor) => {
            print_doctor_report()?;
            return Ok(ExitCode::SUCCESS);
        }
        Some(CommandArg::SplitShellWords { words }) => {
            print_split_shell_words(words)?;
            return Ok(ExitCode::SUCCESS);
        }
        None => {}
    }
    if explain_mode(&args) && print0_enabled(&args) {
        bail!("--explain cannot be combined with --print0");
    }
    enforce_fzf_compat(&args)?;
    warn_reserved_zh_options(&args);
    let RunOptions {
        query,
        interactive,
        field_config,
        search_config: config,
    } = RunOptions::from_args(&args)?;
    if interactive && should_stream_interactive(&args, walker_requested) {
        let backend = create_backend(&args, &query, &[]);
        let (items, receiver) =
            spawn_streaming_candidates(&args, &field_config, backend.clone(), config.clone())?;
        return run_interactive_streaming_mode(
            &args,
            items,
            &field_config,
            receiver,
            backend,
            config,
            query,
        );
    }

    let raw_items =
        read_input_candidates(&args, walker_requested).context("failed to load candidates")?;
    let (header_records, mut raw_items) = split_header_lines(raw_items, header_lines_count(&args));
    if let Some(tail) = tail_count(&args) {
        let keep_from = raw_items.len().saturating_sub(tail);
        raw_items = raw_items.split_off(keep_from);
    }
    if tac_enabled(&args) {
        raw_items.reverse();
    }

    let items = prepare_items(raw_items, &field_config, ansi_enabled(&args))?;
    let backend = create_backend(&args, &query, &items);
    if args.debug_query_variants {
        print_query_variants(&query, backend.as_ref(), &config, print0_enabled(&args))?;
    }
    let mut index = build_index(
        items.iter().map(|item| item.search_text.clone()),
        backend.as_ref(),
        &config,
    );
    apply_aliases(&mut index, &items, &args.aliases, &config)?;

    if interactive {
        return run_interactive_mode(
            &args,
            &items,
            &field_config,
            &index,
            &header_records,
            backend,
            config,
            query,
        );
    }

    let results = search(&query, &index, backend.as_ref(), &config);

    if explain_mode(&args) {
        write_explain_output(
            &query,
            &results,
            &items,
            &field_config,
            &index,
            backend.as_ref(),
            &config,
        )?;
        if results.is_empty() && !exit_0_enabled(&args) {
            return Ok(ExitCode::from(1));
        }
        return Ok(ExitCode::SUCCESS);
    }

    let mut output = Vec::new();
    if print_query_enabled(&args) {
        output.push(OutputRecord::Text(query.clone()));
    }

    if select_1_enabled(&args) && results.len() == 1 {
        output.push(accept_output(
            &items[results[0].id],
            &field_config,
            results[0].id,
        )?);
    } else {
        for result in &results {
            output.push(accept_output(&items[result.id], &field_config, result.id)?);
        }
    }

    write_records(&output, print0_enabled(&args))?;

    if results.is_empty() && !exit_0_enabled(&args) && !args.debug_query_variants {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

#[allow(clippy::too_many_arguments)]
fn run_interactive_mode(
    args: &Args,
    items: &[InputItem],
    field_config: &FieldConfig,
    index: &[yuru_core::Candidate],
    header_records: &[InputRecord],
    backend: Arc<dyn LanguageBackend>,
    config: SearchConfig,
    query: String,
) -> Result<ExitCode> {
    let options = yuru_tui::TuiOptions {
        initial_query: query,
        prompt: args.prompt.clone().unwrap_or_else(|| "> ".to_string()),
        header: header_text(args, header_records),
        footer: footer_text(args),
        expect_keys: parse_expect_keys(expect_arg(args)),
        bindings: parse_bindings(&args.bind),
        height: parse_tui_height(args),
        layout: parse_tui_layout(args)?,
        preview: preview_command(args),
        preview_shell: args.with_shell.clone(),
        preview_image_protocol: preview_image_protocol(args),
        style: parse_tui_style(&args.color),
        highlight_line: highlight_line_enabled(args),
        cycle: cycle_enabled(args),
        multi: multi_enabled(args),
        multi_limit: multi_limit(args),
        no_input: args.no_input,
        pointer: first_line(args.pointer.as_deref().unwrap_or(">")),
        marker: first_line(args.marker.as_deref().unwrap_or("*")),
        ellipsis: first_line(args.ellipsis.as_deref().unwrap_or("..")),
    };

    match yuru_tui::run_interactive(index, backend, config, options)? {
        yuru_tui::TuiOutcome::Accepted { ids, query, expect } => {
            let mut output = Vec::new();
            if expect_arg(args).is_some() {
                output.push(OutputRecord::Text(expect.unwrap_or_default()));
            }
            if print_query_enabled(args) {
                output.push(OutputRecord::Text(query));
            }
            for id in ids {
                output.push(accept_output(&items[id], field_config, id)?);
            }
            write_records(&output, print0_enabled(args))?;
            Ok(ExitCode::SUCCESS)
        }
        yuru_tui::TuiOutcome::NoSelection => {
            if exit_0_enabled(args) {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(1))
            }
        }
        yuru_tui::TuiOutcome::Aborted => Ok(ExitCode::from(130)),
    }
}

fn run_interactive_streaming_mode(
    args: &Args,
    items: SharedInputItems,
    field_config: &FieldConfig,
    receiver: CandidateStreamReceiver,
    backend: Arc<dyn LanguageBackend>,
    config: SearchConfig,
    query: String,
) -> Result<ExitCode> {
    let options = yuru_tui::TuiOptions {
        initial_query: query,
        prompt: args.prompt.clone().unwrap_or_else(|| "> ".to_string()),
        header: header_text(args, &[]),
        footer: footer_text(args),
        expect_keys: parse_expect_keys(expect_arg(args)),
        bindings: parse_bindings(&args.bind),
        height: parse_tui_height(args),
        layout: parse_tui_layout(args)?,
        preview: preview_command(args),
        preview_shell: args.with_shell.clone(),
        preview_image_protocol: preview_image_protocol(args),
        style: parse_tui_style(&args.color),
        highlight_line: highlight_line_enabled(args),
        cycle: cycle_enabled(args),
        multi: multi_enabled(args),
        multi_limit: multi_limit(args),
        no_input: args.no_input,
        pointer: first_line(args.pointer.as_deref().unwrap_or(">")),
        marker: first_line(args.marker.as_deref().unwrap_or("*")),
        ellipsis: first_line(args.ellipsis.as_deref().unwrap_or("..")),
    };

    match yuru_tui::run_interactive_streaming(receiver, backend, config, options)? {
        yuru_tui::TuiOutcome::Accepted { ids, query, expect } => {
            let items = items
                .lock()
                .map_err(|_| anyhow::anyhow!("streamed candidate store is unavailable"))?;
            let mut output = Vec::new();
            if expect_arg(args).is_some() {
                output.push(OutputRecord::Text(expect.unwrap_or_default()));
            }
            if print_query_enabled(args) {
                output.push(OutputRecord::Text(query));
            }
            for id in ids {
                let Some(item) = items.get(id) else {
                    bail!("selected streamed candidate disappeared: {id}");
                };
                output.push(accept_output(item, field_config, id)?);
            }
            write_records(&output, print0_enabled(args))?;
            Ok(ExitCode::SUCCESS)
        }
        yuru_tui::TuiOutcome::NoSelection => {
            if exit_0_enabled(args) {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(1))
            }
        }
        yuru_tui::TuiOutcome::Aborted => Ok(ExitCode::from(130)),
    }
}

fn should_stream_interactive(args: &Args, walker_requested: bool) -> bool {
    if sync_enabled(args)
        || args.input.is_some()
        || tac_enabled(args)
        || tail_count(args).is_some()
        || header_lines_count(args) > 0
        || select_1_enabled(args)
        || exit_0_enabled(args)
        || walker_requested
    {
        return false;
    }

    !io::stdin().is_terminal() || non_empty_default_source_command().is_some()
}

fn spawn_streaming_candidates(
    args: &Args,
    field_config: &FieldConfig,
    backend: Arc<dyn LanguageBackend>,
    config: SearchConfig,
) -> Result<(SharedInputItems, CandidateStreamReceiver)> {
    let (sender, receiver) = mpsc::channel();
    let items = Arc::new(Mutex::new(Vec::new()));
    let worker_items = items.clone();
    let worker_field_config = field_config.clone();
    let read0 = read0_enabled(args);
    let ansi = ansi_enabled(args);
    let aliases = args.aliases.clone();
    let source = if io::stdin().is_terminal() {
        non_empty_default_source_command()
            .map(|(env_name, command)| StreamingSource::Command { env_name, command })
    } else {
        Some(StreamingSource::Stdin)
    };

    let Some(source) = source else {
        bail!("no streaming source available");
    };

    thread::spawn(move || {
        let result = match source {
            StreamingSource::Stdin => {
                let stdin = io::stdin();
                let reader = stdin.lock();
                stream_records_from_reader(
                    reader,
                    read0,
                    worker_items,
                    sender.clone(),
                    worker_field_config,
                    backend,
                    config,
                    aliases,
                    ansi,
                )
            }
            StreamingSource::Command { env_name, command } => stream_records_from_command(
                env_name,
                &command,
                read0,
                worker_items,
                sender.clone(),
                worker_field_config,
                backend,
                config,
                aliases,
                ansi,
            ),
        };

        match result {
            Ok(()) => {
                let _ = sender.send(yuru_tui::CandidateStreamMessage::Finished);
            }
            Err(error) => {
                let _ = sender.send(yuru_tui::CandidateStreamMessage::Error(format!(
                    "{error:#}"
                )));
            }
        }
    });

    Ok((items, receiver))
}

enum StreamingSource {
    Stdin,
    Command {
        env_name: &'static str,
        command: String,
    },
}

#[allow(clippy::too_many_arguments)]
fn stream_records_from_command(
    env_name: &'static str,
    command: &str,
    read0: bool,
    items: Arc<Mutex<Vec<InputItem>>>,
    sender: mpsc::Sender<yuru_tui::CandidateStreamMessage>,
    field_config: FieldConfig,
    backend: Arc<dyn LanguageBackend>,
    config: SearchConfig,
    aliases: Vec<String>,
    ansi: bool,
) -> Result<()> {
    let mut child = default_command_process(command)
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to run {env_name}: {command}"))?;
    let stdout = child
        .stdout
        .take()
        .context("failed to capture default command stdout")?;
    stream_records_from_reader(
        BufReader::new(stdout),
        read0,
        items,
        sender,
        field_config,
        backend,
        config,
        aliases,
        ansi,
    )?;
    let status = child
        .wait()
        .with_context(|| format!("failed to wait for {env_name}: {command}"))?;
    if !status.success() {
        bail!("{env_name} exited with {status}");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn stream_records_from_reader<R: BufRead>(
    mut reader: R,
    read0: bool,
    items: Arc<Mutex<Vec<InputItem>>>,
    sender: mpsc::Sender<yuru_tui::CandidateStreamMessage>,
    field_config: FieldConfig,
    backend: Arc<dyn LanguageBackend>,
    config: SearchConfig,
    aliases: Vec<String>,
    ansi: bool,
) -> Result<()> {
    let delimiter = if read0 { b'\0' } else { b'\n' };
    let mut raw = Vec::new();
    loop {
        raw.clear();
        let read = reader.read_until(delimiter, &mut raw)?;
        if read == 0 {
            break;
        }
        if raw.ends_with(&[delimiter]) {
            raw.pop();
        }
        if !read0 && raw.ends_with(b"\r") {
            raw.pop();
        }
        if read0 && raw.is_empty() {
            continue;
        }

        let record = InputRecord::from_raw(raw.clone());
        let mut locked = items
            .lock()
            .map_err(|_| anyhow::anyhow!("streamed candidate store is unavailable"))?;
        let id = locked.len();
        let item = prepare_item(record, &field_config, ansi, id)?;
        let mut candidate =
            yuru_core::build_candidate(id, item.search_text.clone(), backend.as_ref(), &config);
        apply_aliases_to_candidate(&mut candidate, &item, &aliases, &config)?;
        locked.push(item);
        drop(locked);

        if sender
            .send(yuru_tui::CandidateStreamMessage::Candidate(candidate))
            .is_err()
        {
            break;
        }
    }

    Ok(())
}

fn print_query_variants(
    query: &str,
    backend: &dyn LanguageBackend,
    config: &SearchConfig,
    print0: bool,
) -> Result<()> {
    let variants = dedup_and_limit_variants(
        backend.expand_query(query, config.query_budget()),
        config.max_query_variants,
    );
    let mut records = vec![OutputRecord::Text(format!(
        "variant_count={}",
        variants.len()
    ))];
    records.extend(
        variants
            .into_iter()
            .map(|variant| OutputRecord::Text(format!("{}\t{:?}", variant.text, variant.kind))),
    );
    write_records(&records, print0)
}

fn write_records(records: &[OutputRecord], print0: bool) -> Result<()> {
    let mut stdout = io::stdout().lock();
    let separator = if print0 {
        b"\0".as_slice()
    } else {
        b"
"
        .as_slice()
    };
    for record in records {
        stdout.write_all(record.as_bytes())?;
        stdout.write_all(separator)?;
    }
    Ok(())
}

fn write_explain_output(
    query: &str,
    results: &[ScoredCandidate],
    items: &[InputItem],
    field_config: &FieldConfig,
    candidates: &[Candidate],
    backend: &dyn LanguageBackend,
    config: &SearchConfig,
) -> Result<()> {
    let mut stdout = io::stdout().lock();

    for result in results {
        let record = accept_output(&items[result.id], field_config, result.id)?;
        stdout.write_all(record.as_bytes())?;
        stdout.write_all(
            b"
",
        )?;

        let matched = explain_match(query, result, candidates, backend, config);
        writeln!(stdout, "  score: {}", result.score)?;
        writeln!(stdout, "  matched key: {:?}", result.key_kind)?;
        if let Some(matched) = matched {
            writeln!(stdout, "  query: {}", matched.pattern)?;
            writeln!(stdout, "  matched text: {}", matched.variant.text)?;
            writeln!(
                stdout,
                "  key span: {}..{}",
                matched.key_span.start_char, matched.key_span.end_char
            )?;
            writeln!(stdout, "  key text: {}", matched.key_text)?;
            match matched.source_span {
                Some(span) => {
                    let snippet = char_slice(&result.display, span.start_char, span.end_char);
                    writeln!(
                        stdout,
                        "  source span: {}..{} \"{}\"",
                        span.start_char, span.end_char, snippet
                    )?;
                }
                None => {
                    writeln!(stdout, "  source span: n/a")?;
                }
            }
        } else {
            writeln!(stdout, "  query: {query}")?;
            writeln!(stdout, "  matched text: n/a")?;
            writeln!(stdout, "  key span: n/a")?;
            writeln!(stdout, "  key text: n/a")?;
            writeln!(stdout, "  source span: n/a")?;
        }
    }

    Ok(())
}

#[derive(Clone, Debug)]
struct ExplainMatch {
    pattern: String,
    variant: QueryVariant,
    key_text: String,
    key_span: SourceSpan,
    source_span: Option<SourceSpan>,
}

fn explain_match(
    query: &str,
    result: &ScoredCandidate,
    candidates: &[Candidate],
    backend: &dyn LanguageBackend,
    config: &SearchConfig,
) -> Option<ExplainMatch> {
    let key = matched_key(candidates, result)?;
    let patterns = highlight_patterns(query);
    let patterns = if patterns.is_empty() {
        vec![query.to_string()]
    } else {
        patterns
    };

    for pattern in patterns {
        let variants = dedup_and_limit_variants(
            backend.expand_query(&pattern, config.query_budget()),
            config.max_query_variants,
        );
        for variant in variants {
            if config.case_sensitive && variant.kind == yuru_core::QueryVariantKind::Normalized {
                continue;
            }
            if !key_kind_allowed(&variant, key.kind) {
                continue;
            }
            let Some(positions) = match_positions(&variant.text, &key.text, config.case_sensitive)
            else {
                continue;
            };
            let Some(key_span) = span_from_positions(&positions.char_indices) else {
                continue;
            };
            let source_span = source_span_for_key_positions(key, &positions.char_indices)
                .or_else(|| direct_source_span(&pattern, &result.display, config.case_sensitive));
            return Some(ExplainMatch {
                pattern,
                variant,
                key_text: key.text.clone(),
                key_span,
                source_span,
            });
        }
    }

    None
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

fn span_from_positions(positions: &[usize]) -> Option<SourceSpan> {
    let start = positions.iter().copied().min()?;
    let end = positions.iter().copied().max()?.saturating_add(1);
    Some(SourceSpan {
        start_char: start,
        end_char: end,
    })
}

fn source_span_for_key_positions(key: &SearchKey, positions: &[usize]) -> Option<SourceSpan> {
    let source_map = key.source_map.as_ref()?;
    let mut start = usize::MAX;
    let mut end = 0usize;
    let mut found = false;

    for position in positions {
        let Some(Some(span)) = source_map.get(*position) else {
            continue;
        };
        start = start.min(span.start_char);
        end = end.max(span.end_char);
        found = true;
    }

    found.then_some(SourceSpan {
        start_char: start,
        end_char: end,
    })
}

fn direct_source_span(pattern: &str, display: &str, case_sensitive: bool) -> Option<SourceSpan> {
    let positions = match_positions(pattern, display, case_sensitive)?;
    span_from_positions(&positions.char_indices)
}

fn char_slice(text: &str, start: usize, end: usize) -> String {
    text.chars()
        .enumerate()
        .filter_map(|(index, ch)| (start..end).contains(&index).then_some(ch))
        .collect()
}
