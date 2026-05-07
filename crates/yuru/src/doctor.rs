use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::{
    backend::locale_hint,
    config::{config_string, preparse_load_fzf_default_opts, yuru_config_source, ConfigSource},
    input::default_source_command,
    shell_words::split_shell_words,
};

pub(crate) fn print_doctor_report() -> Result<()> {
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
    writeln!(
        stdout,
        "info preview image protocol: {}",
        doctor_preview_image_protocol(config.as_ref())
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

pub(crate) fn doctor_default_lang(config: Option<&ConfigSource>) -> Option<String> {
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

fn doctor_preview_image_protocol(config: Option<&ConfigSource>) -> String {
    let Some(ConfigSource::Toml(path)) = config else {
        return "none".to_string();
    };
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return "unreadable".to_string(),
    };
    let value = match content.parse::<toml::Value>() {
        Ok(value) => value,
        Err(_) => return "unreadable".to_string(),
    };
    config_string(&value, &["preview", "image_protocol"]).unwrap_or_else(|| "none".to_string())
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
