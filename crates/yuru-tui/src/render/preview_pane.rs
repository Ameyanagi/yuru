use std::io::Write;

use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, SetForegroundColor},
};
#[cfg(feature = "image")]
use ratatui::{buffer::Buffer, layout::Rect};
#[cfg(feature = "image")]
use ratatui_image::{protocol::StatefulProtocol, ResizeEncodeRender};

use crate::preview::PreviewRender;

use super::layout::truncate_to_width_with_ellipsis;
use super::results::RenderContext;

const PREVIEW_SEPARATOR_COLOR: Color = Color::DarkGrey;

pub(super) fn render_preview(
    output: &mut impl Write,
    context: &mut RenderContext<'_>,
    preview_width: usize,
    start_row: usize,
) -> Result<()> {
    if context.preview.is_none() {
        return Ok(());
    }
    if preview_width == 0 {
        return Ok(());
    }

    let x = context.viewport.width.saturating_sub(preview_width);
    let max_rows = context.viewport.rows;

    queue!(output, SetForegroundColor(PREVIEW_SEPARATOR_COLOR))?;
    for row in 0..max_rows {
        queue!(
            output,
            MoveTo(x.saturating_sub(1) as u16, (start_row + row) as u16),
            Print("│")
        )?;
    }
    queue!(output, SetForegroundColor(Color::Reset))?;

    match context.preview.as_mut().expect("preview exists") {
        PreviewRender::Text { text, scroll } => {
            for (row, line) in text.lines().skip(*scroll).take(max_rows).enumerate() {
                queue!(
                    output,
                    MoveTo(x as u16, (start_row + row) as u16),
                    Print(truncate_to_width_with_ellipsis(
                        line,
                        preview_width,
                        context.ellipsis
                    ))
                )?;
            }
        }
        #[cfg(feature = "image")]
        PreviewRender::Image { state } => {
            render_image_preview(output, x, start_row, preview_width, max_rows, state)?;
        }
    }

    Ok(())
}

#[cfg(feature = "image")]
pub(crate) fn render_image_preview(
    output: &mut impl Write,
    x: usize,
    y: usize,
    width: usize,
    rows: usize,
    state: &mut StatefulProtocol,
) -> Result<()> {
    let area = Rect::new(0, 0, width as u16, rows as u16);
    let mut buffer = Buffer::empty(area);
    state.render(area, &mut buffer);
    for row in 0..rows {
        for col in 0..width {
            let Some(cell) = buffer.cell((col as u16, row as u16)) else {
                continue;
            };
            if cell.skip || cell.symbol().is_empty() {
                continue;
            }
            queue!(
                output,
                MoveTo((x + col) as u16, (y + row) as u16),
                Print(cell.symbol())
            )?;
        }
    }
    Ok(())
}
