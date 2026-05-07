//! Terminal user interface for Yuru.
//!
//! The TUI provides fzf-like navigation, multi-select, expected keys, a small
//! supported binding subset, and source-span-aware match highlighting.

mod actions;
mod api;
mod keys;
mod preview;
mod render;
mod run;
mod search_worker;
mod state;
mod terminal;

pub use api::{
    BindingAction, CandidateStreamMessage, ImagePreviewProtocol, KeyBinding, PreviewCommand,
    TuiLayout, TuiOptions, TuiOutcome, TuiRgb, TuiStyle,
};
pub use run::{run_interactive, run_interactive_streaming};
pub use state::{TuiAction, TuiState};

#[cfg(test)]
mod tests;
