use crate::api::PreviewCommand;
#[cfg(unix)]
use crate::preview::PreviewGeometry;
use crate::preview::{run_preview_command, PreviewCache, PreviewPayload};
#[cfg(unix)]
use crate::state::TuiState;
#[cfg(unix)]
use yuru_core::KeyKind;

#[cfg(unix)]
use super::helpers::scored;
#[cfg(feature = "image")]
use super::helpers::tiny_png_bytes;
use super::helpers::{preview_key, test_geometry};

#[test]
fn preview_scroll_clamps_to_visible_lines() {
    let mut cache = PreviewCache::default();
    cache.replace(
        preview_key("cat {}", 0, "alpha"),
        PreviewPayload::Text("one\ntwo\nthree".to_string()),
    );

    cache.scroll_down(10, 2);
    assert_eq!(cache.scroll, 1);

    cache.scroll_up(10, 2);
    assert_eq!(cache.scroll, 0);
}

#[test]
fn preview_scroll_resets_when_preview_key_changes() {
    let mut cache = PreviewCache::default();
    cache.replace(
        preview_key("cat {}", 0, "alpha"),
        PreviewPayload::Text("one\ntwo\nthree".to_string()),
    );
    cache.scroll_down(1, 2);

    cache.replace(
        preview_key("cat {}", 1, "beta"),
        PreviewPayload::Text("four\nfive\nsix".to_string()),
    );

    assert_eq!(cache.scroll, 0);
}
#[cfg(unix)]
#[test]
fn preview_command_returns_stderr_when_stdout_is_empty() {
    let preview = run_preview_command(
        &PreviewCommand::Shell("printf preview-error >&2".to_string()),
        None,
        "alpha",
        test_geometry(),
        None,
    );

    assert!(matches!(preview, PreviewPayload::Text(text) if text == "preview-error"));
}

#[cfg(unix)]
#[test]
fn preview_command_gets_fzf_preview_geometry_env() {
    let preview = run_preview_command(
        &PreviewCommand::Shell("printf '%s,%s,%s,%s' \"$FZF_PREVIEW_COLUMNS\" \"$FZF_PREVIEW_LINES\" \"$FZF_PREVIEW_LEFT\" \"$FZF_PREVIEW_TOP\"".to_string()),
        None,
        "alpha",
        PreviewGeometry {
            columns: 40,
            lines: 12,
            left: 41,
            top: 1,
        },
        None,
    );

    assert!(matches!(preview, PreviewPayload::Text(text) if text == "40,12,41,1"));
}

#[test]
fn builtin_preview_reads_configured_text_extension() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "alpha\nbeta\n").unwrap();

    let preview = run_preview_command(
        &PreviewCommand::Builtin {
            text_extensions: vec!["md".to_string()],
        },
        None,
        path.to_str().unwrap(),
        test_geometry(),
        None,
    );

    assert!(matches!(preview, PreviewPayload::Text(text) if text.contains("alpha")));
}

#[test]
fn builtin_preview_reads_ascii_text_without_configured_extension() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.unknown");
    std::fs::write(&path, "ascii alpha\nascii beta\n").unwrap();

    let preview = run_preview_command(
        &PreviewCommand::Builtin {
            text_extensions: Vec::new(),
        },
        None,
        path.to_str().unwrap(),
        test_geometry(),
        None,
    );

    assert!(matches!(preview, PreviewPayload::Text(text) if text.contains("ascii alpha")));
}

#[test]
fn builtin_preview_truncates_a_huge_first_line_before_full_file_capture() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("huge.txt");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(2 * 1024 * 1024).unwrap();

    let preview = run_preview_command(
        &PreviewCommand::Builtin {
            text_extensions: vec!["txt".to_string()],
        },
        None,
        path.to_str().unwrap(),
        test_geometry(),
        None,
    );

    assert!(matches!(
        preview,
        PreviewPayload::Text(text)
            if text.len() <= 1024 * 1024 + 128 && text.contains("truncated")
    ));
}

#[cfg(unix)]
#[test]
fn shell_preview_truncates_unbounded_output() {
    let preview = run_preview_command(
        &PreviewCommand::Shell("yes preview".to_string()),
        None,
        "alpha",
        test_geometry(),
        None,
    );

    assert!(matches!(
        preview,
        PreviewPayload::Text(text)
            if text.len() <= 1024 * 1024 + 128 && text.contains("truncated")
    ));
}

#[cfg(unix)]
#[test]
fn clearing_preview_cancels_and_joins_the_active_child() {
    let mut cache = PreviewCache::default();
    let results = vec![scored("alpha", KeyKind::Original)];
    let state = TuiState::new("");
    cache.request_for_selection(
        Some(&PreviewCommand::Shell("sleep 30".to_string())),
        None,
        &results,
        &state,
        Some(test_geometry()),
        None,
    );
    std::thread::sleep(std::time::Duration::from_millis(60));
    cache.poll();

    let started = std::time::Instant::now();
    cache.request_for_selection(None, None, &results, &state, Some(test_geometry()), None);
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    assert!(cache.next_poll_timeout().is_none());
}

#[test]
fn builtin_preview_does_not_read_binary_unknown_extension_as_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blob.unknown");
    std::fs::write(&path, [0, 159, 146, 150]).unwrap();

    let preview = run_preview_command(
        &PreviewCommand::Builtin {
            text_extensions: Vec::new(),
        },
        None,
        path.to_str().unwrap(),
        test_geometry(),
        None,
    );

    assert!(matches!(preview, PreviewPayload::Text(text) if text.contains("no text preview")));
}

#[test]
fn builtin_preview_reports_empty_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.txt");
    std::fs::write(&path, "").unwrap();

    let preview = run_preview_command(
        &PreviewCommand::Builtin {
            text_extensions: vec!["txt".to_string()],
        },
        None,
        path.to_str().unwrap(),
        test_geometry(),
        None,
    );

    assert!(matches!(preview, PreviewPayload::Text(text) if text.contains("empty file")));
}

#[cfg(feature = "image")]
#[test]
fn builtin_preview_reports_image_metadata_when_protocol_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.png");
    std::fs::write(&path, tiny_png_bytes()).unwrap();

    let preview = run_preview_command(
        &PreviewCommand::Builtin {
            text_extensions: Vec::new(),
        },
        None,
        path.to_str().unwrap(),
        test_geometry(),
        None,
    );

    assert!(matches!(
        preview,
        PreviewPayload::Text(text)
            if text.contains("format: PNG")
                && text.contains("dimensions: 1 x 1")
                && text.contains("preview: image rendering disabled")
    ));
}
