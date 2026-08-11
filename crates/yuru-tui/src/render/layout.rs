use std::borrow::Cow;

use crossterm::terminal;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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

/// Longest byte run at the front of `text` that the terminal renders as one
/// indivisible unit — a grapheme cluster, so a base character keeps its
/// combining marks, emoji modifiers, variation selectors, and ZWJ partners.
///
/// Returns `None` when the cluster might extend past the scan window, which
/// only happens for a pathological run of continuation scalars. Callers stop
/// there rather than emit half a cluster, and the window keeps a single record
/// from costing an unbounded scan.
pub(super) fn leading_cluster(text: &str) -> Option<&str> {
    const MAX_CLUSTER_SCAN_BYTES: usize = 256;

    let window = &text[..floor_char_boundary(text, MAX_CLUSTER_SCAN_BYTES)];
    let cluster = window.graphemes(true).next()?;
    (cluster.len() < window.len() || window.len() == text.len()).then_some(cluster)
}

fn floor_char_boundary(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    let mut end = index;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// Display width of one grapheme cluster in terminal columns.
///
/// Measured across the whole cluster rather than summed per scalar, because the
/// per-scalar sum is wrong in both directions: an emoji modifier sequence such
/// as `👩` + U+1F3FD sums to four columns but prints in two, while a keycap
/// sequence such as `#` + U+FE0F + U+20E3 sums to one column but prints in two.
/// Zero-width continuation scalars therefore cost nothing on their own.
pub(super) fn cluster_display_width(cluster: &str) -> usize {
    UnicodeWidthStr::width(cluster)
}

/// Display width of escape-free text in terminal columns: the sum of its
/// grapheme clusters' widths.
///
/// This, not `UnicodeWidthStr::width`, is the module's one width metric. The
/// whole-string measure is not additive across cluster boundaries — it collapses
/// a ZWJ-joined pair of non-pictographic clusters such as `🇯🇵` + U+200D + `🇯🇵`
/// to two columns where the clusters report two each — so mixing the two would
/// let the truncation budget and the `bg+` padding accounting disagree and
/// overrun the row. Summing clusters is additive at every cluster boundary,
/// which is the only place this module ever splits text.
pub(crate) fn display_width(text: &str) -> usize {
    text.graphemes(true).map(cluster_display_width).sum()
}

/// Returns the longest prefix of `text` that fits within `max_columns` display
/// columns. A cluster that would straddle the boundary is dropped whole rather
/// than split, so the result never exceeds the budget and never ends in an
/// orphaned combining mark.
///
/// The input must already be free of escape sequences: every character counts
/// against the budget.
pub(super) fn width_prefix(text: &str, max_columns: usize) -> &str {
    let mut columns = 0usize;
    let mut end = 0usize;
    for cluster in text.graphemes(true) {
        let next = columns.saturating_add(cluster_display_width(cluster));
        if next > max_columns {
            break;
        }
        columns = next;
        end += cluster.len();
    }
    &text[..end]
}

/// Clips `text` to `width` display columns, replacing the tail with `ellipsis`
/// when anything was dropped. Both budgets are columns, so `".."` costs two and
/// a user-supplied `"…"` costs one.
pub(crate) fn truncate_to_width_with_ellipsis(text: &str, width: usize, ellipsis: &str) -> String {
    let ellipsis = terminal_safe_text(ellipsis);
    if width == 0 {
        return String::new();
    }
    let (text, truncated) = terminal_safe_prefix(text, false, width);
    if !truncated {
        return text;
    }

    // The ellipsis budget is in columns too: ".." costs two, "…" costs one.
    let ellipsis_width = display_width(ellipsis.as_ref()).min(width);
    let mut out = width_prefix(&text, width.saturating_sub(ellipsis_width)).to_string();
    out.push_str(width_prefix(ellipsis.as_ref(), ellipsis_width));
    out
}

/// Converts terminal controls in lower-trust text to inert, visible,
/// one-character representations. The one-to-one mapping preserves the
/// character indices used by fuzzy highlighting and phonetic source maps.
pub(super) fn terminal_safe_text(text: &str) -> Cow<'_, str> {
    sanitize_terminal_text(text, false)
}

/// Sanitizes only the visible prefix required by the viewport, budgeting by
/// display columns rather than characters so wide CJK and emoji rows cannot
/// overflow the row and wrap.
///
/// ANSI SGR bytes and zero-width characters consume no display width, so the
/// total scanned size is separately bounded to prevent control-only records
/// from bypassing the viewport limit. The budget advances one grapheme cluster
/// at a time: a cluster that does not fit in the remaining columns is dropped
/// whole rather than half-emitted, so a wide character is never split and a
/// combining mark or emoji modifier is never separated from its base.
///
/// Only a cluster that costs columns can exhaust the budget, so the scan runs
/// on past a full budget through anything free — SGR sequences and zero-width
/// clusters — and stops at the first cluster that would actually overflow.
/// Two things follow, and both are load-bearing. A combining scalar reaches its
/// base even when an SGR sequence sits between them, which grapheme
/// segmentation alone cannot deliver because a control byte ends a cluster. And
/// a zero-width tail is consumed rather than left behind, so text that occupies
/// exactly the budget does not report itself truncated and collect an ellipsis
/// it never earned.
pub(crate) fn terminal_safe_prefix(
    text: &str,
    allow_sgr: bool,
    max_visible: usize,
) -> (String, bool) {
    if max_visible == 0 {
        return (String::new(), !text.is_empty());
    }

    let max_scanned_bytes = max_visible.saturating_mul(64).clamp(64, 64 * 1024);
    let mut out = String::with_capacity(text.len().min(max_visible.saturating_mul(4)));
    let mut offset = 0usize;
    let mut visible = 0usize;
    let mut safe_cluster = String::new();
    while offset < text.len() && offset < max_scanned_bytes {
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

        // A control character always ends a grapheme cluster, so an SGR
        // sequence can never begin inside the cluster taken here.
        let Some(cluster) = leading_cluster(&text[offset..]) else {
            break;
        };
        if offset.saturating_add(cluster.len()) > max_scanned_bytes {
            break;
        }
        // Width is measured on the sanitized replacement, since that is what
        // the terminal actually prints.
        safe_cluster.clear();
        safe_cluster.extend(cluster.chars().map(visible_control_char));
        let cluster_width = cluster_display_width(&safe_cluster);
        if visible.saturating_add(cluster_width) > max_visible {
            break;
        }
        out.push_str(&safe_cluster);
        visible += cluster_width;
        offset += cluster.len();
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
