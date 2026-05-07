use std::path::Path;
use std::process::Command;

use super::cache::PreviewPayload;

const ASCII_TEXT_SNIFF_BYTES: usize = 8192;

pub(super) fn run_builtin_preview(item: &str, text_extensions: &[String]) -> PreviewPayload {
    let path = Path::new(item);
    if item.trim().is_empty() {
        return PreviewPayload::Text("no selection".to_string());
    }
    if path.is_dir() {
        return PreviewPayload::Text(preview_directory(path));
    }
    if !path.exists() {
        return PreviewPayload::Text(format!("missing: {item}"));
    }
    if !path.is_file() {
        return PreviewPayload::Text(preview_path_metadata(path));
    }
    if path.metadata().is_ok_and(|metadata| metadata.len() == 0) {
        return PreviewPayload::Text(format!("empty file: {item}"));
    }
    if is_text_path(path, text_extensions) || is_ascii_text_file(path) {
        return PreviewPayload::Text(preview_text_file(path));
    }
    PreviewPayload::Text(preview_path_metadata(path))
}

fn preview_directory(path: &Path) -> String {
    let mut entries = match std::fs::read_dir(path) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| {
                let suffix = entry
                    .file_type()
                    .ok()
                    .filter(|file_type| file_type.is_dir())
                    .map(|_| "/")
                    .unwrap_or_default();
                format!("{}{}", entry.file_name().to_string_lossy(), suffix)
            })
            .collect::<Vec<_>>(),
        Err(error) => return format!("directory: {}\nerror: {error}", path.display()),
    };
    entries.sort();
    let mut output = format!("directory: {}\n\n", path.display());
    for entry in entries.into_iter().take(120) {
        output.push_str(&entry);
        output.push('\n');
    }
    output
}

fn preview_text_file(path: &Path) -> String {
    if let Some(output) = preview_text_with_bat(path) {
        return output;
    }
    if let Some(output) = preview_text_with_cat(path) {
        return limit_preview_lines(&output, 200);
    }
    match std::fs::read(path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            limit_preview_lines(&text, 200)
        }
        Err(error) => format!("file: {}\nerror: {error}", path.display()),
    }
}

fn preview_text_with_bat(path: &Path) -> Option<String> {
    let output = Command::new("bat")
        .args([
            "--style=numbers",
            "--color=never",
            "--paging=never",
            "--line-range",
            ":200",
            "--",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn preview_text_with_cat(path: &Path) -> Option<String> {
    let output = Command::new("cat").arg("--").arg(path).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn limit_preview_lines(text: &str, limit: usize) -> String {
    let mut output = String::new();
    for (index, line) in text.lines().enumerate() {
        if index >= limit {
            output.push_str("...\n");
            break;
        }
        output.push_str(line);
        output.push('\n');
    }
    output
}

fn preview_path_metadata(path: &Path) -> String {
    match path.metadata() {
        Ok(metadata) => format!(
            "file: {}\nsize: {} bytes\npreview: no text preview for this file type",
            path.display(),
            metadata.len()
        ),
        Err(error) => format!("file: {}\nerror: {error}", path.display()),
    }
}

fn is_text_path(path: &Path, text_extensions: &[String]) -> bool {
    let Some(extension) = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
    else {
        return false;
    };
    text_extensions.iter().any(|item| {
        item.trim_start_matches('.')
            .eq_ignore_ascii_case(&extension)
    })
}

fn is_ascii_text_file(path: &Path) -> bool {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let sample = &bytes[..bytes.len().min(ASCII_TEXT_SNIFF_BYTES)];
    !sample.is_empty() && sample.iter().all(|byte| is_ascii_text_byte(*byte))
}

fn is_ascii_text_byte(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\r' | 0x0c | 0x20..=0x7e)
}
