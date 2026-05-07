use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::Path;

use anyhow::{bail, Context, Result};
use ignore::{DirEntry, WalkBuilder};

use crate::{fields::InputRecord, options::read0_enabled, Args};

pub(crate) fn read_input_candidates(
    args: &Args,
    walker_requested: bool,
) -> Result<Vec<InputRecord>> {
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

pub(crate) fn read_stdin_candidates(read0: bool) -> Result<Vec<InputRecord>> {
    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input)?;
    Ok(parse_candidate_bytes(&input, read0))
}

pub(crate) fn read_file_candidates(path: &Path, read0: bool) -> Result<Vec<InputRecord>> {
    let input =
        fs::read(path).with_context(|| format!("failed to read input file {}", path.display()))?;
    Ok(parse_candidate_bytes(&input, read0))
}

pub(crate) fn parse_candidate_bytes(input: &[u8], read0: bool) -> Vec<InputRecord> {
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

pub(crate) fn parse_line_records(input: &[u8]) -> Vec<InputRecord> {
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

pub(crate) fn default_source_command() -> Option<(&'static str, String)> {
    for env_name in ["YURU_DEFAULT_COMMAND", "FZF_DEFAULT_COMMAND"] {
        if let Ok(command) = std::env::var(env_name) {
            return Some((env_name, command));
        }
    }
    None
}

pub(crate) fn non_empty_default_source_command() -> Option<(&'static str, String)> {
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
pub(crate) fn default_command_process(command: &str) -> std::process::Command {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let mut process = std::process::Command::new(shell);
    process.arg("-c").arg(command);
    process
}

#[cfg(windows)]
pub(crate) fn default_command_process(command: &str) -> std::process::Command {
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
pub(crate) struct WalkerOptions {
    pub(crate) files: bool,
    pub(crate) dirs: bool,
    pub(crate) follow: bool,
    pub(crate) hidden: bool,
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

pub(crate) fn parse_walker_options(raw: &str) -> Result<WalkerOptions> {
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

pub(crate) fn parse_walker_skip(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn walker_entry_allowed(
    entry: &DirEntry,
    skips: &[String],
    include_hidden: bool,
) -> bool {
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
