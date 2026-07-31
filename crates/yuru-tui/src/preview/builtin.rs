use std::path::Path;
use std::process::Command;
use std::{fs::File, io::Read};

use super::cache::PreviewPayload;
#[cfg(feature = "image")]
use super::image::preview_image_metadata_from_path;
use super::process::{run_bounded_process, MAX_PREVIEW_OUTPUT_BYTES};
use super::PreviewCancellation;

const ASCII_TEXT_SNIFF_BYTES: usize = 8192;

pub(super) fn run_builtin_preview(
    item: &str,
    text_extensions: &[String],
    cancellation: &PreviewCancellation,
) -> PreviewPayload {
    if cancellation.is_cancelled() {
        return PreviewPayload::Text("preview cancelled".to_string());
    }
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
        return PreviewPayload::Text(preview_text_file(path, cancellation));
    }
    #[cfg(feature = "image")]
    if let Some(metadata) = preview_image_metadata_from_path(path) {
        return PreviewPayload::Text(metadata);
    }
    PreviewPayload::Text(preview_path_metadata(path))
}

fn preview_directory(path: &Path) -> String {
    let mut entries = match std::fs::read_dir(path) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .take(121)
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

fn preview_text_file(path: &Path, cancellation: &PreviewCancellation) -> String {
    if let Some(output) = preview_text_with_bat(path, cancellation) {
        return output;
    }
    if cancellation.is_cancelled() {
        return "preview cancelled".to_string();
    }
    match read_bounded_file(path) {
        Ok((bytes, truncated)) => format_bounded_text(&bytes, truncated),
        Err(error) => format!("file: {}\nerror: {error}", path.display()),
    }
}

fn preview_text_with_bat(path: &Path, cancellation: &PreviewCancellation) -> Option<String> {
    let mut process = Command::new("bat");
    process
        .args([
            "--style=numbers",
            "--color=never",
            "--paging=never",
            "--line-range",
            ":200",
            "--",
        ])
        .arg(path);
    let output = run_bounded_process(&mut process, cancellation).ok()?;
    if output.cancelled
        || output.timed_out
        || !output.status.is_some_and(|status| status.success())
        || output.stdout.is_empty()
    {
        return None;
    }
    Some(format_bounded_text(&output.stdout, output.truncated))
}

fn read_bounded_file(path: &Path) -> std::io::Result<(Vec<u8>, bool)> {
    let file = File::open(path)?;
    let mut bytes = Vec::with_capacity(16 * 1024);
    file.take((MAX_PREVIEW_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    let truncated = bytes.len() > MAX_PREVIEW_OUTPUT_BYTES;
    bytes.truncate(MAX_PREVIEW_OUTPUT_BYTES);
    Ok((bytes, truncated))
}

fn format_bounded_text(bytes: &[u8], byte_truncated: bool) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut output = limit_preview_lines(&text, 200);
    if byte_truncated {
        output.push_str("... preview truncated at 1 MiB ...\n");
    }
    output
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
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut sample = [0u8; ASCII_TEXT_SNIFF_BYTES];
    let len = match file.read(&mut sample) {
        Ok(len) => len,
        Err(_) => return false,
    };
    let sample = &sample[..len];
    !sample.is_empty() && sample.iter().all(|byte| is_ascii_text_byte(*byte))
}

fn is_ascii_text_byte(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\r' | 0x0c | 0x20..=0x7e)
}
