mod fields;
mod shell;

use std::ffi::OsString;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use fields::{accept_output, prepare_items, FieldConfig, InputItem};
use shell::ShellKind;
use walkdir::{DirEntry, WalkDir};
use yuru_core::{
    build_index, dedup_and_limit_keys, dedup_and_limit_variants, search, LanguageBackend,
    PlainBackend, SearchConfig, SearchKey, Tiebreak,
};
use yuru_ja::JapaneseBackend;
use yuru_zh::ChineseBackend;

const DEFAULT_WALKER: &str = "file,follow,hidden";
const DEFAULT_WALKER_ROOT: &str = ".";
const DEFAULT_WALKER_SKIP: &str = ".git,node_modules";
const DEFAULT_INTERACTIVE_LIMIT: usize = 1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum LangArg {
    Plain,
    Ja,
    Zh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SchemeArg {
    Default,
    Path,
    History,
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

    #[arg(long)]
    algo: Option<String>,

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
    let backend = create_backend(args.lang);
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
        tiebreaks,
    };

    if args.debug_query_variants {
        print_query_variants(&query, backend.as_ref(), &config, args.print0)?;
    }

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
        output.push(query.clone());
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

fn create_backend(lang: LangArg) -> Box<dyn LanguageBackend> {
    match lang {
        LangArg::Plain => Box::new(PlainBackend),
        LangArg::Ja => Box::new(JapaneseBackend),
        LangArg::Zh => Box::new(ChineseBackend),
    }
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
        height: parse_tui_height(args),
        cycle: args.cycle,
        multi: args.multi && !args.no_multi,
    };

    match yuru_tui::run_interactive(index, backend, config, options)? {
        yuru_tui::TuiOutcome::Accepted { ids, query } => {
            let mut output = Vec::new();
            if args.print_query {
                output.push(query);
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

fn read_input_candidates(args: &Args, walker_requested: bool) -> Result<Vec<String>> {
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

fn read_stdin_candidates(read0: bool) -> Result<Vec<String>> {
    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input)?;
    let text = String::from_utf8_lossy(&input);

    if read0 {
        Ok(text
            .split('\0')
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect())
    } else {
        Ok(text.lines().map(str::to_owned).collect())
    }
}

fn default_source_command() -> Option<(&'static str, String)> {
    for env_name in ["YURU_DEFAULT_COMMAND", "FZF_DEFAULT_COMMAND"] {
        if let Ok(command) = std::env::var(env_name) {
            return Some((env_name, command));
        }
    }
    None
}

fn run_default_command(env_name: &str, command: &str) -> Result<Vec<String>> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let output = std::process::Command::new(shell)
        .arg("-c")
        .arg(command)
        .output()
        .with_context(|| format!("failed to run {env_name}: {command}"))?;

    if !output.status.success() {
        bail!("{env_name} exited with {}", output.status);
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

#[derive(Clone, Copy, Debug)]
struct WalkerOptions {
    files: bool,
    dirs: bool,
    follow: bool,
    hidden: bool,
}

fn run_walker(args: &Args) -> Result<Vec<String>> {
    let options = parse_walker_options(&args.walker)?;
    let skips = parse_walker_skip(&args.walker_skip);
    let mut out = Vec::new();

    for root in &args.walker_roots {
        for entry in WalkDir::new(root)
            .follow_links(options.follow)
            .into_iter()
            .filter_entry(|entry| walker_entry_allowed(entry, &skips, options.hidden))
        {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) if walker_error_is_skippable(&error) => continue,
                Err(error) => return Err(error.into()),
            };
            if entry.depth() == 0 {
                continue;
            }

            let file_type = entry.file_type();
            let include =
                file_type.is_file() && options.files || file_type.is_dir() && options.dirs;
            if include {
                out.push(display_walked_path(root, entry.path()));
            }
        }
    }

    Ok(out)
}

fn walker_error_is_skippable(error: &walkdir::Error) -> bool {
    if error.depth() == 0 {
        return false;
    }

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
    let mut records = vec![format!("variant_count={}", variants.len())];
    records.extend(
        variants
            .into_iter()
            .map(|variant| format!("{}\t{:?}", variant.text, variant.kind)),
    );
    write_records(&records, print0)
}

fn write_records(records: &[String], print0: bool) -> Result<()> {
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
        if let Some(path) = yuru_config_file() {
            if path.exists() {
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;
                expanded.extend(split_shell_words(&content).map(OsString::from));
            }
        }
        if let Ok(path) = std::env::var("YURU_DEFAULT_OPTS_FILE") {
            let content = std::fs::read_to_string(path)?;
            expanded.extend(split_shell_words(&content).map(OsString::from));
        }
        if let Ok(opts) = std::env::var("YURU_DEFAULT_OPTS") {
            expanded.extend(split_shell_words(&opts).map(OsString::from));
        }
        if let Ok(path) = std::env::var("FZF_DEFAULT_OPTS_FILE") {
            let content = std::fs::read_to_string(path)?;
            expanded.extend(split_shell_words(&content).map(OsString::from));
        }
        if let Ok(opts) = std::env::var("FZF_DEFAULT_OPTS") {
            expanded.extend(split_shell_words(&opts).map(OsString::from));
        }
    }

    expanded.extend(rest.into_iter().map(normalize_plus_arg));
    Ok(expanded)
}

fn yuru_config_file() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("YURU_CONFIG_FILE") {
        return Some(PathBuf::from(path));
    }
    let mut candidates = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let appdata = PathBuf::from(appdata);
            candidates.push(appdata.join("yuru").join("config"));
        }
    }

    if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
        let config_home = PathBuf::from(config_home);
        candidates.push(config_home.join("yuru").join("config"));
    } else if let Ok(home) = std::env::var("HOME") {
        let config_home = PathBuf::from(home).join(".config");
        candidates.push(config_home.join("yuru").join("config"));
    }

    candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .or_else(|| candidates.into_iter().next())
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
