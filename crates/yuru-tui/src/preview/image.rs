use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::thread;

use crossterm::terminal;
use image::DynamicImage;
use ratatui::layout::Rect;
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
    Resize, ResizeEncodeRender,
};

use crate::api::ImagePreviewProtocol;

use super::PreviewCancellation;

const MAX_IMAGE_FILE_BYTES: usize = 32 * 1024 * 1024;
const MAX_IMAGE_WIDTH: u32 = 8192;
const MAX_IMAGE_HEIGHT: u32 = 8192;
const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_IMAGE_DECODE_BYTES: u64 = 128 * 1024 * 1024;

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
    pub(crate) cancellation: PreviewCancellation,
    pub(crate) handle: Option<thread::JoinHandle<()>>,
}

impl Drop for ImageEncodeWorker {
    fn drop(&mut self) {
        self.cancellation.cancel();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
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
    Cancelled,
}

#[cfg(feature = "image")]
#[cfg(test)]
pub(crate) fn encode_image_preview(
    image: DynamicImage,
    picker: Picker,
    area: (u16, u16),
) -> ImageEncodeResult {
    encode_image_preview_cancellable(image, picker, area, &PreviewCancellation::default())
}

pub(crate) fn encode_image_preview_cancellable(
    image: DynamicImage,
    picker: Picker,
    area: (u16, u16),
    cancellation: &PreviewCancellation,
) -> ImageEncodeResult {
    if cancellation.is_cancelled() {
        return ImageEncodeResult::Cancelled;
    }
    let mut state = picker.new_resize_protocol(image);
    let resize = Resize::Fit(None);
    let available = Rect::new(0, 0, area.0, area.1);
    let encode_area = state.needs_resize(&resize, available).unwrap_or(available);
    state.resize_encode(&resize, encode_area);
    if cancellation.is_cancelled() {
        return ImageEncodeResult::Cancelled;
    }
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
pub(super) fn preview_image_metadata_from_output(bytes: &[u8]) -> Option<String> {
    preview_image_metadata_from_bytes(bytes, None, None).or_else(|| {
        std::str::from_utf8(bytes)
            .ok()
            .and_then(preview_image_metadata_from_path_text)
    })
}

#[cfg(feature = "image")]
pub(super) fn preview_image_from_path_text(text: &str) -> Option<DynamicImage> {
    let path = preview_image_path(text)?;
    preview_image_from_path(&path)
}

#[cfg(feature = "image")]
pub(super) fn preview_image_metadata_from_path_text(text: &str) -> Option<String> {
    let path = preview_image_path(text)?;
    preview_image_metadata_from_path(&path)
}

#[cfg(feature = "image")]
fn preview_image_from_path(path: &Path) -> Option<DynamicImage> {
    let bytes = read_bounded_image(path)?;
    preview_image_from_bytes(&bytes, path.parent())
}

#[cfg(feature = "image")]
pub(super) fn preview_image_metadata_from_path(path: &Path) -> Option<String> {
    let bytes = read_bounded_image(path)?;
    preview_image_metadata_from_bytes(&bytes, path.parent(), Some(path))
}

#[cfg(feature = "image")]
fn preview_image_from_bytes(bytes: &[u8], resources_dir: Option<&Path>) -> Option<DynamicImage> {
    if bytes.len() > MAX_IMAGE_FILE_BYTES {
        return None;
    }
    decode_bounded_raster(bytes).or_else(|| preview_svg_from_bytes(bytes, resources_dir))
}

#[cfg(feature = "image")]
fn preview_image_metadata_from_bytes(
    bytes: &[u8],
    resources_dir: Option<&Path>,
    path: Option<&Path>,
) -> Option<String> {
    if bytes.len() > MAX_IMAGE_FILE_BYTES {
        return None;
    }
    image_raster_metadata(bytes)
        .or_else(|| image_svg_metadata(bytes, resources_dir))
        .map(|(format, width, height)| {
            image_metadata_text(path, &format, width, height, bytes.len())
        })
}

#[cfg(feature = "image")]
fn image_raster_metadata(bytes: &[u8]) -> Option<(String, u32, u32)> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    reader.limits(image_limits());
    let format = reader.format()?;
    let (width, height) = reader.into_dimensions().ok()?;
    dimensions_allowed(width, height).then_some(())?;
    Some((format!("{format:?}").to_ascii_uppercase(), width, height))
}

#[cfg(feature = "image")]
fn image_svg_metadata(bytes: &[u8], resources_dir: Option<&Path>) -> Option<(String, u32, u32)> {
    let tree = parse_svg_tree(bytes, resources_dir)?;
    let size = tree.size();
    let width = size.width().ceil().max(1.0) as u32;
    let height = size.height().ceil().max(1.0) as u32;
    dimensions_allowed(width, height).then_some(())?;
    Some(("SVG".to_string(), width, height))
}

#[cfg(feature = "image")]
fn image_metadata_text(
    path: Option<&Path>,
    format: &str,
    width: u32,
    height: u32,
    bytes: usize,
) -> String {
    let name = path
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "inline image".to_string());
    format!(
        "image: {name}\nformat: {format}\ndimensions: {width} x {height}\nsize: {bytes} bytes\npreview: image rendering disabled"
    )
}

#[cfg(feature = "image")]
fn preview_svg_from_bytes(bytes: &[u8], resources_dir: Option<&Path>) -> Option<DynamicImage> {
    let tree = parse_svg_tree(bytes, resources_dir)?;
    let size = tree.size();
    dimensions_allowed(
        size.width().ceil().max(1.0) as u32,
        size.height().ceil().max(1.0) as u32,
    )
    .then_some(())?;
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
fn parse_svg_tree(bytes: &[u8], resources_dir: Option<&Path>) -> Option<resvg::usvg::Tree> {
    if bytes.len() > MAX_IMAGE_FILE_BYTES {
        return None;
    }
    let mut options = resvg::usvg::Options {
        resources_dir: resources_dir.map(Path::to_path_buf),
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..resvg::usvg::Options::default()
    };
    options.fontdb_mut().load_system_fonts();

    resvg::usvg::Tree::from_data(bytes, &options).ok()
}

#[cfg(feature = "image")]
fn read_bounded_image(path: &Path) -> Option<Vec<u8>> {
    let metadata = path.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_FILE_BYTES as u64 {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len().min(MAX_IMAGE_FILE_BYTES as u64) as usize);
    File::open(path)
        .ok()?
        .take((MAX_IMAGE_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= MAX_IMAGE_FILE_BYTES).then_some(bytes)
}

#[cfg(feature = "image")]
fn decode_bounded_raster(bytes: &[u8]) -> Option<DynamicImage> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    reader.limits(image_limits());
    let (width, height) = reader.into_dimensions().ok()?;
    dimensions_allowed(width, height).then_some(())?;

    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    reader.limits(image_limits());
    reader.decode().ok()
}

#[cfg(feature = "image")]
fn image_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_WIDTH);
    limits.max_image_height = Some(MAX_IMAGE_HEIGHT);
    limits.max_alloc = Some(MAX_IMAGE_DECODE_BYTES);
    limits
}

#[cfg(feature = "image")]
fn dimensions_allowed(width: u32, height: u32) -> bool {
    width <= MAX_IMAGE_WIDTH
        && height <= MAX_IMAGE_HEIGHT
        && u64::from(width).saturating_mul(u64::from(height)) <= MAX_IMAGE_PIXELS
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
pub(super) fn image_picker_from_env(protocol: ImagePreviewProtocol) -> Picker {
    let mut picker = image_picker_from_terminal_size().unwrap_or_else(Picker::halfblocks);
    if let Some(protocol) = image_protocol_type(protocol).or_else(image_protocol_from_env) {
        picker.set_protocol_type(protocol);
    }
    picker
}

#[cfg(feature = "image")]
fn image_protocol_type(protocol: ImagePreviewProtocol) -> Option<ProtocolType> {
    match protocol {
        ImagePreviewProtocol::Auto => None,
        ImagePreviewProtocol::Halfblocks => Some(ProtocolType::Halfblocks),
        ImagePreviewProtocol::Sixel => Some(ProtocolType::Sixel),
        ImagePreviewProtocol::Kitty => Some(ProtocolType::Kitty),
        ImagePreviewProtocol::Iterm2 => Some(ProtocolType::Iterm2),
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
