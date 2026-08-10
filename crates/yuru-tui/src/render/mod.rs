mod highlight;
mod layout;
mod preview_pane;
mod results;

#[allow(unused_imports)]
pub(crate) use highlight::HighlightSegment;
#[cfg(test)]
pub(crate) use highlight::{
    highlight_segments_for_result, highlight_segments_for_result_with_ansi,
};
#[cfg(test)]
pub(crate) use layout::{display_width, terminal_safe_prefix, truncate_to_width_with_ellipsis};
pub(crate) use layout::{preview_geometry, Viewport};
#[cfg(feature = "image")]
#[allow(unused_imports)]
pub(crate) use preview_pane::render_image_preview;
pub(crate) use results::{render, RenderContext};
