mod fields;
mod shell;

use std::ffi::OsString;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use fields::{accept_output, prepare_items, FieldConfig, InputItem, InputRecord, OutputRecord};
use ignore::{DirEntry, WalkBuilder};
use shell::ShellKind;
use yuru_core::{
    build_index, dedup_and_limit_keys, dedup_and_limit_variants, search, LanguageBackend,
    MatcherAlgo, PlainBackend, SearchConfig, SearchKey, Tiebreak,
};
use yuru_ja::{JapaneseBackend, JapaneseReadingMode};
use yuru_zh::{ChineseBackend, ChinesePolyphoneMode, ChineseScriptMode};

const DEFAULT_WALKER: &str = "file,follow,hidden";
const DEFAULT_WALKER_ROOT: &str = ".";
const DEFAULT_WALKER_SKIP: &str = ".git,node_modules";
const DEFAULT_INTERACTIVE_LIMIT: usize = 1000;

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
    FzfV1,
    FzfV2,
    Nucleo,
}

#[derive(Debug, Parser)]
#[command(
    name = "yuru",
    about = "A fast phonetic fuzzy finder prototype",
    version,
    args_override_self = true
)]
struct Args {
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

    #[arg(long, default_value_t = true)]
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

    #[arg(long, default_value = "length")]
    tiebreak: String,

    #[arg(long, value_enum, default_value_t = SchemeArg::Default)]
    scheme: SchemeArg,

    #[arg(long)]
    disabled: bool,

    #[arg(long)]
    tac: bool,

    #[arg(long)]
    tail: Option<usize>,

    #[arg(long)]
    read0: bool,

    #[arg(long)]
    print0: bool,

    #[arg(long, hide = true)]
    input: Option<PathBuf>,

    #[arg(long)]
    ansi: bool,

    #[arg(long)]
    print_query: bool,

    #[arg(short = '1', long)]
    select_1: bool,

    #[arg(short = '0', long)]
    exit_0: bool,

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

    #[arg(short = 'm', long)]
    multi: bool,

    #[arg(long)]
    no_multi: bool,

    #[arg(long)]
    expect: Option<String>,

    #[arg(long)]
    bind: Vec<String>,

    #[arg(long)]
    preview: Option<String>,

    #[arg(long)]
    preview_window: Option<String>,

    #[arg(long)]
    height: Option<String>,

    #[arg(long)]
    no_height: bool,

    #[arg(long)]
    layout: Option<String>,

    #[arg(long)]
    border: Option<String>,

    #[arg(long)]
    prompt: Option<String>,

    #[arg(long)]
    header: Option<String>,

    #[arg(long)]
    header_lines: Option<usize>,

    #[arg(long)]
    color: Vec<String>,

    #[arg(long)]
    cycle: bool,

    #[arg(long, default_value = DEFAULT_WALKER)]
    walker: String,

    #[arg(long = "walker-root", default_value = DEFAULT_WALKER_ROOT)]
    walker_roots: Vec<PathBuf>,

    #[arg(long = "walker-skip", default_value = DEFAULT_WALKER_SKIP)]
    walker_skip: String,

    #[arg(long)]
    debug_query_variants: bool,

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
        print!("{}", shell::script(kind));
        return Ok(ExitCode::SUCCESS);
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
        exact: args.exact,
        extended: args.extended && !args.no_extended,
        case_sensitive: case_sensitive(&query, &args),
        disabled: args.disabled,
        no_sort: args.no_sort,
        matcher_algo: matcher_algo(args.algo),
        tiebreaks,
    };

    let mut raw_items =
        read_input_candidates(&args, walker_requested).context("failed to load candidates")?;
    if let Some(tail) = args.tail {
        let keep_from = raw_items.len().saturating_sub(tail);
        raw_items = raw_items.split_off(keep_from);
    }
    if args.tac {
        raw_items.reverse();
    }

    let field_config = FieldConfig {
        delimiter: args.delimiter.clone(),
        nth: args.nth.clone(),
        with_nth: args.with_nth.clone(),
        accept_nth: args.accept_nth.clone(),
    };
    let items = prepare_items(raw_items, &field_config, args.ansi)?;
    let backend = create_backend(&args, &query, &items);
    if args.debug_query_variants {
        print_query_variants(&query, backend.as_ref(), &config, args.print0)?;
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
            backend.as_ref(),
            config,
            query,
        );
    }

    let results = search(&query, &index, backend.as_ref(), &config);

    let mut output = Vec::new();
    if args.print_query {
        output.push(OutputRecord::Text(query.clone()));
    }

    if args.select_1 && results.len() == 1 {
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

    write_records(&output, args.print0)?;

    if results.is_empty() && !args.exit_0 && !args.debug_query_variants {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn create_backend(args: &Args, query: &str, items: &[InputItem]) -> Box<dyn LanguageBackend> {
    let lang = match args.lang {
        LangArg::Auto => detect_auto_lang(query, items),
        lang => lang,
    };

    match lang {
        LangArg::Plain => Box::new(PlainBackend),
        LangArg::Ja => Box::new(JapaneseBackend::new(japanese_reading_mode(args.ja_reading))),
        LangArg::Zh => Box::new(ChineseBackend::new(
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

    if locale.starts_with("ja") && (sample_has_kana || sample_has_han) {
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

fn run_interactive_mode(
    args: &Args,
    items: &[InputItem],
    field_config: &FieldConfig,
    index: &[yuru_core::Candidate],
    backend: &dyn LanguageBackend,
    config: SearchConfig,
    query: String,
) -> Result<ExitCode> {
    let options = yuru_tui::TuiOptions {
        initial_query: query,
        prompt: args.prompt.clone().unwrap_or_else(|| "> ".to_string()),
        header: args.header.clone(),
        expect_keys: parse_expect_keys(args.expect.as_deref()),
        bindings: parse_bindings(&args.bind),
        height: parse_tui_height(args),
        cycle: args.cycle,
        multi: args.multi && !args.no_multi,
    };

    match yuru_tui::run_interactive(index, backend, config, options)? {
        yuru_tui::TuiOutcome::Accepted { ids, query, expect } => {
            let mut output = Vec::new();
            if args.expect.is_some() {
                output.push(OutputRecord::Text(expect.unwrap_or_default()));
            }
            if args.print_query {
                output.push(OutputRecord::Text(query));
            }
            for id in ids {
                output.push(accept_output(&items[id], field_config, id)?);
            }
            write_records(&output, args.print0)?;
            Ok(ExitCode::SUCCESS)
        }
        yuru_tui::TuiOutcome::NoSelection => {
            if args.exit_0 {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::from(1))
            }
        }
        yuru_tui::TuiOutcome::Aborted => Ok(ExitCode::from(130)),
    }
}

fn should_run_interactive(args: &Args) -> bool {
    should_run_interactive_with_tty(args, io::stderr().is_terminal())
}

fn should_run_interactive_with_tty(args: &Args, ui_tty_available: bool) -> bool {
    args.filter.is_none() && !args.debug_query_variants && ui_tty_available
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
        "clear-query" | "clear" => yuru_tui::BindingAction::ClearQuery,
        _ => return None,
    };

    Some(yuru_tui::KeyBinding {
        key: key.trim().to_ascii_lowercase(),
        action,
    })
}

fn has_unsupported_bindings(raw: &[String]) -> bool {
    raw.iter()
        .flat_map(|bindings| bindings.split(','))
        .map(str::trim)
        .filter(|binding| !binding.is_empty())
        .any(|binding| parse_supported_binding(binding).is_none())
}

fn enforce_fzf_compat(args: &Args) -> Result<()> {
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
    if args.preview.is_some() {
        out.push("--preview");
    }
    if args.preview_window.is_some() {
        out.push("--preview-window");
    }
    if args.layout.is_some() {
        out.push("--layout");
    }
    if args.border.is_some() {
        out.push("--border");
    }
    if args.header_lines.is_some() {
        out.push("--header-lines");
    }
    if !args.color.is_empty() {
        out.push("--color");
    }
    out
}

fn read_input_candidates(args: &Args, walker_requested: bool) -> Result<Vec<InputRecord>> {
    if let Some(path) = &args.input {
        return read_file_candidates(path, args.read0);
    }

    let stdin_is_terminal = io::stdin().is_terminal();
    let stdin_items = if stdin_is_terminal {
        Vec::new()
    } else {
        read_stdin_candidates(args.read0)?
    };
    if !stdin_items.is_empty() {
        return Ok(stdin_items);
    }

    if walker_requested {
        return run_walker(args);
    }

    if let Some((env_name, command)) = default_source_command() {
        if !command.trim().is_empty() {
            return run_default_command(env_name, &command);
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

fn run_default_command(env_name: &str, command: &str) -> Result<Vec<InputRecord>> {
    let output = default_command_process(command)
        .output()
        .with_context(|| format!("failed to run {env_name}: {command}"))?;

    if !output.status.success() {
        bail!("{env_name} exited with {}", output.status);
    }

    Ok(parse_candidate_bytes(&output.stdout, false))
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
    error.io_error().is_some_and(|io_error| {
        matches!(
            io_error.kind(),
            io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
        )
    })
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

fn expanded_args() -> Result<Vec<OsString>> {
    let mut args = std::env::args_os();
    let program = args.next().unwrap_or_else(|| OsString::from("yuru"));
    let mut expanded = vec![program];
    let rest: Vec<_> = args.collect();

    if !shell_flags_present(&rest) {
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
            | "--layout"
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

fn normalize_plus_arg(arg: OsString) -> OsString {
    match arg.to_str() {
        Some("+s") => OsString::from("--no-sort"),
        Some("+x") => OsString::from("--no-extended"),
        Some("+i") => OsString::from("--no-ignore-case"),
        Some("+m") => OsString::from("--no-multi"),
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
