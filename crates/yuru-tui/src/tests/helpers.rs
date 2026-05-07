use std::thread;
use std::time::{Duration, Instant};

use yuru_core::{KeyKind, ScoredCandidate, SourceSpan};

use crate::preview::{PreviewGeometry, PreviewKey};
use crate::search_worker::{SearchResponse, SearchWorker};

pub(super) fn force_test_color_output() {
    crossterm::style::force_color_output(true);
}

pub(super) fn preview_key(command: &str, id: usize, item: &str) -> PreviewKey {
    PreviewKey::new(
        command.to_string(),
        String::new(),
        id,
        item.to_string(),
        test_geometry(),
    )
}

pub(super) fn test_geometry() -> PreviewGeometry {
    PreviewGeometry {
        columns: 40,
        lines: 10,
        left: 40,
        top: 0,
    }
}

#[cfg(feature = "image")]
pub(super) fn tiny_png_bytes() -> Vec<u8> {
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
    bytes
}

#[cfg(feature = "image")]
pub(super) struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

#[cfg(feature = "image")]
impl EnvGuard {
    pub(super) fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, previous }
    }

    pub(super) fn unset(key: &'static str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::remove_var(key);
        Self { key, previous }
    }
}

#[cfg(feature = "image")]
impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            std::env::set_var(self.key, previous);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

pub(super) fn scored(display: &str, key_kind: KeyKind) -> ScoredCandidate {
    scored_with_id(0, display, key_kind)
}

pub(super) fn scored_with_id(id: usize, display: &str, key_kind: KeyKind) -> ScoredCandidate {
    ScoredCandidate {
        id,
        display: display.to_string(),
        score: 0,
        key_kind,
        key_index: 0,
    }
}

pub(super) fn wait_for_search_response(worker: &mut SearchWorker) -> SearchResponse {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(response) = worker.try_recv() {
            return response;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for search worker"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn japanese_romaji_source_map() -> Vec<Option<SourceSpan>> {
    [
        Some(SourceSpan {
            start_char: 0,
            end_char: 1,
        }),
        Some(SourceSpan {
            start_char: 1,
            end_char: 2,
        }),
        Some(SourceSpan {
            start_char: 2,
            end_char: 3,
        }),
        Some(SourceSpan {
            start_char: 3,
            end_char: 4,
        }),
        Some(SourceSpan {
            start_char: 4,
            end_char: 5,
        }),
        Some(SourceSpan {
            start_char: 5,
            end_char: 6,
        }),
        Some(SourceSpan {
            start_char: 6,
            end_char: 9,
        }),
        Some(SourceSpan {
            start_char: 6,
            end_char: 9,
        }),
        Some(SourceSpan {
            start_char: 6,
            end_char: 9,
        }),
        Some(SourceSpan {
            start_char: 6,
            end_char: 9,
        }),
        Some(SourceSpan {
            start_char: 6,
            end_char: 9,
        }),
        Some(SourceSpan {
            start_char: 6,
            end_char: 9,
        }),
        Some(SourceSpan {
            start_char: 6,
            end_char: 9,
        }),
        Some(SourceSpan {
            start_char: 6,
            end_char: 9,
        }),
        Some(SourceSpan {
            start_char: 9,
            end_char: 10,
        }),
        Some(SourceSpan {
            start_char: 9,
            end_char: 10,
        }),
        Some(SourceSpan {
            start_char: 10,
            end_char: 11,
        }),
        Some(SourceSpan {
            start_char: 11,
            end_char: 12,
        }),
        Some(SourceSpan {
            start_char: 12,
            end_char: 13,
        }),
        Some(SourceSpan {
            start_char: 13,
            end_char: 14,
        }),
    ]
    .to_vec()
}
