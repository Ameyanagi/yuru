use std::io::Write;

use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    queue,
    style::{
        Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
    },
    terminal::{Clear, ClearType},
};
use yuru_core::{Candidate, ScoredCandidate};

use crate::api::{TuiLayout, TuiStyle};
use crate::preview::PreviewRender;
use crate::state::TuiState;

use super::highlight::highlight_segments_for_result;
use super::layout::{
    footer_start_row, preview_width, scroll_offset, truncate_to_width_with_ellipsis,
    visible_line_count, Viewport,
};
use super::preview_pane::render_preview;

pub(crate) fn render(
    output: &mut impl Write,
    state: &TuiState,
    results: &[ScoredCandidate],
    mut context: RenderContext<'_>,
) -> Result<()> {
    queue!(output, MoveTo(0, 0), Clear(ClearType::All))?;
    let prompt_row = if context.no_input {
        None
    } else if context.layout.prompt_at_bottom() {
        Some(context.viewport.rows)
    } else {
        Some(0)
    };
    if let Some(prompt_row) = prompt_row {
        let input = if state.query().is_empty() {
            format!("{}{}", context.prompt, "")
        } else {
            format!("{}{}", context.prompt, state.query())
        };
        queue!(
            output,
            MoveTo(0, prompt_row as u16),
            Print(truncate_to_width_with_ellipsis(
                &input,
                context.viewport.width,
                context.ellipsis
            ))
        )?;
    }

    let header_rows = visible_line_count(context.header);
    if let Some(header) = context.header {
        let header_start = if context.layout.prompt_at_bottom() {
            0
        } else {
            1
        };
        for (row, line) in header.lines().enumerate() {
            queue!(
                output,
                MoveTo(0, (header_start + row) as u16),
                Print(truncate_to_width_with_ellipsis(
                    line,
                    context.viewport.width,
                    context.ellipsis
                ))
            )?;
        }
    }
    let footer_rows = visible_line_count(context.footer);
    if let Some(footer) = context.footer {
        let footer_start = footer_start_row(context.layout, context.viewport.rows, footer_rows);
        for (row, line) in footer.lines().enumerate() {
            queue!(
                output,
                MoveTo(0, (footer_start + row) as u16),
                Print(truncate_to_width_with_ellipsis(
                    line,
                    context.viewport.width,
                    context.ellipsis
                ))
            )?;
        }
    }

    let preview_width = preview_width(context.viewport.width, context.preview.as_ref());
    let list_width = if preview_width == 0 {
        context.viewport.width
    } else {
        context
            .viewport
            .width
            .saturating_sub(preview_width)
            .saturating_sub(1)
            .max(1)
    };
    let result_rows = context
        .viewport
        .rows
        .saturating_sub(header_rows + footer_rows)
        .max(1);
    let result_bottom = context.viewport.rows.saturating_sub(footer_rows + 1);
    let offset = scroll_offset(state.selected(), results.len(), result_rows);
    for (row, result) in results.iter().skip(offset).take(result_rows).enumerate() {
        let result_row = if context.layout.list_bottom_up() {
            result_bottom.saturating_sub(row)
        } else if context.layout.prompt_at_bottom() {
            row + header_rows
        } else {
            row + 1 + header_rows
        };
        queue!(output, MoveTo(0, result_row as u16))?;
        let mark = if context.multi && state.marked().contains(&result.id) {
            context.marker
        } else {
            " "
        };
        let selected = offset + row == state.selected();
        let pointer_width = context.pointer.chars().count() + context.marker.chars().count();
        let result_width = list_width.saturating_sub(pointer_width);
        let selected_row_background = selected && context.highlight_line;
        if selected_row_background {
            queue!(
                output,
                SetBackgroundColor(context.style.selected_bg_color())
            )?;
            if let Some(color) = context.style.selected_fg_color() {
                queue!(output, SetForegroundColor(color))?;
            }
        }
        if selected {
            if let Some(color) = context.style.pointer_color() {
                queue!(output, SetForegroundColor(color))?;
            }
            queue!(output, Print(context.pointer), Print(mark))?;
            if selected_row_background {
                if let Some(color) = context.style.selected_fg_color() {
                    queue!(output, SetForegroundColor(color))?;
                } else {
                    queue!(output, SetForegroundColor(Color::Reset))?;
                }
            } else {
                queue!(output, SetForegroundColor(Color::Reset))?;
            }
        } else {
            queue!(output, Print(" "), Print(mark))?;
        }
        let printed = render_highlighted_result(
            output,
            state.query(),
            result,
            &context,
            result_width,
            selected,
        )?;
        if selected_row_background {
            let padding = result_width.saturating_sub(printed);
            if padding > 0 {
                queue!(output, Print(" ".repeat(padding)))?;
            }
            queue!(output, ResetColor)?;
        }
        queue!(output, SetAttribute(Attribute::Reset))?;
    }

    render_preview(output, &mut context, preview_width)?;

    if let Some(prompt_row) = prompt_row {
        let cursor_column =
            context.prompt.chars().count() + state.query()[..state.cursor()].chars().count();
        queue!(
            output,
            MoveTo(
                cursor_column.min(context.viewport.width - 1) as u16,
                prompt_row as u16
            )
        )?;
    }
    output.flush()?;
    Ok(())
}

pub(crate) struct RenderContext<'a> {
    pub(crate) candidates: &'a [Candidate],
    pub(crate) prompt: &'a str,
    pub(crate) header: Option<&'a str>,
    pub(crate) footer: Option<&'a str>,
    pub(crate) viewport: Viewport,
    pub(crate) layout: TuiLayout,
    pub(crate) preview: Option<PreviewRender<'a>>,
    pub(crate) style: &'a TuiStyle,
    pub(crate) highlight_line: bool,
    pub(crate) case_sensitive: bool,
    pub(crate) multi: bool,
    pub(crate) no_input: bool,
    pub(crate) pointer: &'a str,
    pub(crate) marker: &'a str,
    pub(crate) ellipsis: &'a str,
}

fn render_highlighted_result(
    output: &mut impl Write,
    query: &str,
    result: &ScoredCandidate,
    context: &RenderContext<'_>,
    width: usize,
    selected: bool,
) -> Result<usize> {
    let mut printed = 0;
    for segment in highlight_segments_for_result(
        query,
        result,
        context.candidates,
        context.case_sensitive,
        width,
    ) {
        if segment.highlighted {
            queue!(
                output,
                SetForegroundColor(context.style.highlight_color(selected)),
                SetAttribute(Attribute::Bold)
            )?;
        }
        printed += segment.text.chars().count();
        queue!(output, Print(segment.text))?;
        if segment.highlighted {
            if selected && context.highlight_line {
                if let Some(color) = context.style.selected_fg_color() {
                    queue!(output, SetForegroundColor(color))?;
                } else {
                    queue!(output, SetForegroundColor(Color::Reset))?;
                }
            } else {
                queue!(output, SetForegroundColor(Color::Reset))?;
            }
            queue!(output, SetAttribute(Attribute::NormalIntensity))?;
        }
    }
    Ok(printed)
}
