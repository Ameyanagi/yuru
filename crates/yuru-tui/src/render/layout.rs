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
    let char_count = text.chars().count();
    if char_count <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }

    let ellipsis_width = ellipsis.chars().count().min(width);
    let mut out: String = text
        .chars()
        .take(width.saturating_sub(ellipsis_width))
        .collect();
    out.extend(ellipsis.chars().take(ellipsis_width));
    out
}
