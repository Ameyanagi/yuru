use std::process::Command;

use crate::api::{ImagePreviewProtocol, PreviewCommand};

use super::builtin::run_builtin_preview;
use super::cache::{PreviewGeometry, PreviewPayload};
#[cfg(feature = "image")]
use super::image::{
    preview_image_from_output, preview_image_from_path_text, preview_image_metadata_from_output,
    preview_image_metadata_from_path_text,
};
use super::process::run_bounded_process;
use super::PreviewCancellation;

#[cfg(test)]
pub(crate) fn run_preview_command(
    command: &PreviewCommand,
    shell: Option<&str>,
    item: &str,
    geometry: PreviewGeometry,
    image_protocol: Option<ImagePreviewProtocol>,
) -> PreviewPayload {
    run_preview_command_cancellable(
        command,
        shell,
        item,
        geometry,
        image_protocol,
        &PreviewCancellation::default(),
    )
}

pub(super) fn run_preview_command_cancellable(
    command: &PreviewCommand,
    shell: Option<&str>,
    item: &str,
    geometry: PreviewGeometry,
    image_protocol: Option<ImagePreviewProtocol>,
    cancellation: &PreviewCancellation,
) -> PreviewPayload {
    #[cfg(not(feature = "image"))]
    let _ = image_protocol;

    #[cfg(feature = "image")]
    {
        if image_protocol.is_some() {
            if let Some(image) = preview_image_from_path_text(item) {
                return PreviewPayload::Image(image);
            }
        } else if let Some(metadata) = preview_image_metadata_from_path_text(item) {
            return PreviewPayload::Text(metadata);
        }
    }

    match command {
        PreviewCommand::Shell(template) => run_shell_preview_command(
            template,
            shell,
            item,
            geometry,
            image_protocol,
            cancellation,
        ),
        PreviewCommand::Builtin { text_extensions } => {
            run_builtin_preview(item, text_extensions, cancellation)
        }
    }
}

fn run_shell_preview_command(
    template: &str,
    shell: Option<&str>,
    item: &str,
    geometry: PreviewGeometry,
    image_protocol: Option<ImagePreviewProtocol>,
    cancellation: &PreviewCancellation,
) -> PreviewPayload {
    #[cfg(not(feature = "image"))]
    let _ = image_protocol;

    let command = expand_preview_template(template, item);
    let mut process = preview_shell_command(&command, shell, geometry);
    let output = run_bounded_process(&mut process, cancellation);

    match output {
        Ok(output) => {
            if output.cancelled {
                return PreviewPayload::Text("preview cancelled".to_string());
            }
            if output.timed_out {
                return PreviewPayload::Text("preview timed out after 5 seconds".to_string());
            }
            let truncation_notice = if output.truncated {
                "\n... preview output truncated ...\n"
            } else {
                ""
            };
            if !output.stdout.is_empty() {
                #[cfg(feature = "image")]
                {
                    if image_protocol.is_some() {
                        if let Some(image) = preview_image_from_output(&output.stdout) {
                            return PreviewPayload::Image(image);
                        }
                    } else if let Some(metadata) =
                        preview_image_metadata_from_output(&output.stdout)
                    {
                        return PreviewPayload::Text(metadata);
                    }
                }
                #[cfg(feature = "image")]
                {
                    if image_protocol.is_some()
                        && output.status.is_some_and(|status| status.success())
                    {
                        if let Some(image) = preview_image_from_path_text(item) {
                            return PreviewPayload::Image(image);
                        }
                    }
                }
                let stdout = String::from_utf8_lossy(&output.stdout);
                return PreviewPayload::Text(format!("{stdout}{truncation_notice}"));
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                return PreviewPayload::Text(format!("{stderr}{truncation_notice}"));
            }
            if output.status.is_some_and(|status| status.success()) {
                #[cfg(feature = "image")]
                {
                    if image_protocol.is_some() {
                        if let Some(image) = preview_image_from_path_text(item) {
                            return PreviewPayload::Image(image);
                        }
                    }
                }
                PreviewPayload::Text(String::new())
            } else {
                PreviewPayload::Text(
                    output
                        .status
                        .map(|status| format!("preview exited with {status}"))
                        .unwrap_or_else(|| "preview process stopped".to_string()),
                )
            }
        }
        Err(error) => PreviewPayload::Text(format!("preview failed: {error}")),
    }
}

fn expand_preview_template(template: &str, item: &str) -> String {
    if template.contains("{}") {
        template.replace("{}", &shell_quote(item))
    } else {
        template.to_string()
    }
}

#[cfg(not(windows))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(windows)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(not(windows))]
fn preview_shell_command(command: &str, shell: Option<&str>, geometry: PreviewGeometry) -> Command {
    let shell = shell
        .map(str::to_string)
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(|| "sh".to_string());
    let mut parts = shell.split_whitespace();
    let program = parts.next().unwrap_or("sh");
    let mut process = Command::new(program);
    let shell_args: Vec<_> = parts.collect();
    if shell_args.is_empty() {
        process.arg("-c");
    } else {
        process.args(shell_args);
    }
    process.arg(command);
    apply_preview_env(&mut process, geometry);
    process
}

#[cfg(windows)]
fn preview_shell_command(command: &str, shell: Option<&str>, geometry: PreviewGeometry) -> Command {
    let shell = shell
        .map(str::to_string)
        .or_else(|| std::env::var("YURU_WINDOWS_SHELL").ok())
        .unwrap_or_else(|| "powershell.exe".to_string());
    let mut parts = shell.split_whitespace();
    let program = parts.next().unwrap_or("powershell.exe");
    let mut process = Command::new(program);
    let shell_args: Vec<_> = parts.collect();
    if shell_args.is_empty() {
        process.args(["-NoLogo", "-NoProfile", "-Command"]);
    } else {
        process.args(shell_args);
    }
    process.arg(command);
    apply_preview_env(&mut process, geometry);
    process
}

fn apply_preview_env(process: &mut Command, geometry: PreviewGeometry) {
    process
        .env("FZF_PREVIEW_COLUMNS", geometry.columns.to_string())
        .env("FZF_PREVIEW_LINES", geometry.lines.to_string())
        .env("FZF_PREVIEW_LEFT", geometry.left.to_string())
        .env("FZF_PREVIEW_TOP", geometry.top.to_string());
}
