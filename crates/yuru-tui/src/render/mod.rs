mod highlight;
mod layout;
mod preview_pane;
mod results;

#[allow(unused_imports)]
pub(crate) use highlight::{highlight_segments_for_result, HighlightSegment};
pub(crate) use layout::{preview_geometry, Viewport};
#[cfg(feature = "image")]
#[allow(unused_imports)]
pub(crate) use preview_pane::render_image_preview;
pub(crate) use results::{render, RenderContext};
