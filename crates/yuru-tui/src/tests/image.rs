use std::sync::mpsc;

use ratatui_image::picker::{Picker, ProtocolType};

#[cfg(unix)]
use crate::api::PreviewCommand;
#[cfg(unix)]
use crate::preview::run_preview_command;
use crate::preview::{
    encode_image_preview, image_protocol_from_env, preview_file_command_path,
    preview_image_from_output, ImageEncodeResult, ImageEncodeWorker, PreviewCache, PreviewContent,
    PreviewPayload, PREVIEW_WORKER_POLL,
};
use crate::render::render_image_preview;

#[cfg(unix)]
use super::helpers::test_geometry;
use super::helpers::{preview_key, tiny_png_bytes, EnvGuard};

#[cfg(feature = "image")]
#[test]
fn preview_output_detects_inline_image_bytes() {
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        1,
        1,
        image::Rgba([255, 0, 0, 255]),
    ));
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();

    assert!(preview_image_from_output(&bytes).is_some());
}

#[cfg(feature = "image")]
#[test]
fn preview_output_detects_inline_svg_bytes() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="8"><rect width="10" height="8" fill="#ff0000"/></svg>"##;
    let image = preview_image_from_output(svg).expect("svg should rasterize");

    assert_eq!(image.width(), 10);
    assert_eq!(image.height(), 8);
}

#[cfg(all(feature = "image", unix))]
#[test]
fn preview_command_prefers_selected_image_path_over_text_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ame.png");
    std::fs::write(&path, tiny_png_bytes()).unwrap();

    let preview = run_preview_command(
        &PreviewCommand::Shell("printf 'text preview'".to_string()),
        None,
        path.to_str().unwrap(),
        test_geometry(),
    );

    assert!(matches!(preview, PreviewPayload::Image(_)));
}

#[cfg(feature = "image")]
#[test]
fn preview_output_detects_file_command_image_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ame image.png");
    std::fs::write(&path, tiny_png_bytes()).unwrap();
    let output = format!("{}: PNG image data, 1 x 1", path.display());

    assert!(preview_image_from_output(output.as_bytes()).is_some());
}

#[cfg(feature = "image")]
#[test]
fn preview_file_command_path_keeps_windows_drive_prefix() {
    assert_eq!(
        preview_file_command_path(r"D:\a\yuru\ame image.png: PNG image data, 1 x 1"),
        Some(r"D:\a\yuru\ame image.png")
    );
}

#[cfg(feature = "image")]
#[test]
fn image_preview_encoding_happens_before_render() {
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        2,
        image::Rgba([255, 0, 0, 255]),
    ));
    let picker = Picker::halfblocks();
    let result = encode_image_preview(image, picker, (4, 2));

    assert!(matches!(result, ImageEncodeResult::Ready { .. }));
}

#[cfg(feature = "image")]
#[test]
fn encoded_image_preview_renders_terminal_output() {
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        2,
        image::Rgba([255, 0, 0, 255]),
    ));
    let picker = Picker::halfblocks();
    let ImageEncodeResult::Ready { mut state, .. } = encode_image_preview(image, picker, (4, 2))
    else {
        panic!("image preview should encode");
    };
    let mut output = Vec::new();

    render_image_preview(&mut output, 10, 0, 4, 2, state.as_mut()).unwrap();

    assert!(!output.is_empty());
}

#[cfg(feature = "image")]
#[test]
fn image_encode_worker_keeps_preview_polling() {
    let mut cache = PreviewCache::default();
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        2,
        image::Rgba([255, 0, 0, 255]),
    ));
    cache.replace(
        preview_key("file {}", 0, "ame.png"),
        PreviewPayload::Image(image),
    );
    let (_sender, receiver) = mpsc::channel();
    if let Some(PreviewContent::Image(image)) = cache.content.as_mut() {
        image.worker = Some(ImageEncodeWorker {
            area: (4, 2),
            receiver,
        });
    }

    assert_eq!(cache.next_poll_timeout(), Some(PREVIEW_WORKER_POLL));
}

#[cfg(feature = "image")]
#[test]
fn ghostty_tmux_env_prefers_kitty_protocol() {
    let _guard = EnvGuard::set("GHOSTTY_RESOURCES_DIR", "/Applications/Ghostty.app");
    let _term_program = EnvGuard::set("TERM_PROGRAM", "tmux");
    let _protocol = EnvGuard::unset("YURU_PREVIEW_IMAGE_PROTOCOL");

    assert_eq!(image_protocol_from_env(), Some(ProtocolType::Kitty));
}
