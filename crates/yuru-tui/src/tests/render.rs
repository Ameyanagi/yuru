use yuru_core::KeyKind;

use crate::api::{TuiLayout, TuiStyle};
use crate::preview::PreviewRender;
use crate::render::{render, RenderContext, Viewport};
use crate::state::TuiState;

use super::helpers::{force_test_color_output, scored, scored_with_id};

#[test]
fn render_default_layout_places_prompt_at_bottom() {
    force_test_color_output();
    let mut output = Vec::new();
    let state = TuiState::new("al");
    let results = vec![scored("alpha", KeyKind::Original)];
    render(
        &mut output,
        &state,
        &results,
        RenderContext {
            candidates: &[],
            prompt: "> ",
            header: None,
            footer: None,
            viewport: Viewport { width: 40, rows: 3 },
            layout: TuiLayout::Default,
            preview: None,
            style: &TuiStyle::default(),
            highlight_line: true,
            case_sensitive: false,
            multi: false,
            no_input: false,
            pointer: ">",
            marker: "*",
            ellipsis: "..",
        },
    )
    .unwrap();

    let rendered = String::from_utf8(output).unwrap();
    assert!(rendered.contains("\u{1b}[4;1H> al"), "{rendered:?}");
    assert!(
        rendered.contains("\u{1b}[3;1H\u{1b}[48;2;52;58;70m> "),
        "{rendered:?}"
    );
    assert!(!rendered.contains("\u{1b}[7m"), "{rendered:?}");
}

#[test]
fn render_default_layout_paints_results_bottom_up() {
    force_test_color_output();
    let mut output = Vec::new();
    let state = TuiState::new("");
    let results = vec![
        scored("alpha", KeyKind::Original),
        scored_with_id(1, "beta", KeyKind::Original),
        scored_with_id(2, "gamma", KeyKind::Original),
    ];
    render(
        &mut output,
        &state,
        &results,
        RenderContext {
            candidates: &[],
            prompt: "> ",
            header: None,
            footer: None,
            viewport: Viewport { width: 40, rows: 3 },
            layout: TuiLayout::Default,
            preview: None,
            style: &TuiStyle::default(),
            highlight_line: true,
            case_sensitive: false,
            multi: false,
            no_input: false,
            pointer: ">",
            marker: "*",
            ellipsis: "..",
        },
    )
    .unwrap();

    let rendered = String::from_utf8(output).unwrap();
    assert!(
        rendered.contains("\u{1b}[3;1H\u{1b}[48;2;52;58;70m> \u{1b}[39malpha"),
        "{rendered:?}"
    );
    assert!(rendered.contains("\u{1b}[2;1H  beta"), "{rendered:?}");
    assert!(rendered.contains("\u{1b}[1;1H  gamma"), "{rendered:?}");
}

#[test]
fn render_reverse_layout_places_prompt_at_top() {
    force_test_color_output();
    let mut output = Vec::new();
    let state = TuiState::new("al");
    let results = vec![scored("alpha", KeyKind::Original)];
    render(
        &mut output,
        &state,
        &results,
        RenderContext {
            candidates: &[],
            prompt: "> ",
            header: None,
            footer: None,
            viewport: Viewport { width: 40, rows: 3 },
            layout: TuiLayout::Reverse,
            preview: None,
            style: &TuiStyle::default(),
            highlight_line: true,
            case_sensitive: false,
            multi: false,
            no_input: false,
            pointer: ">",
            marker: "*",
            ellipsis: "..",
        },
    )
    .unwrap();

    let rendered = String::from_utf8(output).unwrap();
    assert!(rendered.contains("\u{1b}[1;1H> al"), "{rendered:?}");
}

#[test]
fn render_preview_pane_prints_preview_text() {
    force_test_color_output();
    let mut output = Vec::new();
    let state = TuiState::new("");
    let results = vec![scored("alpha", KeyKind::Original)];
    render(
        &mut output,
        &state,
        &results,
        RenderContext {
            candidates: &[],
            prompt: "> ",
            header: None,
            footer: None,
            viewport: Viewport { width: 80, rows: 3 },
            layout: TuiLayout::Default,
            preview: Some(PreviewRender::Text {
                text: "preview alpha\nsecond line",
                scroll: 0,
            }),
            style: &TuiStyle::default(),
            highlight_line: true,
            case_sensitive: false,
            multi: false,
            no_input: false,
            pointer: ">",
            marker: "*",
            ellipsis: "..",
        },
    )
    .unwrap();

    let rendered = String::from_utf8(output).unwrap();
    assert!(
        rendered.contains("\u{1b}[1;41Hpreview alpha"),
        "{rendered:?}"
    );
    assert!(rendered.contains("\u{1b}[2;41Hsecond line"), "{rendered:?}");
}

#[test]
fn render_preview_pane_uses_scroll_offset() {
    force_test_color_output();
    let mut output = Vec::new();
    let state = TuiState::new("");
    let results = vec![scored("alpha", KeyKind::Original)];
    render(
        &mut output,
        &state,
        &results,
        RenderContext {
            candidates: &[],
            prompt: "> ",
            header: None,
            footer: None,
            viewport: Viewport { width: 80, rows: 2 },
            layout: TuiLayout::Default,
            preview: Some(PreviewRender::Text {
                text: "first\nsecond\nthird",
                scroll: 1,
            }),
            style: &TuiStyle::default(),
            highlight_line: true,
            case_sensitive: false,
            multi: false,
            no_input: false,
            pointer: ">",
            marker: "*",
            ellipsis: "..",
        },
    )
    .unwrap();

    let rendered = String::from_utf8(output).unwrap();
    assert!(!rendered.contains("first"), "{rendered:?}");
    assert!(rendered.contains("\u{1b}[1;41Hsecond"), "{rendered:?}");
    assert!(rendered.contains("\u{1b}[2;41Hthird"), "{rendered:?}");
}
