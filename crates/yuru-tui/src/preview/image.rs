use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use crossterm::terminal;
use image::DynamicImage;
use ratatui::layout::Rect;
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
    Resize, ResizeEncodeRender,
};

use crate::api::ImagePreviewProtocol;

#[cfg(feature = "image")]
pub(crate) struct ImagePreview {
    pub(crate) image: DynamicImage,
    pub(crate) state: Option<Box<StatefulProtocol>>,
    pub(crate) worker: Option<ImageEncodeWorker>,
    pub(crate) area: Option<(u16, u16)>,
    pub(crate) error: Option<String>,
}

#[cfg(feature = "image")]
pub(crate) struct ImageEncodeWorker {
    pub(crate) area: (u16, u16),
    pub(crate) receiver: Receiver<ImageEncodeResult>,
}

#[cfg(feature = "image")]
pub(crate) enum ImageEncodeResult {
    Ready {
        area: (u16, u16),
        state: Box<StatefulProtocol>,
    },
    Error {
        area: (u16, u16),
        message: String,
    },
}

#[cfg(feature = "image")]
pub(crate) fn encode_image_preview(
    image: DynamicImage,
    picker: Picker,
    area: (u16, u16),
) -> ImageEncodeResult {
    let mut state = picker.new_resize_protocol(image);
    let resize = Resize::Fit(None);
    let available = Rect::new(0, 0, area.0, area.1);
    let encode_area = state.needs_resize(&resize, available).unwrap_or(available);
    state.resize_encode(&resize, encode_area);
    match state.last_encoding_result() {
        Some(Err(error)) => ImageEncodeResult::Error {
            area,
            message: format!("image preview failed: {error}"),
        },
        _ => ImageEncodeResult::Ready {
            area,
            state: Box::new(state),
        },
    }
}

#[cfg(feature = "image")]
pub(crate) fn preview_image_from_output(bytes: &[u8]) -> Option<DynamicImage> {
    preview_image_from_bytes(bytes, None).or_else(|| {
        std::str::from_utf8(bytes)
            .ok()
            .and_then(preview_image_from_path_text)
    })
}

#[cfg(feature = "image")]
pub(super) fn preview_image_from_path_text(text: &str) -> Option<DynamicImage> {
    let path = preview_image_path(text)?;
    preview_image_from_path(&path)
}

#[cfg(feature = "image")]
fn preview_image_from_path(path: &Path) -> Option<DynamicImage> {
    let bytes = std::fs::read(path).ok()?;
    preview_image_from_bytes(&bytes, path.parent())
}

#[cfg(feature = "image")]
fn preview_image_from_bytes(bytes: &[u8], resources_dir: Option<&Path>) -> Option<DynamicImage> {
    image::load_from_memory(bytes)
        .ok()
        .or_else(|| preview_svg_from_bytes(bytes, resources_dir))
}

#[cfg(feature = "image")]
fn preview_svg_from_bytes(bytes: &[u8], resources_dir: Option<&Path>) -> Option<DynamicImage> {
    let mut options = resvg::usvg::Options {
        resources_dir: resources_dir.map(Path::to_path_buf),
        ..resvg::usvg::Options::default()
    };
    #[cfg(feature = "image")]
    options.fontdb_mut().load_system_fonts();

    let tree = resvg::usvg::Tree::from_data(bytes, &options).ok()?;
    let size = tree.size();
    let scale = (2048.0 / size.width()).min(2048.0 / size.height()).min(1.0);
    let width = (size.width() * scale).ceil().clamp(1.0, 2048.0) as u32;
    let height = (size.height() * scale).ceil().clamp(1.0, 2048.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    let mut pixmap_mut = pixmap.as_mut();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap_mut,
    );
    image::RgbaImage::from_raw(width, height, pixmap.data().to_vec()).map(DynamicImage::ImageRgba8)
}

#[cfg(feature = "image")]
fn preview_image_path(text: &str) -> Option<PathBuf> {
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(path) = preview_image_path_candidate(line) {
            return Some(path);
        }
        if let Some(left) = preview_file_command_path(line) {
            if let Some(path) = preview_image_path_candidate(left) {
                return Some(path);
            }
        }
        if let Some((_, right)) = line.rsplit_once('|') {
            if let Some(path) = preview_image_path_candidate(right) {
                return Some(path);
            }
            if let Some(left) = preview_file_command_path(right) {
                if let Some(path) = preview_image_path_candidate(left) {
                    return Some(path);
                }
            }
        }
    }
    None
}

#[cfg(feature = "image")]
pub(crate) fn preview_file_command_path(line: &str) -> Option<&str> {
    line.rsplit_once(": ")
        .or_else(|| line.rsplit_once(':'))
        .map(|(left, _)| left)
}

#[cfg(feature = "image")]
fn preview_image_path_candidate(raw: &str) -> Option<PathBuf> {
    let raw = raw.trim_matches(['"', '\'']);
    let path = Path::new(raw);
    path.is_file()
        .then(|| path.to_path_buf())
        .filter(|path| is_image_path(path))
}

#[cfg(feature = "image")]
fn is_image_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some(
            "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "bmp"
                | "ico"
                | "tif"
                | "tiff"
                | "webp"
                | "svg"
                | "svgz"
        )
    )
}

#[cfg(feature = "image")]
pub(super) fn image_picker_from_env(protocol: Option<ImagePreviewProtocol>) -> Picker {
    let mut picker = image_picker_from_terminal_size().unwrap_or_else(Picker::halfblocks);
    if let Some(protocol) = protocol
        .map(image_protocol_type)
        .or_else(image_protocol_from_env)
    {
        picker.set_protocol_type(protocol);
    }
    picker
}

#[cfg(feature = "image")]
fn image_protocol_type(protocol: ImagePreviewProtocol) -> ProtocolType {
    match protocol {
        ImagePreviewProtocol::Halfblocks => ProtocolType::Halfblocks,
        ImagePreviewProtocol::Sixel => ProtocolType::Sixel,
        ImagePreviewProtocol::Kitty => ProtocolType::Kitty,
        ImagePreviewProtocol::Iterm2 => ProtocolType::Iterm2,
    }
}

#[cfg(feature = "image")]
fn image_picker_from_terminal_size() -> Option<Picker> {
    let size = terminal::window_size().ok()?;
    if size.columns == 0 || size.rows == 0 || size.width == 0 || size.height == 0 {
        return None;
    }

    let cell_width = (size.width / size.columns).max(1);
    let cell_height = (size.height / size.rows).max(1);
    #[allow(deprecated)]
    Some(Picker::from_fontsize((cell_width, cell_height)))
}

#[cfg(feature = "image")]
pub(crate) fn image_protocol_from_env() -> Option<ProtocolType> {
    if let Ok(protocol) = std::env::var("YURU_PREVIEW_IMAGE_PROTOCOL") {
        return match protocol.to_ascii_lowercase().as_str() {
            "halfblocks" | "halfblock" | "unicode" => Some(ProtocolType::Halfblocks),
            "sixel" => Some(ProtocolType::Sixel),
            "kitty" => Some(ProtocolType::Kitty),
            "iterm2" | "iterm" => Some(ProtocolType::Iterm2),
            _ => None,
        };
    }
    if std::env::var("KITTY_WINDOW_ID").is_ok_and(|value| !value.is_empty())
        || std::env::var("TERM_PROGRAM").is_ok_and(|value| value.eq_ignore_ascii_case("ghostty"))
        || std::env::var("GHOSTTY_RESOURCES_DIR").is_ok_and(|value| !value.is_empty())
        || std::env::var("GHOSTTY_BIN_DIR").is_ok_and(|value| !value.is_empty())
    {
        return Some(ProtocolType::Kitty);
    }
    if std::env::var("TERM_PROGRAM").is_ok_and(|value| {
        value.contains("iTerm") || value.contains("WezTerm") || value.contains("rio")
    }) {
        return Some(ProtocolType::Iterm2);
    }
    if std::env::var("TERM").is_ok_and(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("sixel") || value.contains("mlterm")
    }) {
        return Some(ProtocolType::Sixel);
    }
    None
}
