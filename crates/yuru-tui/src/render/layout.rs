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

/// Longest a single grapheme cluster is allowed to be before the scan gives up
/// on it, which keeps one pathological record from costing an unbounded scan.
const MAX_CLUSTER_SCAN_BYTES: usize = 256;

/// Longest byte run at the front of `text` that the terminal renders as one
/// indivisible unit — a grapheme cluster, so a base character keeps its
/// combining marks, emoji modifiers, variation selectors, and ZWJ partners.
///
/// Returns `None` when the cluster might extend past the scan window, which
/// only happens for a pathological run of continuation scalars. Callers stop
/// there rather than emit half a cluster, and the window keeps a single record
/// from costing an unbounded scan.
pub(super) fn leading_cluster(text: &str) -> Option<&str> {
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

/// Escape-free view of escape-interleaved text, with the SGR sequences lifted
/// out and keyed by where they sit in that view.
///
/// A control byte ends a grapheme cluster, so segmenting the raw string splits
/// every cluster an SGR sequence was written inside — and then charges the
/// halves a width the terminal does not print, in both directions. A keycap
/// written as `#`, `ESC[31m`, U+FE0F, U+20E3 splits into a one-column `#` and a
/// zero-width tail: one column for something composed in two. A modifier
/// sequence written as `👩`, `ESC[0m`, U+1F3FB splits into two clusters of two
/// columns each: four for something that also prints in two. Segmenting this
/// view instead, then mapping the clusters back onto the source, charges what
/// is actually painted.
pub(super) struct SgrSplit<'a> {
    visible: Cow<'a, str>,
    /// Every SGR sequence with its byte offset in `visible`, in source order.
    sgrs: Vec<(usize, &'a str)>,
    /// Bytes of the source the scan consumed.
    scanned: usize,
    /// Where the byte cap fell: the length of `visible`, and the number of
    /// sequences recorded, at the moment the scan crossed it. Nothing past the
    /// cap may start a cluster or contribute styling of its own.
    capped: (usize, usize),
}

impl<'a> SgrSplit<'a> {
    /// Splits `text` up to a `max_scanned_bytes` cap on the source.
    ///
    /// The scan runs one cluster's worth past that cap, so a cluster starting
    /// inside it is seen whole rather than cut in half by the cap itself: the
    /// cap bounds where a cluster may begin, not where its bytes may end.
    ///
    /// `sanitize_controls` replaces control characters with visible
    /// one-character stand-ins, the form whose width the terminal will actually
    /// print.
    pub(super) fn new(
        text: &'a str,
        allow_sgr: bool,
        sanitize_controls: bool,
        max_scanned_bytes: usize,
    ) -> Self {
        let scan_end = max_scanned_bytes.saturating_add(MAX_CLUSTER_SCAN_BYTES);
        let scannable = floor_char_boundary(text, scan_end);
        // Text without control characters has nothing to lift out and nothing
        // to replace, so its view is the source itself.
        if !text[..scannable].chars().any(char::is_control) {
            return Self {
                visible: Cow::Borrowed(&text[..scannable]),
                sgrs: Vec::new(),
                scanned: scannable,
                capped: (floor_char_boundary(text, max_scanned_bytes), 0),
            };
        }

        let mut visible = String::with_capacity(scannable);
        let mut sgrs = Vec::new();
        let mut offset = 0usize;
        let mut capped = None;
        while offset < text.len() {
            if capped.is_none() && offset >= max_scanned_bytes {
                capped = Some((visible.len(), sgrs.len()));
            }
            if allow_sgr {
                if let Some(len) = safe_sgr_sequence_len(&text[offset..]) {
                    if offset.saturating_add(len) > scan_end {
                        break;
                    }
                    // Only a cluster is worth reaching past the cap for, so a
                    // sequence that straddles it is what ends the capped part.
                    if capped.is_none() && offset.saturating_add(len) > max_scanned_bytes {
                        capped = Some((visible.len(), sgrs.len()));
                    }
                    sgrs.push((visible.len(), &text[offset..offset + len]));
                    offset += len;
                    continue;
                }
            }

            let ch = text[offset..]
                .chars()
                .next()
                .expect("valid character boundary");
            if offset.saturating_add(ch.len_utf8()) > scan_end {
                break;
            }
            visible.push(if sanitize_controls {
                visible_control_char(ch)
            } else {
                ch
            });
            offset += ch.len_utf8();
        }

        let capped = capped.unwrap_or((visible.len(), sgrs.len()));
        Self {
            visible: Cow::Owned(visible),
            sgrs,
            scanned: offset,
            capped,
        }
    }

    pub(super) fn visible(&self) -> &str {
        &self.visible
    }

    /// Byte offset in the escape-free view past which no cluster may start.
    fn cluster_start_limit(&self) -> usize {
        self.capped.0
    }

    /// Whether the scan reached the end of `text` rather than the byte cap.
    fn is_complete(&self, text: &str) -> bool {
        self.scanned == text.len()
    }

    /// The `index`-th SGR sequence and where it sits in the escape-free view.
    pub(super) fn sgr(&self, index: usize) -> Option<(usize, &'a str)> {
        self.sgrs.get(index).copied()
    }

    /// The escape-free prefix ending at `end`, with every SGR sequence put back
    /// where it was written — including any at exactly `end`, so a trailing
    /// reset at the viewport boundary is retained and styling cannot leak past
    /// the row. Sequences beyond the byte cap are dropped with the rest of it.
    fn render_prefix(&self, end: usize) -> String {
        let visible = &self.visible[..end];
        let sgrs = &self.sgrs[..self.capped.1.min(self.sgrs.len())];
        if sgrs.is_empty() {
            return visible.to_string();
        }

        let mut out = String::with_capacity(visible.len() + sgrs.len() * 8);
        let mut cursor = 0usize;
        for (offset, sequence) in sgrs {
            if *offset > end {
                break;
            }
            out.push_str(&visible[cursor..*offset]);
            out.push_str(sequence);
            cursor = *offset;
        }
        out.push_str(&visible[cursor..]);
        out
    }
}

/// Byte offset in escape-free `visible` of the end of the longest
/// grapheme-cluster prefix that fits `max_columns`, plus whether the walk
/// stopped on the budget rather than on running out of scanned text.
///
/// A cluster that would straddle the budget is dropped whole. `start_limit`
/// bounds where a cluster may start, so the last one taken keeps continuation
/// scalars that lie past it.
fn cluster_prefix_end(visible: &str, max_columns: usize, start_limit: usize) -> (usize, bool) {
    let mut columns = 0usize;
    let mut end = 0usize;
    while end < start_limit {
        let Some(cluster) = leading_cluster(&visible[end..]) else {
            return (end, true);
        };
        let next = columns.saturating_add(cluster_display_width(cluster));
        if next > max_columns {
            return (end, true);
        }
        columns = next;
        end += cluster.len();
    }
    (end, false)
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
/// Clusters are taken from the escape-free view of the text rather than the raw
/// bytes, because an SGR sequence written between a base character and its
/// continuation would otherwise split the cluster the terminal composes and get
/// the halves charged the wrong width. A zero-width tail is consumed rather
/// than left behind, so text that occupies exactly the budget does not report
/// itself truncated and collect an ellipsis it never earned.
///
/// The view is built over a window of the source that widens only while the
/// budget stays unfilled, so a long line costs the prefix it prints rather than
/// its length, and free content — SGR sequences and zero-width clusters — is
/// still followed out to the byte cap.
pub(crate) fn terminal_safe_prefix(
    text: &str,
    allow_sgr: bool,
    max_visible: usize,
) -> (String, bool) {
    if max_visible == 0 {
        return (String::new(), !text.is_empty());
    }

    let max_scanned_bytes = max_visible.saturating_mul(64).clamp(64, 64 * 1024);
    let mut window = max_visible.saturating_mul(4).clamp(64, max_scanned_bytes);
    loop {
        let split = SgrSplit::new(text, allow_sgr, true, window);
        let (end, budget_stopped) =
            cluster_prefix_end(split.visible(), max_visible, split.cluster_start_limit());
        let whole_text = end == split.visible().len() && split.is_complete(text);
        if budget_stopped || whole_text || window == max_scanned_bytes {
            return (split.render_prefix(end), !whole_text);
        }
        // The window ran out before the budget did, so everything scanned so
        // far was free: widen and rescan until one of them decides.
        window = window.saturating_mul(4).min(max_scanned_bytes);
    }
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
