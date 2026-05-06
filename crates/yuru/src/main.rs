mod fields;
mod shell;

use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use fields::{
    accept_output, prepare_item, prepare_items, FieldConfig, InputItem, InputRecord, OutputRecord,
};
use ignore::{DirEntry, WalkBuilder};
use shell::ShellKind;
use yuru_core::{
    build_index, dedup_and_limit_keys, dedup_and_limit_variants, key_kind_allowed, match_positions,
    search, Candidate, LanguageBackend, MatcherAlgo, PlainBackend, QueryVariant, ScoredCandidate,
    SearchConfig, SearchKey, SourceSpan, Tiebreak,
};
use yuru_ja::{JapaneseBackend, JapaneseReadingMode};
use yuru_zh::{ChineseBackend, ChinesePolyphoneMode, ChineseScriptMode};

const DEFAULT_WALKER: &str = "file,follow,hidden";
const DEFAULT_WALKER_ROOT: &str = ".";
const DEFAULT_WALKER_SKIP: &str = ".git,node_modules";
const DEFAULT_INTERACTIVE_LIMIT: usize = 1000;

type SharedInputItems = Arc<Mutex<Vec<InputItem>>>;
type CandidateStreamReceiver = mpsc::Receiver<yuru_tui::CandidateStreamMessage>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum LangArg {
    Plain,
    Ja,
    Zh,
    Auto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SchemeArg {
    Default,
    Path,
    History,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum FzfCompatArg {
    Strict,
    Warn,
    Ignore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum LoadFzfDefaultOptsArg {
    Never,
    Safe,
    All,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum JaReadingArg {
    None,
    Lindera,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ZhPolyphoneArg {
    None,
    Common,
    Phrase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ZhScriptArg {
    Auto,
    Hans,
    Hant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum AlgoArg {
    Greedy,
    #[value(alias = "v1")]
    FzfV1,
    #[value(alias = "v2")]
    FzfV2,
    Nucleo,
}

#[derive(Debug, Subcommand)]
enum CommandArg {
    /// Reconfigure user defaults interactively.
    Configure,

    /// Print environment, config, and shell integration diagnostics.
    Doctor,

    /// Parse shell words for generated shell integrations.
    #[command(name = "__split-shell-words", hide = true)]
    SplitShellWords {
        #[arg(allow_hyphen_values = true)]
        words: String,
    },
}

#[derive(Debug, Parser)]
#[command(
    name = "yuru",
    about = "A fast phonetic fuzzy finder prototype",
    version,
    args_override_self = true
)]
struct Args {
    #[command(subcommand)]
    command: Option<CommandArg>,

    #[arg(long, value_enum, default_value_t = LangArg::Plain)]
    lang: LangArg,

    #[arg(long = "ja-reading", value_enum, default_value_t = JaReadingArg::Lindera)]
    ja_reading: JaReadingArg,

    #[arg(long = "zh-pinyin", default_value_t = true)]
    zh_pinyin: bool,

    #[arg(long = "no-zh-pinyin")]
    no_zh_pinyin: bool,

    #[arg(long = "zh-initials", default_value_t = true)]
    zh_initials: bool,

    #[arg(long = "no-zh-initials")]
    no_zh_initials: bool,

    #[arg(long = "zh-polyphone", value_enum, default_value_t = ZhPolyphoneArg::Common)]
    zh_polyphone: ZhPolyphoneArg,

    #[arg(long = "zh-script", value_enum, default_value_t = ZhScriptArg::Auto)]
    zh_script: ZhScriptArg,

    #[arg(short = 'q', long)]
    query: Option<String>,

    #[arg(short = 'f', long)]
    filter: Option<String>,

    #[arg(long)]
    limit: Option<usize>,

    #[arg(long, default_value_t = 8)]
    max_query_variants: usize,

    #[arg(long, default_value_t = 8)]
    max_keys_per_candidate: usize,

    #[arg(long, default_value_t = 1024)]
    max_total_key_bytes_per_candidate: usize,

    #[arg(long, default_value_t = 1000)]
    top_b: usize,

    #[arg(short = 'e', long)]
    exact: bool,

    #[arg(long = "no-exact")]
    no_exact: bool,

    #[arg(long = "extended-exact")]
    extended_exact: bool,

    #[arg(short = 'x', long, default_value_t = true)]
    extended: bool,

    #[arg(long = "no-extended")]
    no_extended: bool,

    #[arg(short = 'i', long)]
    ignore_case: bool,

    #[arg(long = "no-ignore-case")]
    no_ignore_case: bool,

    #[arg(long, default_value_t = true)]
    smart_case: bool,

    #[arg(long)]
    no_sort: bool,

    #[arg(short = 's', long, num_args = 0..=1)]
    sort: Option<Option<usize>>,

    #[arg(long, default_value = "length")]
    tiebreak: String,

    #[arg(long, value_enum, default_value_t = SchemeArg::Default)]
    scheme: SchemeArg,

    #[arg(long)]
    disabled: bool,

    #[arg(long)]
    phony: bool,

    #[arg(long)]
    enabled: bool,

    #[arg(long = "no-phony")]
    no_phony: bool,

    #[arg(long)]
    literal: bool,

    #[arg(long = "no-literal")]
    no_literal: bool,

    #[arg(long)]
    tac: bool,

    #[arg(long = "no-tac")]
    no_tac: bool,

    #[arg(long)]
    tail: Option<usize>,

    #[arg(long = "no-tail")]
    no_tail: bool,

    #[arg(long)]
    read0: bool,

    #[arg(long = "no-read0")]
    no_read0: bool,

    #[arg(long)]
    sync: bool,

    #[arg(long = "no-sync", alias = "async")]
    no_sync: bool,

    #[arg(long)]
    print0: bool,

    #[arg(long = "no-print0")]
    no_print0: bool,

    #[arg(long, hide = true)]
    input: Option<PathBuf>,

    #[arg(long)]
    ansi: bool,

    #[arg(long = "no-ansi")]
    no_ansi: bool,

    #[arg(long)]
    print_query: bool,

    #[arg(long = "no-print-query")]
    no_print_query: bool,

    #[arg(short = '1', long)]
    select_1: bool,

    #[arg(long = "no-select-1")]
    no_select_1: bool,

    #[arg(short = '0', long)]
    exit_0: bool,

    #[arg(long = "no-exit-0")]
    no_exit_0: bool,

    #[arg(short = 'n', long)]
    nth: Option<String>,

    #[arg(long)]
    with_nth: Option<String>,

    #[arg(long)]
    accept_nth: Option<String>,

    #[arg(short = 'd', long)]
    delimiter: Option<String>,

    #[arg(long, value_enum, default_value_t = AlgoArg::Greedy)]
    algo: AlgoArg,

    #[arg(long = "fzf-compat", value_enum)]
    fzf_compat: Option<FzfCompatArg>,

    #[arg(long = "load-fzf-default-opts", value_enum, default_value_t = LoadFzfDefaultOptsArg::Safe)]
    load_fzf_default_opts: LoadFzfDefaultOptsArg,

    #[arg(short = 'm', long, num_args = 0..=1)]
    multi: Option<Option<usize>>,

    #[arg(long)]
    no_multi: bool,

    #[arg(long)]
    expect: Option<String>,

    #[arg(long = "no-expect")]
    no_expect: bool,

    #[arg(long)]
    bind: Vec<String>,

    #[arg(long = "toggle-sort")]
    toggle_sort: Option<String>,

    #[arg(long)]
    preview: Option<String>,

    #[arg(long = "no-preview")]
    no_preview: bool,

    #[arg(long)]
    preview_window: Option<String>,

    #[arg(long, num_args = 0..=1)]
    preview_border: Option<Option<String>>,

    #[arg(long = "no-preview-border")]
    no_preview_border: bool,

    #[arg(long)]
    preview_label: Option<String>,

    #[arg(long)]
    preview_label_pos: Option<String>,

    #[arg(long)]
    preview_wrap_sign: Option<String>,

    #[arg(long)]
    height: Option<String>,

    #[arg(long)]
    no_height: bool,

    #[arg(long)]
    min_height: Option<String>,

    #[arg(long, num_args = 0..=1)]
    popup: Option<Option<String>>,

    #[arg(long = "no-popup")]
    no_popup: bool,

    #[arg(long, num_args = 0..=1)]
    tmux: Option<Option<String>>,

    #[arg(long = "no-tmux")]
    no_tmux: bool,

    #[arg(long)]
    layout: Option<String>,

    #[arg(long)]
    reverse: bool,

    #[arg(long = "no-reverse")]
    no_reverse: bool,

    #[arg(long)]
    margin: Option<String>,

    #[arg(long)]
    padding: Option<String>,

    #[arg(long = "no-margin")]
    no_margin: bool,

    #[arg(long = "no-padding")]
    no_padding: bool,

    #[arg(long = "no-border")]
    no_border: bool,

    #[arg(long, num_args = 0..=1)]
    border: Option<Option<String>>,

    #[arg(long)]
    border_label: Option<String>,

    #[arg(long)]
    border_label_pos: Option<String>,

    #[arg(long = "no-border-label")]
    no_border_label: bool,

    #[arg(long)]
    prompt: Option<String>,

    #[arg(long)]
    header: Option<String>,

    #[arg(long = "no-header")]
    no_header: bool,

    #[arg(long)]
    header_lines: Option<usize>,

    #[arg(long = "no-header-lines")]
    no_header_lines: bool,

    #[arg(long)]
    header_first: bool,

    #[arg(long = "no-header-first")]
    no_header_first: bool,

    #[arg(long, num_args = 0..=1)]
    header_border: Option<Option<String>>,

    #[arg(long = "no-header-border")]
    no_header_border: bool,

    #[arg(long, num_args = 0..=1)]
    header_lines_border: Option<Option<String>>,

    #[arg(long = "no-header-lines-border")]
    no_header_lines_border: bool,

    #[arg(long)]
    header_label: Option<String>,

    #[arg(long)]
    header_label_pos: Option<String>,

    #[arg(long = "no-header-label")]
    no_header_label: bool,

    #[arg(long)]
    footer: Option<String>,

    #[arg(long = "no-footer")]
    no_footer: bool,

    #[arg(long, num_args = 0..=1)]
    footer_border: Option<Option<String>>,

    #[arg(long = "no-footer-border")]
    no_footer_border: bool,

    #[arg(long)]
    footer_label: Option<String>,

    #[arg(long)]
    footer_label_pos: Option<String>,

    #[arg(long = "no-footer-label")]
    no_footer_label: bool,

    #[arg(long, num_args = 0..=1)]
    color: Vec<Option<String>>,

    #[arg(long)]
    no_color: bool,

    #[arg(long = "no-256")]
    no_256: bool,

    #[arg(long)]
    bold: bool,

    #[arg(long)]
    no_bold: bool,

    #[arg(long)]
    black: bool,

    #[arg(long = "no-black")]
    no_black: bool,

    #[arg(long)]
    cycle: bool,

    #[arg(long = "no-cycle")]
    no_cycle: bool,

    #[arg(long)]
    highlight_line: bool,

    #[arg(long = "no-highlight-line")]
    no_highlight_line: bool,

    #[arg(long, num_args = 0..=1)]
    wrap: Option<Option<String>>,

    #[arg(long = "no-wrap")]
    no_wrap: bool,

    #[arg(long = "wrap-word")]
    wrap_word: bool,

    #[arg(long = "no-wrap-word")]
    no_wrap_word: bool,

    #[arg(long)]
    wrap_sign: Option<String>,

    #[arg(long = "multi-line")]
    multi_line: bool,

    #[arg(long)]
    no_multi_line: bool,

    #[arg(long)]
    raw: bool,

    #[arg(long = "no-raw")]
    no_raw: bool,

    #[arg(long)]
    track: bool,

    #[arg(long = "no-track")]
    no_track: bool,

    #[arg(long)]
    id_nth: Option<String>,

    #[arg(long = "no-id-nth")]
    no_id_nth: bool,

    #[arg(long, num_args = 0..=1)]
    gap: Option<Option<usize>>,

    #[arg(long = "no-gap")]
    no_gap: bool,

    #[arg(long, num_args = 0..=1)]
    gap_line: Option<Option<String>>,

    #[arg(long = "no-gap-line")]
    no_gap_line: bool,

    #[arg(long)]
    freeze_left: Option<usize>,

    #[arg(long)]
    freeze_right: Option<usize>,

    #[arg(long)]
    keep_right: bool,

    #[arg(long = "no-keep-right")]
    no_keep_right: bool,

    #[arg(long)]
    scroll_off: Option<usize>,

    #[arg(long)]
    no_hscroll: bool,

    #[arg(long)]
    hscroll: bool,

    #[arg(long)]
    hscroll_off: Option<usize>,

    #[arg(long)]
    jump_labels: Option<String>,

    #[arg(long)]
    gutter: Option<String>,

    #[arg(long)]
    gutter_raw: Option<String>,

    #[arg(long)]
    pointer: Option<String>,

    #[arg(long)]
    marker: Option<String>,

    #[arg(long)]
    marker_multi_line: Option<String>,

    #[arg(long)]
    ellipsis: Option<String>,

    #[arg(long)]
    tabstop: Option<usize>,

    #[arg(long, num_args = 0..=1)]
    scrollbar: Option<Option<String>>,

    #[arg(long)]
    no_scrollbar: bool,

    #[arg(long, num_args = 0..=1)]
    list_border: Option<Option<String>>,

    #[arg(long = "no-list-border")]
    no_list_border: bool,

    #[arg(long)]
    list_label: Option<String>,

    #[arg(long)]
    list_label_pos: Option<String>,

    #[arg(long = "no-list-label")]
    no_list_label: bool,

    #[arg(long)]
    no_input: bool,

    #[arg(long)]
    info: Option<String>,

    #[arg(long)]
    info_command: Option<String>,

    #[arg(long = "no-info-command")]
    no_info_command: bool,

    #[arg(long = "no-info")]
    no_info: bool,

    #[arg(long = "inline-info")]
    inline_info: bool,

    #[arg(long = "no-inline-info")]
    no_inline_info: bool,

    #[arg(long)]
    separator: Option<String>,

    #[arg(long)]
    no_separator: bool,

    #[arg(long)]
    ghost: Option<String>,

    #[arg(long)]
    filepath_word: bool,

    #[arg(long = "no-filepath-word")]
    no_filepath_word: bool,

    #[arg(long, num_args = 0..=1)]
    input_border: Option<Option<String>>,

    #[arg(long = "no-input-border")]
    no_input_border: bool,

    #[arg(long)]
    input_label: Option<String>,

    #[arg(long)]
    input_label_pos: Option<String>,

    #[arg(long = "no-input-label")]
    no_input_label: bool,

    #[arg(long, default_value = DEFAULT_WALKER)]
    walker: String,

    #[arg(long = "walker-root", default_value = DEFAULT_WALKER_ROOT)]
    walker_roots: Vec<PathBuf>,

    #[arg(long = "walker-skip", default_value = DEFAULT_WALKER_SKIP)]
    walker_skip: String,

    #[arg(long)]
    with_shell: Option<String>,

    #[arg(long)]
    style: Option<String>,

    #[arg(long, num_args = 0..=1)]
    listen: Option<Option<String>>,

    #[arg(long = "no-listen")]
    no_listen: bool,

    #[arg(long, num_args = 0..=1)]
    listen_unsafe: Option<Option<String>>,

    #[arg(long = "no-listen-unsafe")]
    no_listen_unsafe: bool,

    #[arg(long)]
    history: Option<PathBuf>,

    #[arg(long = "no-history")]
    no_history: bool,

    #[arg(long)]
    history_size: Option<usize>,

    #[arg(long)]
    no_tty_default: bool,

    #[arg(long)]
    tty_default: Option<String>,

    #[arg(long = "force-tty-in")]
    force_tty_in: bool,

    #[arg(long = "no-force-tty-in")]
    no_force_tty_in: bool,

    #[arg(long = "proxy-script")]
    proxy_script: Option<String>,

    #[arg(long = "no-winpty")]
    no_winpty: bool,

    #[arg(long)]
    no_mouse: bool,

    #[arg(long)]
    no_unicode: bool,

    #[arg(long)]
    unicode: bool,

    #[arg(long)]
    ambidouble: bool,

    #[arg(long = "no-ambidouble")]
    no_ambidouble: bool,

    #[arg(long)]
    clear: bool,

    #[arg(long)]
    no_clear: bool,

    #[arg(long)]
    man: bool,

    #[arg(long)]
    threads: Option<usize>,

    #[arg(long)]
    bench: Option<String>,

    #[arg(long = "profile-cpu")]
    profile_cpu: Option<PathBuf>,

    #[arg(long = "profile-mem")]
    profile_mem: Option<PathBuf>,

    #[arg(long = "profile-block")]
    profile_block: Option<PathBuf>,

    #[arg(long = "profile-mutex")]
    profile_mutex: Option<PathBuf>,

    #[arg(long)]
    debug_query_variants: bool,

    #[arg(long)]
    explain: bool,

    #[arg(long = "debug-match", hide = true)]
    debug_match: bool,

    #[arg(long = "alias")]
    aliases: Vec<String>,

    #[arg(long)]
    bash: bool,

    #[arg(long)]
    zsh: bool,

    #[arg(long)]
    fish: bool,

    #[arg(long)]
    powershell: bool,
}

fn main() -> ExitCode {
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
    let query = effective_query(&args);
    let interactive = should_run_interactive(&args);
    let limit = args
        .limit
        .unwrap_or_else(|| default_limit(&args, interactive));
    let tiebreaks = parse_tiebreaks(&args)?;
    let config = SearchConfig {
        max_query_variants: args.max_query_variants,
        max_search_keys_per_candidate: args.max_keys_per_candidate,
        max_total_key_bytes_per_candidate: args.max_total_key_bytes_per_candidate,
        limit,
        top_b_for_quality_score: args.top_b,
        exact: exact_enabled(&args),
        extended: extended_enabled(&args),
        case_sensitive: case_sensitive(&query, &args),
        disabled: disabled_enabled(&args),
        no_sort: no_sort_enabled(&args),
        normalize: normalize_enabled(&args),
        matcher_algo: matcher_algo(args.algo),
        tiebreaks,
    };

    let field_config = FieldConfig {
        delimiter: args.delimiter.clone(),
        nth: args.nth.clone(),
        with_nth: args.with_nth.clone(),
        accept_nth: args.accept_nth.clone(),
    };
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

fn create_backend(args: &Args, query: &str, items: &[InputItem]) -> Arc<dyn LanguageBackend> {
    let lang = match args.lang {
        LangArg::Auto => detect_auto_lang(query, items),
        lang => lang,
    };

    match lang {
        LangArg::Plain => Arc::new(PlainBackend),
        LangArg::Ja => Arc::new(JapaneseBackend::new(japanese_reading_mode(args.ja_reading))),
        LangArg::Zh => Arc::new(ChineseBackend::new(
            args.zh_pinyin && !args.no_zh_pinyin,
            args.zh_initials && !args.no_zh_initials,
            chinese_polyphone_mode(args.zh_polyphone),
            chinese_script_mode(args.zh_script),
        )),
        LangArg::Auto => unreachable!("auto language mode is resolved before backend creation"),
    }
}

fn japanese_reading_mode(value: JaReadingArg) -> JapaneseReadingMode {
    match value {
        JaReadingArg::None => JapaneseReadingMode::None,
        JaReadingArg::Lindera => JapaneseReadingMode::Lindera,
    }
}

fn chinese_polyphone_mode(value: ZhPolyphoneArg) -> ChinesePolyphoneMode {
    match value {
        ZhPolyphoneArg::None => ChinesePolyphoneMode::None,
        ZhPolyphoneArg::Common => ChinesePolyphoneMode::Common,
        ZhPolyphoneArg::Phrase => ChinesePolyphoneMode::Phrase,
    }
}

fn chinese_script_mode(value: ZhScriptArg) -> ChineseScriptMode {
    match value {
        ZhScriptArg::Auto => ChineseScriptMode::Auto,
        ZhScriptArg::Hans => ChineseScriptMode::Hans,
        ZhScriptArg::Hant => ChineseScriptMode::Hant,
    }
}

fn matcher_algo(value: AlgoArg) -> MatcherAlgo {
    match value {
        AlgoArg::Greedy => MatcherAlgo::Greedy,
        AlgoArg::FzfV1 => MatcherAlgo::FzfV1,
        AlgoArg::FzfV2 => MatcherAlgo::FzfV2,
        AlgoArg::Nucleo => MatcherAlgo::Nucleo,
    }
}

fn detect_auto_lang(query: &str, items: &[InputItem]) -> LangArg {
    if yuru_core::normalize::contains_kana(query) {
        return LangArg::Ja;
    }

    let ascii_query = query.chars().any(|ch| ch.is_ascii_alphabetic())
        && query.chars().all(|ch| ch.is_ascii() || ch.is_whitespace());
    if !ascii_query {
        return LangArg::Plain;
    }

    let locale = locale_hint();
    let sample = items.iter().take(256);
    let mut sample_has_kana = false;
    let mut sample_has_han = false;
    for item in sample {
        sample_has_kana |= yuru_core::normalize::contains_kana(&item.search_text);
        sample_has_han |= contains_han(&item.search_text);
        if sample_has_kana && sample_has_han {
            break;
        }
    }

    if sample_has_kana || locale.starts_with("ja") && sample_has_han {
        LangArg::Ja
    } else if locale.starts_with("zh") && sample_has_han {
        LangArg::Zh
    } else {
        LangArg::Plain
    }
}

fn locale_hint() -> String {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn contains_han(text: &str) -> bool {
    text.chars().any(|ch| {
        ('\u{3400}'..='\u{4dbf}').contains(&ch) || ('\u{4e00}'..='\u{9fff}').contains(&ch)
    })
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
        style: parse_tui_style(&args.color),
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
        style: parse_tui_style(&args.color),
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

fn should_run_interactive(args: &Args) -> bool {
    should_run_interactive_with_tty(args, io::stderr().is_terminal())
}

fn should_run_interactive_with_tty(args: &Args, ui_tty_available: bool) -> bool {
    args.filter.is_none() && !args.debug_query_variants && !explain_mode(args) && ui_tty_available
}

fn explain_mode(args: &Args) -> bool {
    args.explain || args.debug_match
}

fn default_limit(args: &Args, interactive: bool) -> usize {
    if args.filter.is_some() {
        usize::MAX
    } else if interactive {
        DEFAULT_INTERACTIVE_LIMIT
    } else {
        10
    }
}

fn exact_enabled(args: &Args) -> bool {
    (args.exact || args.extended_exact) && !args.no_exact
}

fn extended_enabled(args: &Args) -> bool {
    (args.extended || args.extended_exact) && !args.no_extended
}

fn disabled_enabled(args: &Args) -> bool {
    (args.disabled || args.phony) && !(args.enabled || args.no_phony)
}

fn no_sort_enabled(args: &Args) -> bool {
    args.no_sort && args.sort.is_none()
}

fn normalize_enabled(args: &Args) -> bool {
    !args.literal || args.no_literal
}

fn tac_enabled(args: &Args) -> bool {
    args.tac && !args.no_tac
}

fn tail_count(args: &Args) -> Option<usize> {
    (!args.no_tail).then_some(args.tail).flatten()
}

fn read0_enabled(args: &Args) -> bool {
    args.read0 && !args.no_read0
}

fn sync_enabled(args: &Args) -> bool {
    args.sync && !args.no_sync
}

fn print0_enabled(args: &Args) -> bool {
    args.print0 && !args.no_print0
}

fn ansi_enabled(args: &Args) -> bool {
    args.ansi && !args.no_ansi
}

fn print_query_enabled(args: &Args) -> bool {
    args.print_query && !args.no_print_query
}

fn select_1_enabled(args: &Args) -> bool {
    args.select_1 && !args.no_select_1
}

fn exit_0_enabled(args: &Args) -> bool {
    args.exit_0 && !args.no_exit_0
}

fn multi_enabled(args: &Args) -> bool {
    args.multi.is_some() && !args.no_multi
}

fn multi_limit(args: &Args) -> Option<usize> {
    args.multi.flatten()
}

fn cycle_enabled(args: &Args) -> bool {
    args.cycle && !args.no_cycle
}

fn expect_arg(args: &Args) -> Option<&str> {
    (!args.no_expect)
        .then_some(args.expect.as_deref())
        .flatten()
}

fn preview_command(args: &Args) -> Option<String> {
    (!args.no_preview).then_some(args.preview.clone()).flatten()
}

fn header_lines_count(args: &Args) -> usize {
    (!args.no_header_lines)
        .then_some(args.header_lines)
        .flatten()
        .unwrap_or(0)
}

fn split_header_lines(
    mut records: Vec<InputRecord>,
    count: usize,
) -> (Vec<InputRecord>, Vec<InputRecord>) {
    let split_at = count.min(records.len());
    let candidates = records.split_off(split_at);
    (records, candidates)
}

fn header_text(args: &Args, header_records: &[InputRecord]) -> Option<String> {
    let mut lines = Vec::new();
    if !args.no_header {
        if let Some(header) = &args.header {
            lines.push(header.clone());
        }
    }
    if !args.no_header_lines {
        lines.extend(header_records.iter().map(|record| record.display.clone()));
    }

    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn footer_text(args: &Args) -> Option<String> {
    (!args.no_footer).then_some(args.footer.clone()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_binding_invocations_can_run_interactive_with_captured_stdout() {
        let args = Args::parse_from([
            "yuru",
            "--scheme",
            "history",
            "--tac",
            "--no-sort",
            "--no-multi",
        ]);

        assert!(should_run_interactive_with_tty(&args, true));
    }

    #[test]
    fn filter_and_debug_modes_stay_non_interactive() {
        let filter_args = Args::parse_from(["yuru", "--filter", "abc"]);
        let debug_args = Args::parse_from(["yuru", "--debug-query-variants"]);

        assert!(!should_run_interactive_with_tty(&filter_args, true));
        assert!(!should_run_interactive_with_tty(&debug_args, true));
        assert!(!should_run_interactive_with_tty(
            &Args::parse_from(["yuru"]),
            false
        ));
    }

    #[test]
    fn interactive_mode_uses_larger_default_limit() {
        let interactive_args = Args::parse_from(["yuru"]);
        let batch_args = Args::parse_from(["yuru"]);
        let filter_args = Args::parse_from(["yuru", "--filter", "abc"]);

        assert_eq!(
            default_limit(&interactive_args, true),
            DEFAULT_INTERACTIVE_LIMIT
        );
        assert_eq!(default_limit(&batch_args, false), 10);
        assert_eq!(default_limit(&filter_args, false), usize::MAX);
    }

    #[test]
    fn header_lines_are_split_from_candidates() {
        let records = vec![
            InputRecord::from_raw(b"title".to_vec()),
            InputRecord::from_raw(b"alpha".to_vec()),
            InputRecord::from_raw(b"beta".to_vec()),
        ];
        let (headers, candidates) = split_header_lines(records, 1);

        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].display, "title");
        assert_eq!(
            candidates
                .iter()
                .map(|record| record.display.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "beta"]
        );
    }

    #[test]
    fn header_text_combines_explicit_header_and_header_lines() {
        let args = Args::parse_from(["yuru", "--header", "extra", "--header-lines", "2"]);
        let records = vec![
            InputRecord::from_raw(b"first".to_vec()),
            InputRecord::from_raw(b"second".to_vec()),
        ];

        assert_eq!(
            header_text(&args, &records).as_deref(),
            Some("extra\nfirst\nsecond")
        );
    }

    #[test]
    fn streaming_reader_builds_candidates_without_waiting_for_eof_batch() {
        let (sender, receiver) = mpsc::channel();
        let items = Arc::new(Mutex::new(Vec::new()));
        stream_records_from_reader(
            io::Cursor::new(b"alpha\nbeta\n".to_vec()),
            false,
            items.clone(),
            sender,
            FieldConfig {
                delimiter: None,
                nth: None,
                with_nth: None,
                accept_nth: None,
            },
            Arc::new(PlainBackend),
            SearchConfig::default(),
            Vec::new(),
            false,
        )
        .unwrap();

        let messages: Vec<_> = receiver.try_iter().collect();
        assert_eq!(messages.len(), 2);
        match &messages[0] {
            yuru_tui::CandidateStreamMessage::Candidate(candidate) => {
                assert_eq!(candidate.id, 0);
                assert_eq!(candidate.display, "alpha");
            }
            other => panic!("unexpected stream message: {other:?}"),
        }
        match &messages[1] {
            yuru_tui::CandidateStreamMessage::Candidate(candidate) => {
                assert_eq!(candidate.id, 1);
                assert_eq!(candidate.display, "beta");
            }
            other => panic!("unexpected stream message: {other:?}"),
        }
        let items = items.lock().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].original, "alpha");
        assert_eq!(items[1].original, "beta");
    }

    #[test]
    fn configless_preparse_only_matches_leading_subcommands() {
        assert!(configless_command_present(&[OsString::from("doctor")]));
        assert!(configless_command_present(&[OsString::from("configure")]));
        assert!(configless_command_present(&[OsString::from(
            "__split-shell-words"
        )]));
        assert!(!configless_command_present(&[
            OsString::from("--filter"),
            OsString::from("doctor")
        ]));
    }

    #[test]
    fn shell_config_from_toml_overrides_generated_defaults() {
        let value = r#"
[shell]
bindings = "ctrl-t,ctrl-r"
ctrl_t_command = "__yuru_compgen_path__ ."
ctrl_t_opts = "--preview 'cat {}'"
alt_c_command = "__yuru_compgen_dir__ ."
alt_c_opts = "--preview 'ls {}'"
"#
        .parse::<toml::Value>()
        .unwrap();

        let config = shell_config_from_value(&value);

        assert_eq!(config.bindings, "ctrl-t,ctrl-r");
        assert_eq!(config.ctrl_t_command, "__yuru_compgen_path__ .");
        assert_eq!(config.ctrl_t_opts, "--preview 'cat {}'");
        assert_eq!(config.alt_c_command, "__yuru_compgen_dir__ .");
        assert_eq!(config.alt_c_opts, "--preview 'ls {}'");
        assert!(shell_config_prefix(ShellKind::Zsh, &config).contains("YURU_SHELL_BINDINGS"));
        assert!(shell_config_prefix(ShellKind::Fish, &config)
            .contains("set -gx YURU_CTRL_T_OPTS \"--preview 'cat {}'\""));
    }

    #[test]
    fn generated_shell_scripts_avoid_eval_for_completion_paths_and_option_parsing() {
        for kind in [ShellKind::Bash, ShellKind::Zsh, ShellKind::Fish] {
            let script = shell::script(kind);
            assert!(!script.contains("eval \"base=$base\""));
            assert!(!script.contains("eval \"opt_args=($opts)\""));
            assert!(!script.contains("eval \"set opts $raw\""));
        }
    }

    #[test]
    fn hidden_shell_word_splitter_accepts_hyphen_values() {
        let args = Args::parse_from([
            "yuru",
            "__split-shell-words",
            "--preview 'file {}' --bind ctrl-j:preview-down",
        ]);

        match args.command {
            Some(CommandArg::SplitShellWords { words }) => {
                assert_eq!(
                    parse_shell_words(&words).unwrap(),
                    vec!["--preview", "file {}", "--bind", "ctrl-j:preview-down"]
                );
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn preview_scroll_bind_actions_are_supported() {
        let binding = parse_supported_binding("ctrl-j:preview-down").unwrap();

        assert_eq!(binding.key, "ctrl-j");
        assert_eq!(binding.action, yuru_tui::BindingAction::PreviewDown);
        assert!(!has_unsupported_bindings(&[
            "ctrl-k:preview-up,ctrl-j:preview-down".to_string(),
            "ctrl-b:preview-page-up,ctrl-f:preview-page-down".to_string(),
            "home:preview-top,end:preview-bottom".to_string(),
        ]));
    }

    #[test]
    fn common_navigation_bind_actions_are_supported() {
        assert!(!has_unsupported_bindings(&[
            "ctrl-k:up,ctrl-j:down".to_string(),
            "home:first,end:last".to_string(),
            "pgup:page-up,pgdn:page-down".to_string(),
            "tab:toggle+down,btab:toggle+up".to_string(),
            "ctrl-a:beginning-of-line,ctrl-e:end-of-line".to_string(),
            "ctrl-h:backward-delete-char,del:delete-char".to_string(),
        ]));
        assert_eq!(normalize_binding_key("btab"), "shift-tab");
        assert_eq!(normalize_binding_key("pgdn"), "page-down");
    }

    #[test]
    fn parses_representative_fzf_option_surface() {
        let args = Args::parse_from([
            "yuru",
            "--extended-exact",
            "--no-exact",
            "--literal",
            "--no-literal",
            "--algo",
            "v2",
            "--scheme",
            "path",
            "--expect",
            "ctrl-y,alt-enter",
            "--no-expect",
            "--enabled",
            "--disabled",
            "--phony",
            "--no-phony",
            "--bind",
            "ctrl-j:preview-down",
            "--toggle-sort",
            "ctrl-s",
            "--color",
            "hl:#00ff00,hl+:#00aa00,pointer:#ff0000",
            "--no-color",
            "--no-256",
            "--black",
            "--no-black",
            "--bold",
            "--no-bold",
            "--layout",
            "reverse-list",
            "--reverse",
            "--no-reverse",
            "--cycle",
            "--no-cycle",
            "--highlight-line",
            "--no-highlight-line",
            "--wrap=word",
            "--no-wrap",
            "--wrap-word",
            "--no-wrap-word",
            "--wrap-sign",
            ">",
            "--multi-line",
            "--no-multi-line",
            "--raw",
            "--no-raw",
            "--track",
            "--no-track",
            "--id-nth",
            "1",
            "--no-id-nth",
            "--tac",
            "--no-tac",
            "--tail",
            "10",
            "--no-tail",
            "--ansi",
            "--no-ansi",
            "--read0",
            "--no-read0",
            "--print0",
            "--no-print0",
            "--print-query",
            "--no-print-query",
            "--select-1",
            "--no-select-1",
            "--exit-0",
            "--no-exit-0",
            "--sync",
            "--async",
            "--multi=3",
            "--multi",
            "4",
            "-m5",
            "--no-multi",
            "-x",
            "--preview",
            "cat {}",
            "--no-preview",
            "--preview-window",
            "right,60%,wrap",
            "--preview-border=rounded",
            "--no-preview-border",
            "--preview-label",
            "preview",
            "--preview-label-pos",
            "2",
            "--preview-wrap-sign",
            ">",
            "--height",
            "40%",
            "--min-height",
            "10",
            "--no-height",
            "--popup=center,50%",
            "--no-popup",
            "--tmux=center,50%",
            "--no-tmux",
            "--margin",
            "1,2",
            "--no-margin",
            "--padding",
            "1",
            "--no-padding",
            "--border=rounded",
            "--no-border",
            "--border-label",
            "yuru",
            "--border-label-pos",
            "center",
            "--no-border-label",
            "--header",
            "head",
            "--no-header",
            "--header-lines",
            "1",
            "--no-header-lines",
            "--header-first",
            "--no-header-first",
            "--header-border=rounded",
            "--no-header-border",
            "--header-lines-border=inline",
            "--no-header-lines-border",
            "--header-label",
            "h",
            "--header-label-pos",
            "1",
            "--no-header-label",
            "--footer",
            "foot",
            "--no-footer",
            "--footer-border=rounded",
            "--no-footer-border",
            "--footer-label",
            "f",
            "--footer-label-pos",
            "1",
            "--no-footer-label",
            "--gap=2",
            "--no-gap",
            "--gap-line",
            "-",
            "--no-gap-line",
            "--freeze-left",
            "1",
            "--freeze-right",
            "1",
            "--keep-right",
            "--no-keep-right",
            "--scroll-off",
            "2",
            "--hscroll",
            "--no-hscroll",
            "--hscroll-off",
            "3",
            "--jump-labels",
            "asdf",
            "--gutter",
            "|",
            "--gutter-raw",
            ":",
            "--pointer",
            ">",
            "--marker",
            "*",
            "--marker-multi-line",
            "|||",
            "--ellipsis",
            "..",
            "--tabstop",
            "4",
            "--scrollbar",
            "|",
            "--no-scrollbar",
            "--list-border=rounded",
            "--no-list-border",
            "--list-label",
            "list",
            "--list-label-pos",
            "1",
            "--no-list-label",
            "--no-input",
            "--prompt",
            "> ",
            "--info",
            "inline",
            "--info-command",
            "echo info",
            "--no-info-command",
            "--no-info",
            "--inline-info",
            "--no-inline-info",
            "--separator",
            "-",
            "--no-separator",
            "--ghost",
            "type",
            "--filepath-word",
            "--no-filepath-word",
            "--input-border=rounded",
            "--no-input-border",
            "--input-label",
            "input",
            "--input-label-pos",
            "1",
            "--no-input-label",
            "--style",
            "full",
            "--with-shell",
            "sh -c",
            "--listen=localhost:0",
            "--no-listen",
            "--listen-unsafe=localhost:0",
            "--no-listen-unsafe",
            "--history",
            "hist.txt",
            "--no-history",
            "--history-size",
            "100",
            "--tty-default",
            "/dev/tty",
            "--no-tty-default",
            "--force-tty-in",
            "--no-force-tty-in",
            "--proxy-script",
            "proxy",
            "--no-winpty",
            "--no-mouse",
            "--unicode",
            "--no-unicode",
            "--ambidouble",
            "--no-ambidouble",
            "--clear",
            "--no-clear",
            "--threads",
            "2",
            "--bench",
            "1s",
            "--profile-cpu",
            "cpu.prof",
            "--profile-mem",
            "mem.prof",
            "--profile-block",
            "block.prof",
            "--profile-mutex",
            "mutex.prof",
        ]);

        assert!(accepted_fzf_option_count(&args) > 0);
    }
}

fn parse_tui_height(args: &Args) -> Option<usize> {
    if args.no_height {
        return None;
    }
    args.height
        .as_deref()
        .and_then(|height| height.parse().ok())
        .filter(|height| *height > 0)
}

fn parse_tui_layout(args: &Args) -> Result<yuru_tui::TuiLayout> {
    if args.reverse {
        return Ok(yuru_tui::TuiLayout::Reverse);
    }

    match args.layout.as_deref().unwrap_or("default") {
        "default" => Ok(yuru_tui::TuiLayout::Default),
        "reverse" => Ok(yuru_tui::TuiLayout::Reverse),
        "reverse-list" => Ok(yuru_tui::TuiLayout::ReverseList),
        other => bail!("unsupported --layout value: {other}"),
    }
}

fn parse_tui_style(raw: &[Option<String>]) -> yuru_tui::TuiStyle {
    let mut style = yuru_tui::TuiStyle::default();
    for color_set in raw.iter().flatten() {
        for entry in color_set.split(',') {
            let Some((name, value)) = entry.split_once(':') else {
                continue;
            };
            let Some(color) = parse_hex_color(value) else {
                continue;
            };
            match name {
                "pointer" => style.pointer = Some(color),
                "hl" => style.highlight = Some(color),
                "hl+" => style.highlight_selected = Some(color),
                _ => {}
            }
        }
    }
    style
}

fn first_line(value: &str) -> String {
    value.lines().next().unwrap_or_default().to_string()
}

fn parse_hex_color(value: &str) -> Option<yuru_tui::TuiRgb> {
    let value = value.strip_prefix('#')?;
    if value.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&value[0..2], 16).ok()?;
    let g = u8::from_str_radix(&value[2..4], 16).ok()?;
    let b = u8::from_str_radix(&value[4..6], 16).ok()?;
    Some(yuru_tui::TuiRgb { r, g, b })
}

fn parse_expect_keys(raw: Option<&str>) -> Vec<String> {
    raw.into_iter()
        .flat_map(|keys| keys.split(','))
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(|key| key.to_ascii_lowercase())
        .collect()
}

fn parse_bindings(raw: &[String]) -> Vec<yuru_tui::KeyBinding> {
    raw.iter()
        .flat_map(|bindings| bindings.split(','))
        .filter_map(parse_supported_binding)
        .collect()
}

fn parse_supported_binding(raw: &str) -> Option<yuru_tui::KeyBinding> {
    let (key, action) = raw.split_once(':')?;
    let action = match action.trim() {
        "accept" => yuru_tui::BindingAction::Accept,
        "abort" => yuru_tui::BindingAction::Abort,
        "clear-query" | "clear" | "unix-line-discard" => yuru_tui::BindingAction::ClearQuery,
        "up" | "previous" => yuru_tui::BindingAction::MoveSelectionUp,
        "down" | "next" => yuru_tui::BindingAction::MoveSelectionDown,
        "first" | "top" => yuru_tui::BindingAction::MoveSelectionFirst,
        "last" | "bottom" => yuru_tui::BindingAction::MoveSelectionLast,
        "page-up" => yuru_tui::BindingAction::PageUp,
        "page-down" => yuru_tui::BindingAction::PageDown,
        "toggle" => yuru_tui::BindingAction::ToggleMark,
        "toggle+down" => yuru_tui::BindingAction::ToggleMarkAndDown,
        "toggle+up" => yuru_tui::BindingAction::ToggleMarkAndUp,
        "beginning-of-line" => yuru_tui::BindingAction::MoveCursorStart,
        "end-of-line" => yuru_tui::BindingAction::MoveCursorEnd,
        "backward-char" => yuru_tui::BindingAction::MoveCursorLeft,
        "forward-char" => yuru_tui::BindingAction::MoveCursorRight,
        "backward-delete-char" => yuru_tui::BindingAction::Backspace,
        "delete-char" => yuru_tui::BindingAction::Delete,
        "preview-up" => yuru_tui::BindingAction::PreviewUp,
        "preview-down" => yuru_tui::BindingAction::PreviewDown,
        "preview-page-up" => yuru_tui::BindingAction::PreviewPageUp,
        "preview-page-down" => yuru_tui::BindingAction::PreviewPageDown,
        "preview-top" => yuru_tui::BindingAction::PreviewTop,
        "preview-bottom" => yuru_tui::BindingAction::PreviewBottom,
        _ => return None,
    };

    Some(yuru_tui::KeyBinding {
        key: normalize_binding_key(key),
        action,
    })
}

fn normalize_binding_key(key: &str) -> String {
    match key.trim().to_ascii_lowercase().as_str() {
        "btab" => "shift-tab".to_string(),
        "pgup" => "page-up".to_string(),
        "pgdn" => "page-down".to_string(),
        other => other.to_string(),
    }
}

fn has_unsupported_bindings(raw: &[String]) -> bool {
    raw.iter()
        .flat_map(|bindings| bindings.split(','))
        .map(str::trim)
        .filter(|binding| !binding.is_empty())
        .any(|binding| parse_supported_binding(binding).is_none())
}

fn enforce_fzf_compat(args: &Args) -> Result<()> {
    let _ = accepted_fzf_option_count(args);
    let mode = effective_fzf_compat(args)?;
    let ignored = ignored_fzf_options(args);
    if ignored.is_empty() || mode == FzfCompatArg::Ignore {
        return Ok(());
    }

    match mode {
        FzfCompatArg::Strict => {
            bail!(
                "unsupported fzf option(s): {}. Use --fzf-compat=warn or --fzf-compat=ignore to allow them",
                ignored.join(", ")
            );
        }
        FzfCompatArg::Warn => {
            for option in ignored {
                eprintln!("yuru: warning: ignoring unsupported fzf option {option}");
            }
        }
        FzfCompatArg::Ignore => {}
    }

    Ok(())
}

fn accepted_fzf_option_count(args: &Args) -> usize {
    macro_rules! count_bool {
        ($($field:ident),* $(,)?) => {
            0 $(+ usize::from(args.$field))*
        };
    }
    macro_rules! count_opt {
        ($($field:ident),* $(,)?) => {
            0 $(+ usize::from(args.$field.is_some()))*
        };
    }

    count_bool!(
        no_exact,
        extended_exact,
        no_extended,
        ignore_case,
        no_ignore_case,
        smart_case,
        no_sort,
        disabled,
        phony,
        enabled,
        no_phony,
        literal,
        no_literal,
        tac,
        no_tac,
        no_tail,
        read0,
        no_read0,
        sync,
        no_sync,
        print0,
        no_print0,
        ansi,
        no_ansi,
        print_query,
        no_print_query,
        select_1,
        no_select_1,
        exit_0,
        no_exit_0,
        no_multi,
        no_expect,
        no_preview,
        no_preview_border,
        no_height,
        no_popup,
        no_tmux,
        reverse,
        no_reverse,
        no_margin,
        no_padding,
        no_border,
        no_border_label,
        no_header,
        no_header_lines,
        header_first,
        no_header_first,
        no_header_border,
        no_header_lines_border,
        no_header_label,
        no_footer,
        no_footer_border,
        no_footer_label,
        no_color,
        no_256,
        bold,
        no_bold,
        black,
        no_black,
        cycle,
        no_cycle,
        highlight_line,
        no_highlight_line,
        no_wrap,
        wrap_word,
        no_wrap_word,
        multi_line,
        no_multi_line,
        raw,
        no_raw,
        track,
        no_track,
        no_id_nth,
        no_gap,
        no_gap_line,
        keep_right,
        no_keep_right,
        no_hscroll,
        hscroll,
        no_scrollbar,
        no_list_border,
        no_list_label,
        no_input,
        no_info_command,
        no_info,
        inline_info,
        no_inline_info,
        no_separator,
        filepath_word,
        no_filepath_word,
        no_input_border,
        no_input_label,
        no_listen,
        no_listen_unsafe,
        no_history,
        no_tty_default,
        force_tty_in,
        no_force_tty_in,
        no_winpty,
        no_mouse,
        no_unicode,
        unicode,
        ambidouble,
        no_ambidouble,
        clear,
        no_clear,
        man,
    ) + count_opt!(
        sort,
        tail,
        expect,
        toggle_sort,
        preview,
        preview_window,
        preview_border,
        preview_label,
        preview_label_pos,
        preview_wrap_sign,
        height,
        min_height,
        popup,
        tmux,
        layout,
        margin,
        padding,
        border,
        border_label,
        border_label_pos,
        prompt,
        header,
        header_lines,
        header_border,
        header_lines_border,
        header_label,
        header_label_pos,
        footer,
        footer_border,
        footer_label,
        footer_label_pos,
        wrap,
        wrap_sign,
        id_nth,
        gap,
        gap_line,
        freeze_left,
        freeze_right,
        scroll_off,
        hscroll_off,
        jump_labels,
        gutter,
        gutter_raw,
        pointer,
        marker,
        marker_multi_line,
        ellipsis,
        tabstop,
        scrollbar,
        list_border,
        list_label,
        list_label_pos,
        info,
        info_command,
        separator,
        ghost,
        input_border,
        input_label,
        input_label_pos,
        with_shell,
        style,
        listen,
        listen_unsafe,
        history,
        history_size,
        tty_default,
        proxy_script,
        threads,
        bench,
        profile_cpu,
        profile_mem,
        profile_block,
        profile_mutex,
    ) + args.bind.len()
        + args.color.len()
}

fn effective_fzf_compat(args: &Args) -> Result<FzfCompatArg> {
    if let Some(mode) = args.fzf_compat {
        return Ok(mode);
    }

    match std::env::var("YURU_FZF_COMPAT") {
        Ok(value) => parse_fzf_compat_env(&value),
        Err(std::env::VarError::NotPresent) => Ok(FzfCompatArg::Warn),
        Err(error) => Err(error).context("failed to read YURU_FZF_COMPAT"),
    }
}

fn parse_fzf_compat_env(value: &str) -> Result<FzfCompatArg> {
    match value.trim() {
        "strict" => Ok(FzfCompatArg::Strict),
        "warn" => Ok(FzfCompatArg::Warn),
        "ignore" => Ok(FzfCompatArg::Ignore),
        other => bail!("unsupported YURU_FZF_COMPAT value: {other}"),
    }
}

fn ignored_fzf_options(args: &Args) -> Vec<&'static str> {
    let mut out = Vec::new();

    if has_unsupported_bindings(&args.bind) {
        out.push("--bind");
    }
    out
}

fn read_input_candidates(args: &Args, walker_requested: bool) -> Result<Vec<InputRecord>> {
    if let Some(path) = &args.input {
        return read_file_candidates(path, read0_enabled(args));
    }

    let stdin_is_terminal = io::stdin().is_terminal();
    let stdin_items = if stdin_is_terminal {
        Vec::new()
    } else {
        read_stdin_candidates(read0_enabled(args))?
    };
    if !stdin_items.is_empty() {
        return Ok(stdin_items);
    }

    if walker_requested {
        return run_walker(args);
    }

    if let Some((env_name, command)) = default_source_command() {
        if !command.trim().is_empty() {
            return run_default_command(env_name, &command, read0_enabled(args));
        }
    }

    if !stdin_is_terminal {
        return Ok(stdin_items);
    }

    run_walker(args)
}

fn read_stdin_candidates(read0: bool) -> Result<Vec<InputRecord>> {
    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input)?;
    Ok(parse_candidate_bytes(&input, read0))
}

fn read_file_candidates(path: &Path, read0: bool) -> Result<Vec<InputRecord>> {
    let input =
        fs::read(path).with_context(|| format!("failed to read input file {}", path.display()))?;
    Ok(parse_candidate_bytes(&input, read0))
}

fn parse_candidate_bytes(input: &[u8], read0: bool) -> Vec<InputRecord> {
    if read0 {
        input
            .split(|byte| *byte == b'\0')
            .filter(|item| !item.is_empty())
            .map(|item| InputRecord::from_raw(item.to_vec()))
            .collect()
    } else {
        parse_line_records(input)
    }
}

fn parse_line_records(input: &[u8]) -> Vec<InputRecord> {
    if input.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut start = 0usize;
    for (index, byte) in input.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        out.push(InputRecord::from_raw(
            trim_trailing_cr(&input[start..index]).to_vec(),
        ));
        start = index + 1;
    }
    if start < input.len() {
        out.push(InputRecord::from_raw(
            trim_trailing_cr(&input[start..]).to_vec(),
        ));
    }
    out
}

fn trim_trailing_cr(input: &[u8]) -> &[u8] {
    input.strip_suffix(b"\r").unwrap_or(input)
}

fn default_source_command() -> Option<(&'static str, String)> {
    for env_name in ["YURU_DEFAULT_COMMAND", "FZF_DEFAULT_COMMAND"] {
        if let Ok(command) = std::env::var(env_name) {
            return Some((env_name, command));
        }
    }
    None
}

fn non_empty_default_source_command() -> Option<(&'static str, String)> {
    default_source_command().filter(|(_, command)| !command.trim().is_empty())
}

fn run_default_command(env_name: &str, command: &str, read0: bool) -> Result<Vec<InputRecord>> {
    let output = default_command_process(command)
        .output()
        .with_context(|| format!("failed to run {env_name}: {command}"))?;

    if !output.status.success() {
        bail!("{env_name} exited with {}", output.status);
    }

    Ok(parse_candidate_bytes(&output.stdout, read0))
}

#[cfg(not(windows))]
fn default_command_process(command: &str) -> std::process::Command {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let mut process = std::process::Command::new(shell);
    process.arg("-c").arg(command);
    process
}

#[cfg(windows)]
fn default_command_process(command: &str) -> std::process::Command {
    let shell =
        std::env::var("YURU_WINDOWS_SHELL").unwrap_or_else(|_| "powershell.exe".to_string());
    let mut process = std::process::Command::new(shell);
    process
        .arg("-NoLogo")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(command);
    process
}

#[derive(Clone, Copy, Debug)]
struct WalkerOptions {
    files: bool,
    dirs: bool,
    follow: bool,
    hidden: bool,
}

fn run_walker(args: &Args) -> Result<Vec<InputRecord>> {
    let options = parse_walker_options(&args.walker)?;
    let skips = parse_walker_skip(&args.walker_skip);
    let mut out = Vec::new();

    for root in &args.walker_roots {
        let mut builder = WalkBuilder::new(root);
        builder
            .follow_links(options.follow)
            .hidden(!options.hidden)
            .ignore(true)
            .git_ignore(true)
            .git_global(true)
            .parents(true)
            .require_git(false);
        let skips = skips.clone();
        builder.filter_entry(move |entry| walker_entry_allowed(entry, &skips, options.hidden));

        for entry in builder.build() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) if walker_error_is_skippable(&error) => continue,
                Err(error) => return Err(error.into()),
            };
            if entry.depth() == 0 {
                continue;
            }

            let Some(file_type) = entry.file_type() else {
                continue;
            };
            let include =
                file_type.is_file() && options.files || file_type.is_dir() && options.dirs;
            if include {
                out.push(InputRecord::from_raw(
                    display_walked_path(root, entry.path()).into_bytes(),
                ));
            }
        }
    }

    Ok(out)
}

fn walker_error_is_skippable(error: &ignore::Error) -> bool {
    if ignore_error_is_loop(error) {
        return true;
    }

    error.io_error().is_some_and(|io_error| {
        matches!(
            io_error.kind(),
            io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
        )
    })
}

fn ignore_error_is_loop(error: &ignore::Error) -> bool {
    match error {
        ignore::Error::Loop { .. } => true,
        ignore::Error::Partial(errors) => errors.iter().any(ignore_error_is_loop),
        ignore::Error::WithLineNumber { err, .. }
        | ignore::Error::WithPath { err, .. }
        | ignore::Error::WithDepth { err, .. } => ignore_error_is_loop(err),
        _ => false,
    }
}

fn parse_walker_options(raw: &str) -> Result<WalkerOptions> {
    let mut options = WalkerOptions {
        files: false,
        dirs: false,
        follow: false,
        hidden: false,
    };

    for part in raw
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match part {
            "file" => options.files = true,
            "dir" => options.dirs = true,
            "follow" => options.follow = true,
            "hidden" => options.hidden = true,
            other => bail!("unknown walker option: {other}"),
        }
    }

    Ok(options)
}

fn parse_walker_skip(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn walker_entry_allowed(entry: &DirEntry, skips: &[String], include_hidden: bool) -> bool {
    if entry.depth() == 0 {
        return true;
    }

    let name = entry.file_name().to_string_lossy();
    if skips.iter().any(|skip| skip == name.as_ref()) {
        return false;
    }
    include_hidden || !name.starts_with('.')
}

fn display_walked_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    if root == Path::new(".") {
        relative.display().to_string()
    } else {
        root.join(relative).display().to_string()
    }
}

fn apply_aliases(
    candidates: &mut [yuru_core::Candidate],
    items: &[InputItem],
    aliases: &[String],
    config: &SearchConfig,
) -> Result<()> {
    for alias in aliases {
        let Some((query, display)) = alias.split_once('=') else {
            bail!("alias must use query=display format: {alias}");
        };
        if let Some(candidate) = candidates.iter_mut().find(|candidate| {
            let item = &items[candidate.id];
            item.original == display || item.display == display || candidate.display == display
        }) {
            candidate.keys.push(SearchKey::learned_alias(query));
            candidate.keys = dedup_and_limit_keys(std::mem::take(&mut candidate.keys), config);
        }
    }
    Ok(())
}

fn apply_aliases_to_candidate(
    candidate: &mut yuru_core::Candidate,
    item: &InputItem,
    aliases: &[String],
    config: &SearchConfig,
) -> Result<()> {
    for alias in aliases {
        let Some((query, display)) = alias.split_once('=') else {
            bail!("alias must use query=display format: {alias}");
        };
        if item.original == display || item.display == display || candidate.display == display {
            candidate.keys.push(SearchKey::learned_alias(query));
        }
    }
    candidate.keys = dedup_and_limit_keys(std::mem::take(&mut candidate.keys), config);
    Ok(())
}

fn effective_query(args: &Args) -> String {
    args.filter
        .as_ref()
        .or(args.query.as_ref())
        .cloned()
        .unwrap_or_default()
}

fn case_sensitive(query: &str, args: &Args) -> bool {
    if args.ignore_case {
        return false;
    }
    if args.no_ignore_case {
        return true;
    }
    args.smart_case && query.chars().any(char::is_uppercase)
}

fn parse_tiebreaks(args: &Args) -> Result<Vec<Tiebreak>> {
    let raw = match args.scheme {
        SchemeArg::Default => args.tiebreak.as_str(),
        SchemeArg::Path if args.tiebreak == "length" => "pathname,length",
        SchemeArg::Path => args.tiebreak.as_str(),
        SchemeArg::History if args.tiebreak == "length" => "index",
        SchemeArg::History => args.tiebreak.as_str(),
    };

    let mut out = Vec::new();
    for part in raw
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let criterion = match part {
            "length" => Tiebreak::Length,
            "chunk" => Tiebreak::Chunk,
            "pathname" => Tiebreak::Pathname,
            "begin" => Tiebreak::Begin,
            "end" => Tiebreak::End,
            "index" => Tiebreak::Index,
            other => bail!("unknown --tiebreak criterion: {other}"),
        };
        if out.contains(&criterion) {
            bail!("duplicate --tiebreak criterion: {part}");
        }
        if out.contains(&Tiebreak::Index) {
            bail!("--tiebreak=index is only allowed at the end");
        }
        out.push(criterion);
    }

    if out.is_empty() {
        out.push(Tiebreak::Length);
    }
    if !out.contains(&Tiebreak::Index) {
        out.push(Tiebreak::Index);
    }
    Ok(out)
}

fn print_query_variants(
    query: &str,
    backend: &dyn LanguageBackend,
    config: &SearchConfig,
    print0: bool,
) -> Result<()> {
    let variants = dedup_and_limit_variants(backend.expand_query(query), config.max_query_variants);
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
        b"\n".as_slice()
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
        stdout.write_all(b"\n")?;

        let matched = explain_match(query, result, candidates, backend, config);
        writeln!(stdout, "  score: {}", result.score)?;
        writeln!(stdout, "  matched key: {:?}", result.key_kind)?;
        if let Some(matched) = matched {
            writeln!(stdout, "  query: {}", matched.pattern)?;
            writeln!(stdout, "  matched text: {}", matched.variant.text)?;
            writeln!(
                stdout,
                "  key span: {}..{}",
                matched.key_span.start, matched.key_span.end
            )?;
            writeln!(stdout, "  key text: {}", matched.key_text)?;
            match matched.source_span {
                Some(span) => {
                    let snippet = char_slice(&result.display, span.start, span.end);
                    writeln!(
                        stdout,
                        "  source span: {}..{} \"{}\"",
                        span.start, span.end, snippet
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
        let variants =
            dedup_and_limit_variants(backend.expand_query(&pattern), config.max_query_variants);
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
    Some(SourceSpan { start, end })
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
        start = start.min(span.start);
        end = end.max(span.end);
        found = true;
    }

    found.then_some(SourceSpan { start, end })
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

#[derive(Clone, Debug)]
struct ShellConfigDefaults {
    bindings: String,
    ctrl_t_command: String,
    ctrl_t_opts: String,
    alt_c_command: String,
    alt_c_opts: String,
}

impl Default for ShellConfigDefaults {
    fn default() -> Self {
        Self {
            bindings: "all".to_string(),
            ctrl_t_command: default_ctrl_t_command().to_string(),
            ctrl_t_opts: default_ctrl_t_opts().to_string(),
            alt_c_command: default_alt_c_command().to_string(),
            alt_c_opts: default_alt_c_opts().to_string(),
        }
    }
}

fn default_ctrl_t_command() -> &'static str {
    #[cfg(windows)]
    {
        "Get-YuruPathItems ."
    }
    #[cfg(not(windows))]
    {
        "__yuru_compgen_path__ ."
    }
}

fn default_ctrl_t_opts() -> &'static str {
    #[cfg(windows)]
    {
        "--preview 'Get-Item -LiteralPath {} | Format-List | Out-String'"
    }
    #[cfg(not(windows))]
    {
        "--preview 'file {}'"
    }
}

fn default_alt_c_command() -> &'static str {
    #[cfg(windows)]
    {
        "Get-YuruDirItems ."
    }
    #[cfg(not(windows))]
    {
        "__yuru_compgen_dir__ ."
    }
}

fn default_alt_c_opts() -> &'static str {
    #[cfg(windows)]
    {
        "--preview 'Get-ChildItem -Force -LiteralPath {} | Select-Object -First 100 | Out-String'"
    }
    #[cfg(not(windows))]
    {
        "--preview 'ls -la {} 2>/dev/null | head -100'"
    }
}

fn print_shell_script(kind: ShellKind) -> Result<()> {
    let config = shell_config_defaults().unwrap_or_else(|error| {
        eprintln!("yuru: warning: failed to load shell config defaults: {error:#}");
        ShellConfigDefaults::default()
    });
    print!("{}", shell_config_prefix(kind, &config));
    print!("{}", shell::script(kind));
    Ok(())
}

fn shell_config_defaults() -> Result<ShellConfigDefaults> {
    let mut defaults = ShellConfigDefaults::default();
    let Some(ConfigSource::Toml(path)) = yuru_config_source() else {
        return Ok(defaults);
    };
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let value = content
        .parse::<toml::Value>()
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if let Some(shell) = value.get("shell") {
        if let Some(bindings) = shell.get("bindings").and_then(toml::Value::as_str) {
            defaults.bindings = bindings.to_string();
        }
        if let Some(command) = shell.get("ctrl_t_command").and_then(toml::Value::as_str) {
            defaults.ctrl_t_command = command.to_string();
        }
        if let Some(opts) = shell.get("ctrl_t_opts").and_then(toml::Value::as_str) {
            defaults.ctrl_t_opts = opts.to_string();
        }
        if let Some(command) = shell.get("alt_c_command").and_then(toml::Value::as_str) {
            defaults.alt_c_command = command.to_string();
        }
        if let Some(opts) = shell.get("alt_c_opts").and_then(toml::Value::as_str) {
            defaults.alt_c_opts = opts.to_string();
        }
    }
    Ok(defaults)
}

fn shell_config_prefix(kind: ShellKind, config: &ShellConfigDefaults) -> String {
    match kind {
        ShellKind::Bash | ShellKind::Zsh => format!(
            "# yuru config defaults\n\
             if [ -z \"${{YURU_SHELL_BINDINGS+x}}\" ]; then export YURU_SHELL_BINDINGS={}; fi\n\
             if [ -z \"${{YURU_CTRL_T_COMMAND+x}}\" ]; then export YURU_CTRL_T_COMMAND={}; fi\n\
             if [ -z \"${{YURU_CTRL_T_OPTS+x}}\" ]; then export YURU_CTRL_T_OPTS={}; fi\n\
             if [ -z \"${{YURU_ALT_C_COMMAND+x}}\" ]; then export YURU_ALT_C_COMMAND={}; fi\n\
             if [ -z \"${{YURU_ALT_C_OPTS+x}}\" ]; then export YURU_ALT_C_OPTS={}; fi\n\n",
            sh_quote(&config.bindings),
            sh_quote(&config.ctrl_t_command),
            sh_quote(&config.ctrl_t_opts),
            sh_quote(&config.alt_c_command),
            sh_quote(&config.alt_c_opts)
        ),
        ShellKind::Fish => format!(
            "# yuru config defaults\n\
             if not set -q YURU_SHELL_BINDINGS\n  set -gx YURU_SHELL_BINDINGS {}\nend\n\
             if not set -q YURU_CTRL_T_COMMAND\n  set -gx YURU_CTRL_T_COMMAND {}\nend\n\
             if not set -q YURU_CTRL_T_OPTS\n  set -gx YURU_CTRL_T_OPTS {}\nend\n\
             if not set -q YURU_ALT_C_COMMAND\n  set -gx YURU_ALT_C_COMMAND {}\nend\n\
             if not set -q YURU_ALT_C_OPTS\n  set -gx YURU_ALT_C_OPTS {}\nend\n\n",
            fish_quote(&config.bindings),
            fish_quote(&config.ctrl_t_command),
            fish_quote(&config.ctrl_t_opts),
            fish_quote(&config.alt_c_command),
            fish_quote(&config.alt_c_opts)
        ),
        ShellKind::PowerShell => format!(
            "# yuru config defaults\n\
             if (-not $env:YURU_SHELL_BINDINGS) {{ $env:YURU_SHELL_BINDINGS = {} }}\n\
             if (-not $env:YURU_CTRL_T_COMMAND) {{ $env:YURU_CTRL_T_COMMAND = {} }}\n\
             if (-not $env:YURU_CTRL_T_OPTS) {{ $env:YURU_CTRL_T_OPTS = {} }}\n\
             if (-not $env:YURU_ALT_C_COMMAND) {{ $env:YURU_ALT_C_COMMAND = {} }}\n\
             if (-not $env:YURU_ALT_C_OPTS) {{ $env:YURU_ALT_C_OPTS = {} }}\n\n",
            ps_quote(&config.bindings),
            ps_quote(&config.ctrl_t_command),
            ps_quote(&config.ctrl_t_opts),
            ps_quote(&config.alt_c_command),
            ps_quote(&config.alt_c_opts)
        ),
    }
}

fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn fish_quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('\n', "\\n")
    )
}

fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn configure_interactive() -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("yuru configure requires an interactive terminal");
    }

    let path = config_path_for_write();
    let mut value = read_config_for_write(&path)?;
    let current_lang = config_string(&value, &["defaults", "lang"])
        .or_else(|| doctor_default_lang(yuru_config_source().as_ref()))
        .unwrap_or_else(|| "ja".to_string());
    let current_load_fzf = config_string(&value, &["defaults", "load_fzf_defaults"])
        .or_else(|| {
            config_bool(&value, &["fzf", "safe_default_opts"]).map(|safe| {
                if safe {
                    "safe".to_string()
                } else {
                    "all".to_string()
                }
            })
        })
        .unwrap_or_else(|| "safe".to_string());
    let current_fzf_compat = config_string(&value, &["defaults", "fzf_compat"])
        .or_else(|| config_string(&value, &["fzf", "unsupported_options"]))
        .unwrap_or_else(|| "warn".to_string());
    let current_shell = shell_config_from_value(&value);

    println!("Yuru configure");
    println!("Config: {}", path.display());
    println!("Press Enter to keep the shown default.");

    let lang = prompt_choice(
        "Default language",
        &current_lang,
        &["plain", "ja", "zh", "auto", "none"],
    )?;
    let load_fzf_defaults = prompt_choice(
        "Load FZF_DEFAULT_OPTS",
        &current_load_fzf,
        &["never", "safe", "all"],
    )?;
    let fzf_compat = prompt_choice(
        "Unsupported fzf options",
        &current_fzf_compat,
        &["strict", "warn", "ignore"],
    )?;
    let bindings = prompt_bindings_value(&current_shell.bindings)?;
    let ctrl_t_command = prompt_string(
        "CTRL-T command",
        &current_shell.ctrl_t_command,
        "Use 'none' to disable this binding's candidate command.",
    )?;
    let ctrl_t_opts = prompt_string(
        "CTRL-T options",
        &current_shell.ctrl_t_opts,
        "Use 'none' to disable extra options such as preview.",
    )?;
    let alt_c_command = prompt_string(
        "ALT-C command",
        &current_shell.alt_c_command,
        "Use 'none' to disable this binding's candidate command.",
    )?;
    let alt_c_opts = prompt_string(
        "ALT-C options",
        &current_shell.alt_c_opts,
        "Use 'none' to disable extra options such as preview.",
    )?;

    {
        let defaults = ensure_toml_table(&mut value, "defaults");
        if lang == "none" {
            defaults.remove("lang");
        } else {
            defaults.insert("lang".to_string(), toml::Value::String(lang));
        }
        defaults.insert(
            "load_fzf_defaults".to_string(),
            toml::Value::String(load_fzf_defaults),
        );
        defaults.insert("fzf_compat".to_string(), toml::Value::String(fzf_compat));
    }

    {
        let shell = ensure_toml_table(&mut value, "shell");
        shell.insert("bindings".to_string(), toml::Value::String(bindings));
        set_optional_toml_string(shell, "ctrl_t_command", ctrl_t_command);
        set_optional_toml_string(shell, "ctrl_t_opts", ctrl_t_opts);
        set_optional_toml_string(shell, "alt_c_command", alt_c_command);
        set_optional_toml_string(shell, "alt_c_opts", alt_c_opts);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let content = toml::to_string_pretty(&value).context("failed to serialize config")?;
    fs::write(&path, content).with_context(|| format!("failed to write {}", path.display()))?;
    println!("Updated {}", path.display());
    println!("Restart your shell or reload your profile for shell binding changes.");
    Ok(())
}

fn config_path_for_write() -> PathBuf {
    if let Ok(path) = std::env::var("YURU_CONFIG_FILE") {
        return PathBuf::from(path);
    }
    if let Some(ConfigSource::Toml(path)) = yuru_config_source() {
        return path;
    }
    default_config_path()
}

fn default_config_path() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("yuru").join("config.toml");
        }
    }
    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(config_home).join("yuru").join("config.toml")
    } else {
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
            .join(".config")
            .join("yuru")
            .join("config.toml")
    }
}

fn read_config_for_write(path: &Path) -> Result<toml::Value> {
    if !path.exists() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    content
        .parse::<toml::Value>()
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn shell_config_from_value(value: &toml::Value) -> ShellConfigDefaults {
    let mut config = ShellConfigDefaults::default();
    if let Some(shell) = value.get("shell") {
        if let Some(bindings) = shell.get("bindings").and_then(toml::Value::as_str) {
            config.bindings = bindings.to_string();
        }
        if let Some(command) = shell.get("ctrl_t_command").and_then(toml::Value::as_str) {
            config.ctrl_t_command = command.to_string();
        }
        if let Some(opts) = shell.get("ctrl_t_opts").and_then(toml::Value::as_str) {
            config.ctrl_t_opts = opts.to_string();
        }
        if let Some(command) = shell.get("alt_c_command").and_then(toml::Value::as_str) {
            config.alt_c_command = command.to_string();
        }
        if let Some(opts) = shell.get("alt_c_opts").and_then(toml::Value::as_str) {
            config.alt_c_opts = opts.to_string();
        }
    }
    config
}

fn ensure_toml_table<'a>(
    value: &'a mut toml::Value,
    key: &str,
) -> &'a mut toml::map::Map<String, toml::Value> {
    if !value.is_table() {
        *value = toml::Value::Table(toml::map::Map::new());
    }
    let root = value.as_table_mut().expect("root config is a TOML table");
    let entry = root
        .entry(key.to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    if !entry.is_table() {
        *entry = toml::Value::Table(toml::map::Map::new());
    }
    entry
        .as_table_mut()
        .expect("config section is a TOML table")
}

fn set_optional_toml_string(
    table: &mut toml::map::Map<String, toml::Value>,
    key: &str,
    value: Option<String>,
) {
    if let Some(value) = value {
        table.insert(key.to_string(), toml::Value::String(value));
    } else {
        table.remove(key);
    }
}

fn config_string(value: &toml::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str().map(str::to_string)
}

fn config_bool(value: &toml::Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_bool()
}

fn prompt_choice(prompt: &str, current: &str, choices: &[&str]) -> Result<String> {
    loop {
        print!("{prompt} [{}] ({current}): ", choices.join("/"));
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        let answer = answer.trim();
        let value = if answer.is_empty() { current } else { answer };
        if choices.contains(&value) {
            return Ok(value.to_string());
        }
        println!("Please enter one of: {}", choices.join(", "));
    }
}

fn prompt_bindings_value(current: &str) -> Result<String> {
    loop {
        print!("Shell bindings [all/custom/none/list] ({current}): ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        let answer = answer.trim();
        let value = if answer.is_empty() { current } else { answer };
        match value {
            "all" | "none" => return Ok(value.to_string()),
            "custom" => return prompt_custom_bindings(),
            _ if validate_binding_value(value) => return Ok(value.to_string()),
            _ => {
                println!(
                    "Please enter all, custom, none, or a comma-separated list of ctrl-t, ctrl-r, alt-c, completion."
                );
            }
        }
    }
}

fn validate_binding_value(value: &str) -> bool {
    value.split(',').all(|item| {
        matches!(
            item.trim(),
            "ctrl-t" | "ctrl-r" | "alt-c" | "completion" | "tab" | "path-completion"
        )
    })
}

fn prompt_custom_bindings() -> Result<String> {
    let mut selected = Vec::new();
    if prompt_yes_no("Enable CTRL-T file search?", true)? {
        selected.push("ctrl-t");
    }
    if prompt_yes_no("Enable CTRL-R history search?", true)? {
        selected.push("ctrl-r");
    }
    if prompt_yes_no("Enable ALT-C directory jump?", true)? {
        selected.push("alt-c");
    }
    if prompt_yes_no("Enable **<TAB> path completion?", true)? {
        selected.push("completion");
    }
    Ok(if selected.is_empty() {
        "none".to_string()
    } else {
        selected.join(",")
    })
}

fn prompt_yes_no(prompt: &str, default_yes: bool) -> Result<bool> {
    let suffix = if default_yes { "Y/n" } else { "y/N" };
    loop {
        print!("{prompt} [{suffix}]: ");
        io::stdout().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        match answer.trim() {
            "" => return Ok(default_yes),
            "y" | "Y" | "yes" | "YES" | "Yes" => return Ok(true),
            "n" | "N" | "no" | "NO" | "No" => return Ok(false),
            _ => println!("Please enter yes or no."),
        }
    }
}

fn prompt_string(prompt: &str, current: &str, help: &str) -> Result<Option<String>> {
    println!("{help}");
    print!("{prompt} ({current}): ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim();
    if answer.is_empty() {
        Ok(Some(current.to_string()))
    } else if answer == "none" {
        Ok(Some(String::new()))
    } else {
        Ok(Some(answer.to_string()))
    }
}

fn print_doctor_report() -> Result<()> {
    let mut stdout = io::stdout().lock();
    let exe = std::env::current_exe().context("failed to resolve current executable")?;
    let config = yuru_config_source();
    let default_lang = doctor_default_lang(config.as_ref()).unwrap_or_else(|| "plain".to_string());
    let fzf_mode = match preparse_load_fzf_default_opts(&[], config.as_ref()) {
        Ok(mode) => format!("{mode:?}").to_ascii_lowercase(),
        Err(error) => format!("unreadable ({error})"),
    };

    writeln!(stdout, "Yuru doctor")?;
    writeln!(stdout, "ok binary: {}", exe.display())?;
    writeln!(stdout, "ok version: {}", env!("CARGO_PKG_VERSION"))?;
    match path_visibility(&exe) {
        Some(path) => writeln!(stdout, "ok path: visible in PATH at {}", path.display())?,
        None => writeln!(stdout, "warn path: binary directory is not visible in PATH")?,
    }

    match &config {
        Some(ConfigSource::Toml(path)) => {
            writeln!(stdout, "ok config: {} (toml)", path.display())?;
        }
        Some(ConfigSource::Legacy(path)) => {
            writeln!(
                stdout,
                "warn config: {} (legacy shell words; migrate to config.toml)",
                path.display()
            )?;
        }
        None => {
            writeln!(stdout, "warn config: missing (using compiled defaults)")?;
        }
    }
    writeln!(stdout, "info default language: {default_lang}")?;
    writeln!(
        stdout,
        "info fzf default opts: {}",
        doctor_fzf_defaults(&fzf_mode)
    )?;
    writeln!(stdout, "info locale: {}", doctor_locale())?;
    writeln!(stdout, "info default command: {}", doctor_default_command())?;
    writeln!(
        stdout,
        "info shell integration: {}",
        doctor_shell_integration()
    )?;
    Ok(())
}

fn path_visibility(exe: &Path) -> Option<PathBuf> {
    let exe_name = exe.file_name()?;
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(exe_name))
        .find(|candidate| candidate.exists())
}

fn doctor_default_lang(config: Option<&ConfigSource>) -> Option<String> {
    match config? {
        ConfigSource::Toml(path) => toml_config_default_lang(path),
        ConfigSource::Legacy(path) => shell_word_default_lang(path),
    }
}

fn toml_config_default_lang(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let value = content.parse::<toml::Value>().ok()?;
    value
        .get("defaults")
        .and_then(|defaults| defaults.get("lang"))
        .and_then(toml::Value::as_str)
        .map(str::to_string)
}

fn shell_word_default_lang(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    find_option_value(split_shell_words(&content), "--lang")
}

fn find_option_value<I>(args: I, option: &str) -> Option<String>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let equals_prefix = format!("{option}=");
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let arg = arg.as_ref();
        if let Some(value) = arg.strip_prefix(&equals_prefix) {
            return Some(value.to_string());
        }
        if arg == option {
            return args.next().map(|value| value.as_ref().to_string());
        }
    }
    None
}

fn doctor_fzf_defaults(mode: &str) -> String {
    let mut sources = Vec::new();
    for name in [
        "FZF_DEFAULT_OPTS_FILE",
        "FZF_DEFAULT_OPTS",
        "YURU_DEFAULT_OPTS_FILE",
        "YURU_DEFAULT_OPTS",
    ] {
        if std::env::var_os(name).is_some() {
            sources.push(name);
        }
    }

    if sources.is_empty() {
        format!("{mode} (no default opts env)")
    } else {
        format!("{mode} ({})", sources.join(", "))
    }
}

fn doctor_locale() -> String {
    let locale = locale_hint();
    if locale.is_empty() {
        "(not set)".to_string()
    } else {
        locale
    }
}

fn doctor_default_command() -> String {
    default_source_command()
        .map(|(name, command)| {
            if command.trim().is_empty() {
                format!("{name} is set but empty")
            } else {
                format!("{name} ({command})")
            }
        })
        .unwrap_or_else(|| "built-in walker".to_string())
}

fn doctor_shell_integration() -> String {
    match detected_shell_profile() {
        Some((shell, path)) if profile_has_shell_integration(&path) => {
            format!("{shell} ({})", path.display())
        }
        Some((shell, path)) => {
            format!("{shell} profile missing marker ({})", path.display())
        }
        None => "unknown shell/profile".to_string(),
    }
}

#[cfg(not(windows))]
fn detected_shell_profile() -> Option<(&'static str, PathBuf)> {
    let shell = std::env::var("SHELL").ok()?;
    let home = PathBuf::from(std::env::var("HOME").ok()?);
    let shell_name = Path::new(&shell).file_name()?.to_string_lossy();
    match shell_name.as_ref() {
        "zsh" => Some(("zsh", home.join(".zshrc"))),
        "bash" => Some(("bash", home.join(".bashrc"))),
        "fish" => Some((
            "fish",
            home.join(".config").join("fish").join("config.fish"),
        )),
        _ => None,
    }
}

#[cfg(windows)]
fn detected_shell_profile() -> Option<(&'static str, PathBuf)> {
    let home = PathBuf::from(std::env::var("USERPROFILE").ok()?);
    let profiles = [
        home.join("Documents")
            .join("PowerShell")
            .join("Microsoft.PowerShell_profile.ps1"),
        home.join("Documents")
            .join("WindowsPowerShell")
            .join("Microsoft.PowerShell_profile.ps1"),
    ];
    profiles
        .into_iter()
        .find(|path| path.exists())
        .map(|path| ("powershell", path))
}

fn profile_has_shell_integration(path: &Path) -> bool {
    fs::read_to_string(path)
        .map(|content| content.contains("yuru shell integration"))
        .unwrap_or(false)
}

fn print_split_shell_words(words: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();
    for word in parse_shell_words(words)? {
        stdout.write_all(word.as_bytes())?;
        stdout.write_all(&[0])?;
    }
    Ok(())
}

fn expanded_args() -> Result<Vec<OsString>> {
    let mut args = std::env::args_os();
    let program = args.next().unwrap_or_else(|| OsString::from("yuru"));
    let mut expanded = vec![program];
    let rest: Vec<_> = args.collect();

    if !shell_flags_present(&rest) && !configless_command_present(&rest) {
        let config = yuru_config_source();
        let load_fzf_defaults = preparse_load_fzf_default_opts(&rest, config.as_ref())?;
        append_fzf_default_opts(&mut expanded, load_fzf_defaults)?;
        if let Some(config) = &config {
            expanded.extend(read_yuru_config_args(config)?);
        }
        append_shell_word_env(&mut expanded, "YURU_DEFAULT_OPTS_FILE", true)?;
        append_shell_word_env(&mut expanded, "YURU_DEFAULT_OPTS", false)?;
    }

    expanded.extend(rest.into_iter().map(normalize_plus_arg));
    Ok(expanded)
}

fn configless_command_present(args: &[OsString]) -> bool {
    matches!(
        args.first().and_then(|arg| arg.to_str()),
        Some("doctor" | "configure" | "__split-shell-words")
    )
}

#[derive(Clone, Debug)]
enum ConfigSource {
    Toml(PathBuf),
    Legacy(PathBuf),
}

fn yuru_config_source() -> Option<ConfigSource> {
    if let Ok(path) = std::env::var("YURU_CONFIG_FILE") {
        let path = PathBuf::from(path);
        return path.exists().then(|| config_source_for_path(path));
    }
    let mut candidates = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let appdata = PathBuf::from(appdata);
            candidates.push(appdata.join("yuru").join("config.toml"));
            candidates.push(appdata.join("yuru").join("config"));
        }
    }

    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        let config_home = PathBuf::from(config_home);
        candidates.push(config_home.join("yuru").join("config.toml"));
        candidates.push(config_home.join("yuru").join("config"));
    } else if let Ok(home) = std::env::var("HOME") {
        let config_home = PathBuf::from(home).join(".config");
        candidates.push(config_home.join("yuru").join("config.toml"));
        candidates.push(config_home.join("yuru").join("config"));
    }

    candidates
        .into_iter()
        .find(|path| path.exists())
        .map(config_source_for_path)
}

fn config_source_for_path(path: PathBuf) -> ConfigSource {
    if path
        .extension()
        .is_some_and(|extension| extension == "toml")
    {
        ConfigSource::Toml(path)
    } else {
        ConfigSource::Legacy(path)
    }
}

fn preparse_load_fzf_default_opts(
    cli_args: &[OsString],
    config: Option<&ConfigSource>,
) -> Result<LoadFzfDefaultOptsArg> {
    let mut mode = LoadFzfDefaultOptsArg::Safe;

    if let Some(ConfigSource::Toml(path)) = config {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let value = content
            .parse::<toml::Value>()
            .with_context(|| format!("failed to parse {}", path.display()))?;
        if let Some(config_mode) = toml_load_fzf_default_opts(&value)? {
            mode = config_mode;
        }
    }

    if let Some(env_mode) = load_fzf_default_opts_from_yuru_env()? {
        mode = env_mode;
    }
    if let Some(cli_mode) =
        load_fzf_default_opts_from_args(cli_args.iter().filter_map(|arg| arg.to_str()))?
    {
        mode = cli_mode;
    }

    Ok(mode)
}

fn load_fzf_default_opts_from_yuru_env() -> Result<Option<LoadFzfDefaultOptsArg>> {
    let mut mode = None;
    if let Ok(path) = std::env::var("YURU_DEFAULT_OPTS_FILE") {
        let content = fs::read_to_string(path)?;
        mode = load_fzf_default_opts_from_args(split_shell_words(&content))?.or(mode);
    }
    if let Ok(opts) = std::env::var("YURU_DEFAULT_OPTS") {
        mode = load_fzf_default_opts_from_args(split_shell_words(&opts))?.or(mode);
    }
    Ok(mode)
}

fn load_fzf_default_opts_from_args<I, S>(args: I) -> Result<Option<LoadFzfDefaultOptsArg>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter().peekable();
    let mut out = None;

    while let Some(arg) = args.next() {
        let arg = arg.as_ref();
        if let Some(value) = arg.strip_prefix("--load-fzf-default-opts=") {
            out = Some(parse_load_fzf_default_opts(value)?);
        } else if arg == "--load-fzf-default-opts" {
            let Some(value) = args.next() else {
                bail!("--load-fzf-default-opts requires a value");
            };
            out = Some(parse_load_fzf_default_opts(value.as_ref())?);
        }
    }

    Ok(out)
}

fn parse_load_fzf_default_opts(value: &str) -> Result<LoadFzfDefaultOptsArg> {
    match value {
        "never" => Ok(LoadFzfDefaultOptsArg::Never),
        "safe" => Ok(LoadFzfDefaultOptsArg::Safe),
        "all" => Ok(LoadFzfDefaultOptsArg::All),
        other => bail!("unsupported --load-fzf-default-opts value: {other}"),
    }
}

fn toml_load_fzf_default_opts(value: &toml::Value) -> Result<Option<LoadFzfDefaultOptsArg>> {
    if let Some(raw) = value
        .get("defaults")
        .and_then(|defaults| defaults.get("load_fzf_defaults"))
        .and_then(toml::Value::as_str)
    {
        return parse_load_fzf_default_opts(raw).map(Some);
    }

    Ok(value
        .get("fzf")
        .and_then(|fzf| fzf.get("safe_default_opts"))
        .and_then(toml::Value::as_bool)
        .map(|safe| {
            if safe {
                LoadFzfDefaultOptsArg::Safe
            } else {
                LoadFzfDefaultOptsArg::All
            }
        }))
}

fn append_fzf_default_opts(
    expanded: &mut Vec<OsString>,
    mode: LoadFzfDefaultOptsArg,
) -> Result<()> {
    if mode == LoadFzfDefaultOptsArg::Never {
        return Ok(());
    }

    append_fzf_default_opts_env(expanded, "FZF_DEFAULT_OPTS_FILE", true, mode)?;
    append_fzf_default_opts_env(expanded, "FZF_DEFAULT_OPTS", false, mode)
}

fn append_fzf_default_opts_env(
    expanded: &mut Vec<OsString>,
    env_name: &str,
    is_file: bool,
    mode: LoadFzfDefaultOptsArg,
) -> Result<()> {
    let Ok(value) = std::env::var(env_name) else {
        return Ok(());
    };
    let content = if is_file {
        fs::read_to_string(value)?
    } else {
        value
    };
    let words: Vec<String> = split_shell_words(&content).collect();
    let words = if mode == LoadFzfDefaultOptsArg::Safe {
        safe_fzf_default_opts(&words)
    } else {
        words
    };
    expanded.extend(words.into_iter().map(OsString::from));
    Ok(())
}

fn append_shell_word_env(
    expanded: &mut Vec<OsString>,
    env_name: &str,
    is_file: bool,
) -> Result<()> {
    let Ok(value) = std::env::var(env_name) else {
        return Ok(());
    };
    let content = if is_file {
        fs::read_to_string(value)?
    } else {
        value
    };
    expanded.extend(split_shell_words(&content).map(OsString::from));
    Ok(())
}

fn read_yuru_config_args(config: &ConfigSource) -> Result<Vec<OsString>> {
    match config {
        ConfigSource::Toml(path) => {
            let content = fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let value = content
                .parse::<toml::Value>()
                .with_context(|| format!("failed to parse {}", path.display()))?;
            toml_config_args(&value)
        }
        ConfigSource::Legacy(path) => {
            eprintln!(
                "yuru: warning: legacy shell-word config {} is deprecated; use config.toml",
                path.display()
            );
            let content = fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            Ok(split_shell_words(&content).map(OsString::from).collect())
        }
    }
}

fn toml_config_args(value: &toml::Value) -> Result<Vec<OsString>> {
    let mut out = Vec::new();

    if let Some(defaults) = value.get("defaults") {
        push_toml_string_arg(&mut out, defaults, "lang", "--lang");
        push_toml_string_arg(&mut out, defaults, "scheme", "--scheme");
        push_toml_usize_arg(&mut out, defaults, "limit", "--limit")?;
        push_toml_string_arg(
            &mut out,
            defaults,
            "load_fzf_defaults",
            "--load-fzf-default-opts",
        );
        push_toml_string_arg(&mut out, defaults, "fzf_compat", "--fzf-compat");
        if let Some(case) = defaults.get("case").and_then(toml::Value::as_str) {
            match case {
                "smart" => out.push(OsString::from("--smart-case")),
                "ignore" => out.push(OsString::from("--ignore-case")),
                "respect" => out.push(OsString::from("--no-ignore-case")),
                other => bail!("unsupported defaults.case value: {other}"),
            }
        }
    }

    if let Some(matching) = value.get("matching") {
        push_toml_string_arg(&mut out, matching, "algo", "--algo");
        push_toml_usize_arg(
            &mut out,
            matching,
            "max_query_variants",
            "--max-query-variants",
        )?;
        push_toml_usize_arg(
            &mut out,
            matching,
            "max_search_keys_per_candidate",
            "--max-keys-per-candidate",
        )?;
        push_toml_usize_arg(
            &mut out,
            matching,
            "max_keys_per_candidate",
            "--max-keys-per-candidate",
        )?;
        push_toml_usize_arg(
            &mut out,
            matching,
            "max_total_key_bytes_per_candidate",
            "--max-total-key-bytes-per-candidate",
        )?;
        push_toml_usize_arg(&mut out, matching, "top_b", "--top-b")?;
    }

    if let Some(ja) = value.get("ja") {
        push_toml_string_arg(&mut out, ja, "reading", "--ja-reading");
    }

    if let Some(zh) = value.get("zh") {
        push_toml_bool_flag(&mut out, zh, "pinyin", "--zh-pinyin", "--no-zh-pinyin");
        push_toml_bool_flag(
            &mut out,
            zh,
            "initials",
            "--zh-initials",
            "--no-zh-initials",
        );
        push_toml_string_arg(&mut out, zh, "polyphone", "--zh-polyphone");
        push_toml_string_arg(&mut out, zh, "script", "--zh-script");
    }

    if let Some(fzf) = value.get("fzf") {
        push_toml_string_arg(&mut out, fzf, "unsupported_options", "--fzf-compat");
        if let Some(safe) = fzf.get("safe_default_opts").and_then(toml::Value::as_bool) {
            out.push(OsString::from("--load-fzf-default-opts"));
            out.push(OsString::from(if safe { "safe" } else { "all" }));
        }
    }

    Ok(out)
}

fn push_toml_string_arg(out: &mut Vec<OsString>, table: &toml::Value, key: &str, arg: &str) {
    if let Some(value) = table.get(key).and_then(toml::Value::as_str) {
        out.push(OsString::from(arg));
        out.push(OsString::from(value));
    }
}

fn push_toml_usize_arg(
    out: &mut Vec<OsString>,
    table: &toml::Value,
    key: &str,
    arg: &str,
) -> Result<()> {
    if let Some(value) = table.get(key).and_then(toml::Value::as_integer) {
        let value =
            usize::try_from(value).with_context(|| format!("{key} must be non-negative"))?;
        out.push(OsString::from(arg));
        out.push(OsString::from(value.to_string()));
    }
    Ok(())
}

fn push_toml_bool_flag(
    out: &mut Vec<OsString>,
    table: &toml::Value,
    key: &str,
    enabled_arg: &str,
    disabled_arg: &str,
) {
    if let Some(value) = table.get(key).and_then(toml::Value::as_bool) {
        out.push(OsString::from(if value {
            enabled_arg
        } else {
            disabled_arg
        }));
    }
}

fn safe_fzf_default_opts(words: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut index = 0usize;

    while index < words.len() {
        let word = &words[index];
        if safe_fzf_flag(word) {
            out.push(
                normalize_plus_arg(OsString::from(word))
                    .to_string_lossy()
                    .into_owned(),
            );
            index += 1;
            continue;
        }

        if let Some((name, _)) = word.split_once('=') {
            if safe_fzf_value_option(name) {
                out.push(word.clone());
            }
            index += 1;
            continue;
        }

        if safe_fzf_value_option(word) {
            if let Some(value) = words.get(index + 1) {
                out.push(word.clone());
                out.push(value.clone());
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }

        if unsafe_fzf_value_option(word) {
            index += 1 + usize::from(
                words
                    .get(index + 1)
                    .is_some_and(|next| !next.starts_with('-')),
            );
        } else {
            index += 1;
        }
    }

    out
}

fn safe_fzf_flag(word: &str) -> bool {
    matches!(
        word,
        "-e" | "--exact"
            | "-x"
            | "+x"
            | "--no-extended"
            | "-i"
            | "--ignore-case"
            | "+i"
            | "--no-ignore-case"
            | "--smart-case"
            | "+s"
            | "--no-sort"
            | "--disabled"
            | "--tac"
            | "--sync"
            | "--read0"
            | "--print0"
            | "--ansi"
            | "--print-query"
            | "-1"
            | "--select-1"
            | "-0"
            | "--exit-0"
            | "-m"
            | "--multi"
            | "+m"
            | "--no-multi"
            | "--cycle"
            | "--no-height"
            | "--extended"
            | "--reverse"
    )
}

fn safe_fzf_value_option(word: &str) -> bool {
    matches!(
        word,
        "-q" | "--query"
            | "-f"
            | "--filter"
            | "--limit"
            | "-n"
            | "--nth"
            | "--with-nth"
            | "--accept-nth"
            | "-d"
            | "--delimiter"
            | "--scheme"
            | "--tail"
            | "--tiebreak"
            | "--walker"
            | "--walker-root"
            | "--walker-skip"
            | "--height"
            | "--layout"
            | "--color"
            | "--prompt"
    )
}

fn unsafe_fzf_value_option(word: &str) -> bool {
    matches!(
        word,
        "--preview"
            | "--preview-window"
            | "--bind"
            | "--expect"
            | "--header"
            | "--header-lines"
            | "--border"
            | "--color"
            | "--style"
            | "--margin"
            | "--padding"
            | "--tmux"
            | "--popup"
            | "--listen"
            | "--history"
            | "--with-shell"
    )
}

fn shell_flags_present(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        matches!(
            arg.to_str(),
            Some(
                "--bash"
                    | "--zsh"
                    | "--fish"
                    | "--powershell"
                    | "--bash=true"
                    | "--zsh=true"
                    | "--fish=true"
                    | "--powershell=true"
            )
        )
    })
}

fn walker_flags_present(args: &[OsString]) -> bool {
    args.iter().skip(1).any(|arg| {
        let Some(arg) = arg.to_str() else {
            return false;
        };

        matches!(arg, "--walker" | "--walker-root" | "--walker-skip")
            || arg.starts_with("--walker=")
            || arg.starts_with("--walker-root=")
            || arg.starts_with("--walker-skip=")
    })
}

fn split_shell_words(input: &str) -> impl Iterator<Item = String> + '_ {
    shlex::split(input).unwrap_or_default().into_iter()
}

fn parse_shell_words(input: &str) -> Result<Vec<String>> {
    shlex::split(input).with_context(|| "failed to parse shell words")
}

fn normalize_plus_arg(arg: OsString) -> OsString {
    match arg.to_str() {
        Some("+s") => OsString::from("--no-sort"),
        Some("+x") => OsString::from("--no-extended"),
        Some("+e") => OsString::from("--no-exact"),
        Some("+i") => OsString::from("--no-ignore-case"),
        Some("+m") => OsString::from("--no-multi"),
        Some("+1") => OsString::from("--no-select-1"),
        Some("+0") => OsString::from("--no-exit-0"),
        Some("+c") => OsString::from("--no-color"),
        Some("+2") => OsString::from("--no-256"),
        Some("+S") => OsString::from("--no-clear"),
        _ => arg,
    }
}

fn shell_script_kind(args: &Args) -> Result<Option<ShellKind>> {
    let selected = [args.bash, args.zsh, args.fish, args.powershell]
        .into_iter()
        .filter(|enabled| *enabled)
        .count();
    if selected > 1 {
        bail!("only one of --bash, --zsh, --fish, or --powershell can be used");
    }

    Ok(if args.bash {
        Some(ShellKind::Bash)
    } else if args.zsh {
        Some(ShellKind::Zsh)
    } else if args.fish {
        Some(ShellKind::Fish)
    } else if args.powershell {
        Some(ShellKind::PowerShell)
    } else {
        None
    })
}
