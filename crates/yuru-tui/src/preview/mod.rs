mod builtin;
mod cache;
mod command;
#[cfg(feature = "image")]
mod image;
mod process;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Clone, Default)]
pub(crate) struct PreviewCancellation(Arc<AtomicBool>);

impl PreviewCancellation {
    pub(super) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[allow(unused_imports)]
pub(crate) use cache::{
    PreviewCache, PreviewContent, PreviewGeometry, PreviewKey, PreviewPayload, PreviewRender,
    PREVIEW_WORKER_POLL,
};
#[allow(unused_imports)]
#[cfg(test)]
pub(crate) use command::run_preview_command;
#[cfg(all(feature = "image", test))]
pub(crate) use image::encode_image_preview;
#[cfg(feature = "image")]
#[allow(unused_imports)]
pub(crate) use image::{
    image_protocol_from_env, preview_file_command_path, preview_image_from_output,
    ImageEncodeResult, ImageEncodeWorker,
};
