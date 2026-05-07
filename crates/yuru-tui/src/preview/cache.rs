use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(feature = "image")]
use image::DynamicImage;
#[cfg(feature = "image")]
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use yuru_core::ScoredCandidate;

use crate::api::{ImagePreviewProtocol, PreviewCommand};
use crate::state::TuiState;

use super::command::run_preview_command;
#[cfg(feature = "image")]
use super::image::{
    encode_image_preview, image_picker_from_env, ImageEncodeResult, ImageEncodeWorker, ImagePreview,
};

const PREVIEW_DEBOUNCE: Duration = Duration::from_millis(50);
pub(crate) const PREVIEW_WORKER_POLL: Duration = Duration::from_millis(25);
const PREVIEW_LOADING: &str = "loading preview...";
#[cfg(feature = "image")]
const IMAGE_PREVIEW_LOADING: &str = "loading image preview...";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PreviewGeometry {
    pub(crate) columns: usize,
    pub(crate) lines: usize,
    pub(crate) left: usize,
    pub(crate) top: usize,
}

#[derive(Default)]
pub(crate) struct PreviewCache {
    key: Option<PreviewKey>,
    pub(crate) content: Option<PreviewContent>,
    pending: Option<PreviewRequest>,
    worker: Option<PreviewWorker>,
    pub(crate) scroll: usize,
    #[cfg(feature = "image")]
    image_picker: Option<Picker>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreviewKey {
    command: String,
    shell: String,
    selected_id: usize,
    display: String,
    geometry: PreviewGeometry,
}

impl PreviewKey {
    pub(crate) fn new(
        command: String,
        shell: String,
        selected_id: usize,
        display: String,
        geometry: PreviewGeometry,
    ) -> Self {
        Self {
            command,
            shell,
            selected_id,
            display,
            geometry,
        }
    }
}

struct PreviewRequest {
    key: PreviewKey,
    command: PreviewCommand,
    shell: Option<String>,
    item: String,
    geometry: PreviewGeometry,
    requested_at: Instant,
}

struct PreviewWorker {
    key: PreviewKey,
    receiver: Receiver<(PreviewKey, PreviewPayload)>,
}

pub(crate) enum PreviewContent {
    Text(String),
    #[cfg(feature = "image")]
    Image(ImagePreview),
}

pub(crate) enum PreviewPayload {
    Text(String),
    #[cfg(feature = "image")]
    Image(DynamicImage),
}

impl PreviewCache {
    pub(crate) fn request_for_selection(
        &mut self,
        command: Option<&PreviewCommand>,
        shell: Option<&str>,
        results: &[ScoredCandidate],
        state: &TuiState,
        geometry: Option<PreviewGeometry>,
    ) {
        let Some(command) = command else {
            self.clear();
            return;
        };
        let Some(selected) = results.get(state.selected()) else {
            self.clear();
            return;
        };
        let Some(geometry) = geometry else {
            self.clear();
            return;
        };
        let key = PreviewKey::new(
            command.cache_key(),
            shell.unwrap_or_default().to_string(),
            selected.id,
            selected.display.clone(),
            geometry,
        );

        if self.key.as_ref() != Some(&key) {
            if self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.key == key)
                || self.worker.as_ref().is_some_and(|worker| worker.key == key)
            {
                return;
            }

            self.key = None;
            self.content = None;
            self.scroll = 0;
            self.worker = None;
            self.pending = Some(PreviewRequest {
                key,
                command: command.clone(),
                shell: shell.map(str::to_string),
                item: selected.display.clone(),
                geometry,
                requested_at: Instant::now(),
            });
        }
    }

    pub(crate) fn poll(&mut self) -> bool {
        self.start_ready_worker();
        let changed = self.receive_worker_result();
        #[cfg(feature = "image")]
        {
            changed | self.receive_image_result()
        }
        #[cfg(not(feature = "image"))]
        {
            changed
        }
    }

    fn start_ready_worker(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let Some(pending) = &self.pending else {
            return;
        };
        if pending.requested_at.elapsed() < PREVIEW_DEBOUNCE {
            return;
        }

        let pending = self.pending.take().expect("pending preview exists");
        let (sender, receiver) = mpsc::channel();
        let key = pending.key.clone();
        let worker_key = pending.key.clone();
        thread::spawn(move || {
            let payload = run_preview_command(
                &pending.command,
                pending.shell.as_deref(),
                &pending.item,
                pending.geometry,
            );
            let _ = sender.send((key, payload));
        });
        self.worker = Some(PreviewWorker {
            key: worker_key,
            receiver,
        });
    }

    fn receive_worker_result(&mut self) -> bool {
        let Some(worker) = &self.worker else {
            return false;
        };
        match worker.receiver.try_recv() {
            Ok((key, payload)) => {
                self.worker = None;
                self.replace(key, payload);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.worker = None;
                false
            }
        }
    }

    pub(crate) fn next_poll_timeout(&self) -> Option<Duration> {
        if self.worker.is_some() {
            return Some(PREVIEW_WORKER_POLL);
        }
        #[cfg(feature = "image")]
        if matches!(
            &self.content,
            Some(PreviewContent::Image(ImagePreview {
                worker: Some(_),
                ..
            }))
        ) {
            return Some(PREVIEW_WORKER_POLL);
        }
        let pending = self.pending.as_ref()?;
        Some(PREVIEW_DEBOUNCE.saturating_sub(pending.requested_at.elapsed()))
    }

    pub(crate) fn replace(&mut self, key: PreviewKey, payload: PreviewPayload) {
        self.key = Some(key);
        self.content = Some(match payload {
            PreviewPayload::Text(text) => PreviewContent::Text(text),
            #[cfg(feature = "image")]
            PreviewPayload::Image(image) => PreviewContent::Image(ImagePreview {
                image,
                state: None,
                worker: None,
                area: None,
                error: None,
            }),
        });
        self.scroll = 0;
    }

    fn clear(&mut self) {
        self.key = None;
        self.content = None;
        self.pending = None;
        self.worker = None;
        self.scroll = 0;
    }

    pub(crate) fn render(&mut self) -> Option<PreviewRender<'_>> {
        let Some(content) = self.content.as_mut() else {
            if self.pending.is_some() || self.worker.is_some() {
                return Some(PreviewRender::Text {
                    text: PREVIEW_LOADING,
                    scroll: 0,
                });
            }
            return None;
        };
        match content {
            PreviewContent::Text(text) => Some(PreviewRender::Text {
                text,
                scroll: self.scroll,
            }),
            #[cfg(feature = "image")]
            PreviewContent::Image(image) => {
                if let Some(state) = image.state.as_mut() {
                    Some(PreviewRender::Image {
                        state: state.as_mut(),
                    })
                } else if let Some(error) = image.error.as_deref() {
                    Some(PreviewRender::Text {
                        text: error,
                        scroll: 0,
                    })
                } else {
                    Some(PreviewRender::Text {
                        text: IMAGE_PREVIEW_LOADING,
                        scroll: 0,
                    })
                }
            }
        }
    }

    pub(crate) fn scroll_up(&mut self, rows: usize, visible_rows: usize) {
        self.scroll = self.scroll.saturating_sub(rows);
        self.clamp_scroll(visible_rows);
    }

    pub(crate) fn scroll_down(&mut self, rows: usize, visible_rows: usize) {
        self.scroll = self.scroll.saturating_add(rows);
        self.clamp_scroll(visible_rows);
    }

    pub(crate) fn scroll_top(&mut self) {
        self.scroll = 0;
    }

    pub(crate) fn scroll_bottom(&mut self, visible_rows: usize) {
        self.scroll = self.max_scroll(visible_rows);
    }

    pub(crate) fn clamp_scroll(&mut self, visible_rows: usize) {
        self.scroll = self.scroll.min(self.max_scroll(visible_rows));
    }

    fn max_scroll(&self, visible_rows: usize) -> usize {
        self.line_count().saturating_sub(visible_rows.max(1))
    }

    fn line_count(&self) -> usize {
        match &self.content {
            Some(PreviewContent::Text(text)) => text.lines().count(),
            #[cfg(feature = "image")]
            Some(PreviewContent::Image(image)) => {
                usize::from(image.error.is_some() || image.state.is_none())
            }
            None => 0,
        }
    }

    #[cfg(feature = "image")]
    fn image_picker(&mut self, protocol: Option<ImagePreviewProtocol>) -> &Picker {
        self.image_picker
            .get_or_insert_with(|| image_picker_from_env(protocol))
    }

    #[cfg(feature = "image")]
    pub(crate) fn prepare_image(
        &mut self,
        protocol: Option<ImagePreviewProtocol>,
        width: usize,
        rows: usize,
    ) -> bool {
        let mut changed = self.receive_image_result();
        let area = (width as u16, rows as u16);
        if area.0 == 0 || area.1 == 0 {
            return changed;
        }

        let picker = self.image_picker(protocol).clone();
        let Some(PreviewContent::Image(image)) = self.content.as_mut() else {
            return changed;
        };
        if image.error.is_some() {
            return changed;
        }
        if image.state.is_some() && image.area == Some(area) {
            return changed;
        }
        if image
            .worker
            .as_ref()
            .is_some_and(|worker| worker.area == area)
        {
            return changed;
        }

        let source = image.image.clone();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = encode_image_preview(source, picker, area);
            let _ = sender.send(result);
        });
        image.worker = Some(ImageEncodeWorker { area, receiver });
        image.state = None;
        image.area = None;
        image.error = None;
        changed = true;
        changed
    }

    #[cfg(not(feature = "image"))]
    pub(crate) fn prepare_image(
        &mut self,
        _protocol: Option<ImagePreviewProtocol>,
        _width: usize,
        _rows: usize,
    ) -> bool {
        false
    }

    #[cfg(feature = "image")]
    fn receive_image_result(&mut self) -> bool {
        let Some(PreviewContent::Image(image)) = self.content.as_mut() else {
            return false;
        };
        let Some(worker) = &image.worker else {
            return false;
        };
        match worker.receiver.try_recv() {
            Ok(ImageEncodeResult::Ready { area, state }) => {
                if image
                    .worker
                    .as_ref()
                    .is_some_and(|worker| worker.area == area)
                {
                    image.worker = None;
                    image.state = Some(state);
                    image.area = Some(area);
                    image.error = None;
                    return true;
                }
                false
            }
            Ok(ImageEncodeResult::Error { area, message }) => {
                if image
                    .worker
                    .as_ref()
                    .is_some_and(|worker| worker.area == area)
                {
                    image.worker = None;
                    image.state = None;
                    image.area = None;
                    image.error = Some(message);
                    return true;
                }
                false
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                image.worker = None;
                image.error = Some("image preview worker stopped".to_string());
                true
            }
        }
    }
}

pub(crate) enum PreviewRender<'a> {
    Text {
        text: &'a str,
        scroll: usize,
    },
    #[cfg(feature = "image")]
    Image {
        state: &'a mut StatefulProtocol,
    },
}
