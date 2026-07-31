mod highlight;
mod layout;
mod preview_pane;
mod results;

#[cfg(test)]
pub(crate) use highlight::highlight_segments_for_result;
#[allow(unused_imports)]
pub(crate) use highlight::HighlightSegment;
pub(crate) use layout::{preview_geometry, Viewport};
#[cfg(feature = "image")]
#[allow(unused_imports)]
pub(crate) use preview_pane::render_image_preview;
pub(crate) use results::{render, RenderContext};
