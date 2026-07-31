use std::borrow::Cow;

use crossterm::terminal;

use crate::api::TuiLayout;
use crate::preview::{PreviewGeometry, PreviewRender};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Viewport {
    pub(crate) width: usize,
    pub(crate) rows: usize,
}

impl Viewport {
    pub(crate) fn from_terminal(height: Option<usize>, reserve_prompt_row: bool) -> Self {
        let (width, terminal_rows) = terminal::size().unwrap_or((80, 24));
        let max_rows = if reserve_prompt_row {
            usize::from(terminal_rows).saturating_sub(1).max(1)
        } else {
            usize::from(terminal_rows).max(1)
        };
        Self {
            width: usize::from(width).max(1),
            rows: height.unwrap_or(max_rows).clamp(1, max_rows),
        }
    }
}

pub(super) fn visible_line_count(text: Option<&str>) -> usize {
    text.map(|text| text.lines().count()).unwrap_or(0)
}

pub(super) fn content_start_row(layout: TuiLayout, has_prompt: bool) -> usize {
    usize::from(has_prompt && !layout.prompt_at_bottom())
}

pub(super) fn footer_start_row(
    content_top: usize,
    viewport_rows: usize,
    footer_rows: usize,
) -> usize {
    if footer_rows == 0 {
        return 0;
    }

    content_top
        .saturating_add(viewport_rows)
        .saturating_sub(footer_rows)
}

pub(super) fn preview_width(total_width: usize, preview: Option<&PreviewRender<'_>>) -> usize {
    preview_width_for_presence(total_width, preview.is_some())
}

fn preview_width_for_presence(total_width: usize, has_preview: bool) -> usize {
    if !has_preview || total_width < 30 {
        0
    } else {
        (total_width / 2).clamp(12, total_width.saturating_sub(12))
    }
}

pub(crate) fn preview_geometry(
    viewport: Viewport,
    layout: TuiLayout,
    has_prompt: bool,
    has_preview: bool,
) -> Option<PreviewGeometry> {
    let columns = preview_width_for_presence(viewport.width, has_preview);
    if columns == 0 {
        return None;
    }
    let left = viewport.width.saturating_sub(columns);
    let top = content_start_row(layout, has_prompt);
    Some(PreviewGeometry {
        columns,
        lines: viewport.rows,
        left,
        top,
    })
}

pub(super) fn scroll_offset(selected: usize, len: usize, rows: usize) -> usize {
    if len == 0 || selected < rows {
        0
    } else {
        selected + 1 - rows
    }
}

pub(super) fn truncate_to_width_with_ellipsis(text: &str, width: usize, ellipsis: &str) -> String {
    let ellipsis = terminal_safe_text(ellipsis);
    if width == 0 {
        return String::new();
    }
    let (text, truncated) = terminal_safe_prefix(text, false, width);
    if !truncated {
        return text;
    }

    let ellipsis_width = ellipsis.chars().count().min(width);
    let mut out: String = text
        .chars()
        .take(width.saturating_sub(ellipsis_width))
        .collect();
    out.extend(ellipsis.chars().take(ellipsis_width));
    out
}

/// Converts terminal controls in lower-trust text to inert, visible,
/// one-character representations. The one-to-one mapping preserves the
/// character indices used by fuzzy highlighting and phonetic source maps.
pub(super) fn terminal_safe_text(text: &str) -> Cow<'_, str> {
    sanitize_terminal_text(text, false)
}

/// Sanitizes only the visible prefix required by the viewport. ANSI SGR bytes
/// do not consume display width, so their total scanned size is separately
/// bounded to prevent control-only records from bypassing the viewport limit.
pub(super) fn terminal_safe_prefix(
    text: &str,
    allow_sgr: bool,
    max_visible: usize,
) -> (String, bool) {
    if max_visible == 0 {
        return (String::new(), !text.is_empty());
    }

    let max_scanned_bytes = if allow_sgr {
        max_visible.saturating_mul(64).clamp(64, 64 * 1024)
    } else {
        usize::MAX
    };
    let mut out = String::with_capacity(text.len().min(max_visible.saturating_mul(4)));
    let mut offset = 0usize;
    let mut visible = 0usize;
    while offset < text.len() && visible < max_visible && offset < max_scanned_bytes {
        if allow_sgr {
            if let Some(len) = safe_sgr_sequence_len(&text[offset..]) {
                if offset.saturating_add(len) > max_scanned_bytes {
                    break;
                }
                out.push_str(&text[offset..offset + len]);
                offset += len;
                continue;
            }
        }

        let ch = text[offset..]
            .chars()
            .next()
            .expect("valid character boundary");
        out.push(visible_control_char(ch));
        visible += 1;
        offset += ch.len_utf8();
    }
    (out, offset < text.len())
}

pub(super) fn safe_sgr_sequence_len(text: &str) -> Option<usize> {
    const MAX_SGR_SEQUENCE_BYTES: usize = 64;
    let bytes = text.as_bytes();
    if !bytes.starts_with(b"\x1b[") {
        return None;
    }
    for (offset, byte) in bytes[2..].iter().copied().enumerate() {
        if offset + 3 > MAX_SGR_SEQUENCE_BYTES {
            return None;
        }
        if byte == b'm' {
            return Some(offset + 3);
        }
        if !byte.is_ascii_digit() && byte != b';' && byte != b':' {
            return None;
        }
    }
    None
}

pub(super) fn terminal_visible_text(text: &str) -> Cow<'_, str> {
    if !text.contains('\u{1b}') {
        return Cow::Borrowed(text);
    }

    let mut out = String::with_capacity(text.len());
    let mut offset = 0;
    while offset < text.len() {
        if let Some(len) = safe_sgr_sequence_len(&text[offset..]) {
            offset += len;
            continue;
        }
        let ch = text[offset..]
            .chars()
            .next()
            .expect("valid character boundary");
        out.push(ch);
        offset += ch.len_utf8();
    }
    Cow::Owned(out)
}

fn sanitize_terminal_text(text: &str, allow_sgr: bool) -> Cow<'_, str> {
    if !text.chars().any(char::is_control) {
        return Cow::Borrowed(text);
    }

    let mut out = String::with_capacity(text.len());
    let mut offset = 0;
    while offset < text.len() {
        if allow_sgr {
            if let Some(len) = safe_sgr_sequence_len(&text[offset..]) {
                out.push_str(&text[offset..offset + len]);
                offset += len;
                continue;
            }
        }

        let ch = text[offset..]
            .chars()
            .next()
            .expect("valid character boundary");
        out.push(visible_control_char(ch));
        offset += ch.len_utf8();
    }
    Cow::Owned(out)
}

fn visible_control_char(ch: char) -> char {
    match ch as u32 {
        0x00..=0x1f => char::from_u32(0x2400 + ch as u32).unwrap_or('\u{fffd}'),
        0x7f => '\u{2421}',
        // UTF-8 C1 characters are not single-byte terminal controls, but
        // replacing them keeps every control character visible.
        _ if ch.is_control() => '\u{fffd}',
        _ => ch,
    }
}
