use crossterm::style::Color;
use yuru_core::Candidate;

const DEFAULT_SELECTED_ROW_BG: Color = Color::Rgb {
    r: 52,
    g: 58,
    b: 70,
};

#[derive(Clone, Debug)]
/// Configuration for an interactive TUI session.
pub struct TuiOptions {
    /// Query shown when the interface opens.
    pub initial_query: String,
    /// Prompt text displayed before the query.
    pub prompt: String,
    /// Optional header line.
    pub header: Option<String>,
    /// Optional footer line.
    pub footer: Option<String>,
    /// Accepted keys that return through the outcome.
    pub expect_keys: Vec<String>,
    /// Custom key bindings.
    pub bindings: Vec<KeyBinding>,
    /// Optional maximum interface height in terminal rows.
    pub height: Option<usize>,
    /// Vertical layout mode.
    pub layout: TuiLayout,
    /// Optional preview command.
    pub preview: Option<PreviewCommand>,
    /// Optional shell used for preview commands.
    pub preview_shell: Option<String>,
    /// Optional image preview protocol.
    pub preview_image_protocol: Option<ImagePreviewProtocol>,
    /// Display colors for selected UI elements.
    pub style: TuiStyle,
    /// Whether the selected row uses a full-width background.
    pub highlight_line: bool,
    /// Whether selection wraps at list boundaries.
    pub cycle: bool,
    /// Whether multiple candidates can be marked.
    pub multi: bool,
    /// Optional cap for marked candidates.
    pub multi_limit: Option<usize>,
    /// Whether text input is disabled.
    pub no_input: bool,
    /// Marker shown next to the selected row.
    pub pointer: String,
    /// Marker shown next to marked rows.
    pub marker: String,
    /// Text used when display values are truncated.
    pub ellipsis: String,
}

impl Default for TuiOptions {
    fn default() -> Self {
        Self {
            initial_query: String::new(),
            prompt: "> ".to_string(),
            header: None,
            footer: None,
            expect_keys: Vec::new(),
            bindings: Vec::new(),
            height: None,
            layout: TuiLayout::default(),
            preview: None,
            preview_shell: None,
            preview_image_protocol: None,
            style: TuiStyle::default(),
            highlight_line: true,
            cycle: false,
            multi: false,
            multi_limit: None,
            no_input: false,
            pointer: ">".to_string(),
            marker: "*".to_string(),
            ellipsis: "..".to_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Preview source for the selected candidate.
pub enum PreviewCommand {
    /// Run a shell command to produce preview text.
    Shell(String),
    /// Use the built-in file previewer.
    Builtin {
        /// File extensions treated as text by the built-in previewer.
        text_extensions: Vec<String>,
    },
}

impl PreviewCommand {
    pub(crate) fn cache_key(&self) -> String {
        match self {
            Self::Shell(command) => format!("shell:{command}"),
            Self::Builtin { text_extensions } => {
                format!("builtin:{}", text_extensions.join(","))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Terminal graphics protocol used for image previews.
pub enum ImagePreviewProtocol {
    /// Render images with Unicode half-block characters.
    Halfblocks,
    /// Render images with Sixel graphics.
    Sixel,
    /// Render images with Kitty graphics.
    Kitty,
    /// Render images with iTerm2 inline images.
    Iterm2,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// Vertical arrangement for prompt, results, and preview.
pub enum TuiLayout {
    /// Prompt at the bottom with results above it.
    #[default]
    Default,
    /// Prompt at the top with results below it.
    Reverse,
    /// Prompt at the bottom with a top-down result list.
    ReverseList,
}

impl TuiLayout {
    pub(crate) fn prompt_at_bottom(self) -> bool {
        matches!(self, Self::Default | Self::ReverseList)
    }

    pub(crate) fn list_bottom_up(self) -> bool {
        matches!(self, Self::Default)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// RGB color used by the TUI style options.
pub struct TuiRgb {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
}

impl From<TuiRgb> for Color {
    fn from(color: TuiRgb) -> Self {
        Self::Rgb {
            r: color.r,
            g: color.g,
            b: color.b,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// Optional colors for TUI rendering.
pub struct TuiStyle {
    /// Color for the selection pointer.
    pub pointer: Option<TuiRgb>,
    /// Color for matched text.
    pub highlight: Option<TuiRgb>,
    /// Color for matched text on the selected row.
    pub highlight_selected: Option<TuiRgb>,
    /// Foreground color for the selected row.
    pub selected_fg: Option<TuiRgb>,
    /// Background color for the selected row.
    pub selected_bg: Option<TuiRgb>,
}

impl TuiStyle {
    pub(crate) fn pointer_color(&self) -> Option<Color> {
        self.pointer.map(Color::from)
    }

    pub(crate) fn highlight_color(&self, selected: bool) -> Color {
        if selected {
            self.highlight_selected
                .or(self.highlight)
                .map(Color::from)
                .unwrap_or(Color::Yellow)
        } else {
            self.highlight.map(Color::from).unwrap_or(Color::Yellow)
        }
    }

    pub(crate) fn selected_bg_color(&self) -> Color {
        self.selected_bg
            .map(Color::from)
            .unwrap_or(DEFAULT_SELECTED_ROW_BG)
    }

    pub(crate) fn selected_fg_color(&self) -> Option<Color> {
        self.selected_fg.map(Color::from)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// A key binding and the action it triggers.
pub struct KeyBinding {
    /// Key name in the TUI binding syntax.
    pub key: String,
    /// Action triggered by the key.
    pub action: BindingAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Action assigned to a key binding.
pub enum BindingAction {
    /// Accept the current selection.
    Accept,
    /// Abort the interface.
    Abort,
    /// Clear the query text.
    ClearQuery,
    /// Move the selected row up.
    MoveSelectionUp,
    /// Move the selected row down.
    MoveSelectionDown,
    /// Move to the first row.
    MoveSelectionFirst,
    /// Move to the last row.
    MoveSelectionLast,
    /// Move one page up.
    PageUp,
    /// Move one page down.
    PageDown,
    /// Toggle the selected row mark.
    ToggleMark,
    /// Toggle the selected row mark and move down.
    ToggleMarkAndDown,
    /// Toggle the selected row mark and move up.
    ToggleMarkAndUp,
    /// Move the query cursor to the start.
    MoveCursorStart,
    /// Move the query cursor to the end.
    MoveCursorEnd,
    /// Move the query cursor left.
    MoveCursorLeft,
    /// Move the query cursor right.
    MoveCursorRight,
    /// Delete the character before the cursor.
    Backspace,
    /// Delete the character at the cursor.
    Delete,
    /// Scroll preview up.
    PreviewUp,
    /// Scroll preview down.
    PreviewDown,
    /// Scroll preview one page up.
    PreviewPageUp,
    /// Scroll preview one page down.
    PreviewPageDown,
    /// Scroll preview to the top.
    PreviewTop,
    /// Scroll preview to the bottom.
    PreviewBottom,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Result returned by an interactive TUI session.
pub enum TuiOutcome {
    /// The user accepted one or more candidates.
    Accepted {
        /// Accepted candidate ids.
        ids: Vec<usize>,
        /// Final query text.
        query: String,
        /// Matched expected key, when acceptance used one.
        expect: Option<String>,
    },
    /// Acceptance was requested with no selected candidate.
    NoSelection,
    /// The user aborted the interface.
    Aborted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Message sent to a streaming TUI session.
pub enum CandidateStreamMessage {
    /// Append one candidate.
    Candidate(Candidate),
    /// Mark the stream as finished.
    Finished,
    /// Stop the session with an error.
    Error(String),
}
