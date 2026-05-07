use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};

use crate::{
    cli::LoadFzfDefaultOptsArg, options::parse_preview_text_extensions,
    shell_words::split_shell_words,
};

pub(crate) fn expanded_args() -> Result<Vec<OsString>> {
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

pub(crate) fn configless_command_present(args: &[OsString]) -> bool {
    matches!(
        args.first().and_then(|arg| arg.to_str()),
        Some("doctor" | "configure" | "__split-shell-words")
    )
}

#[derive(Clone, Debug)]
pub(crate) enum ConfigSource {
    Toml(PathBuf),
    Legacy(PathBuf),
}

pub(crate) fn yuru_config_source() -> Option<ConfigSource> {
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

pub(crate) fn preparse_load_fzf_default_opts(
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

pub(crate) fn toml_config_args(value: &toml::Value) -> Result<Vec<OsString>> {
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

    if let Some(ko) = value.get("ko") {
        push_toml_bool_flag(
            &mut out,
            ko,
            "romanization",
            "--ko-romanization",
            "--no-ko-romanization",
        );
        push_toml_bool_flag(
            &mut out,
            ko,
            "initials",
            "--ko-initials",
            "--no-ko-initials",
        );
        push_toml_bool_flag(
            &mut out,
            ko,
            "keyboard",
            "--ko-keyboard",
            "--no-ko-keyboard",
        );
    }

    if let Some(fzf) = value.get("fzf") {
        push_toml_string_arg(&mut out, fzf, "unsupported_options", "--fzf-compat");
        if let Some(safe) = fzf.get("safe_default_opts").and_then(toml::Value::as_bool) {
            out.push(OsString::from("--load-fzf-default-opts"));
            out.push(OsString::from(if safe { "safe" } else { "all" }));
        }
    }

    if let Some(preview) = value.get("preview") {
        if let Some(command) = preview.get("command").and_then(toml::Value::as_str) {
            match command {
                "auto" => out.push(OsString::from("--preview-auto")),
                "none" => out.push(OsString::from("--no-preview")),
                command => {
                    out.push(OsString::from("--preview"));
                    out.push(OsString::from(command));
                }
            }
        }
        push_toml_string_list_arg(
            &mut out,
            preview,
            "text_extensions",
            "--preview-text-extensions",
        );
        push_toml_string_arg(
            &mut out,
            preview,
            "image_protocol",
            "--preview-image-protocol",
        );
    }

    Ok(out)
}

fn push_toml_string_list_arg(out: &mut Vec<OsString>, table: &toml::Value, key: &str, arg: &str) {
    let Some(value) = table.get(key) else {
        return;
    };
    let items = if let Some(raw) = value.as_str() {
        parse_preview_text_extensions(raw)
    } else if let Some(array) = value.as_array() {
        array
            .iter()
            .filter_map(toml::Value::as_str)
            .flat_map(parse_preview_text_extensions)
            .collect()
    } else {
        Vec::new()
    };
    if !items.is_empty() {
        out.push(OsString::from(arg));
        out.push(OsString::from(items.join(",")));
    }
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

pub(crate) fn walker_flags_present(args: &[OsString]) -> bool {
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

pub(crate) fn config_string(value: &toml::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str().map(str::to_string)
}

pub(crate) fn config_string_list(value: &toml::Value, path: &[&str]) -> Option<Vec<String>> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    if let Some(raw) = current.as_str() {
        return Some(parse_preview_text_extensions(raw));
    }
    current.as_array().map(|items| {
        items
            .iter()
            .filter_map(toml::Value::as_str)
            .map(str::to_string)
            .collect()
    })
}

pub(crate) fn config_bool(value: &toml::Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configless_preparse_only_matches_leading_subcommands() {
        assert!(configless_command_present(&[OsString::from("doctor")]));
        assert!(configless_command_present(&[OsString::from("configure")]));
        assert!(configless_command_present(&[OsString::from(
            "__split-shell-words"
        )]));
        assert!(!configless_command_present(&[
            OsString::from("--filter"),
            OsString::from("doctor"),
        ]));
    }

    #[test]
    fn toml_config_supports_preview_options() {
        let value = r#"
[preview]
command = "auto"
text_extensions = ["txt", "md"]
image_protocol = "sixel"
"#
        .parse::<toml::Value>()
        .unwrap();

        assert_eq!(
            toml_config_args(&value).unwrap(),
            vec![
                OsString::from("--preview-auto"),
                OsString::from("--preview-text-extensions"),
                OsString::from("txt,md"),
                OsString::from("--preview-image-protocol"),
                OsString::from("sixel"),
            ]
        );
    }
}
