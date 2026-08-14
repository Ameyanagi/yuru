use std::collections::BTreeMap;

use unicode_segmentation::UnicodeSegmentation;
use yuru_core::KeyKind;

use crate::api::{TuiLayout, TuiStyle};
use crate::preview::PreviewRender;
use crate::render::{
    display_width, preview_geometry, render, terminal_safe_prefix, truncate_to_width_with_ellipsis,
    RenderContext, Viewport,
};
use crate::state::TuiState;
use crate::TuiAction;

use super::helpers::{force_test_color_output, scored, scored_with_id};

/// Length, final byte, and parameter bytes of the CSI sequence at the start of
/// `text`, so a rendered frame can be replayed without its escape codes.
fn csi_sequence(text: &str) -> Option<(usize, u8, &str)> {
    let bytes = text.as_bytes();
    if !bytes.starts_with(b"\x1b[") {
        return None;
    }
    bytes[2..]
        .iter()
        .position(|byte| (0x40..=0x7e).contains(byte))
        .map(|index| (index + 3, bytes[2 + index], &text[2..2 + index]))
}

/// The characters a frame prints, with its escape sequences removed — the text
/// the terminal is left holding.
fn without_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut offset = 0usize;
    while offset < text.len() {
        if let Some((len, _, _)) = csi_sequence(&text[offset..]) {
            offset += len;
            continue;
        }
        let ch = text[offset..]
            .chars()
            .next()
            .expect("valid character boundary");
        assert_ne!(ch, '\u{1b}', "unhandled escape sequence in {text:?}");
        out.push(ch);
        offset += ch.len_utf8();
    }
    out
}

/// Replays a rendered frame and returns, per one-based terminal row, the last
/// column that row actually paints. `MoveTo` starts a run at a column and every
/// other CSI sequence is skipped, since the terminal keeps composing across
/// styling: a run's width is therefore measured over the grapheme clusters of
/// its escape-free text, so a cluster written across an SGR sequence costs what
/// it prints rather than the sum of its halves.
fn painted_row_extents(rendered: &str) -> BTreeMap<usize, usize> {
    let mut runs: Vec<(usize, usize, String)> = vec![(1, 1, String::new())];
    let mut offset = 0usize;
    while offset < rendered.len() {
        if let Some((len, final_byte, params)) = csi_sequence(&rendered[offset..]) {
            if final_byte == b'H' {
                let mut parts = params.split(';');
                let row = parts.next().and_then(|part| part.parse().ok()).unwrap_or(1);
                let column = parts.next().and_then(|part| part.parse().ok()).unwrap_or(1);
                runs.push((row, column, String::new()));
            }
            offset += len;
            continue;
        }

        let ch = rendered[offset..]
            .chars()
            .next()
            .expect("valid character boundary");
        assert_ne!(ch, '\u{1b}', "unhandled escape sequence in {rendered:?}");
        runs.last_mut().expect("a run is always open").2.push(ch);
        offset += ch.len_utf8();
    }

    let mut extents = BTreeMap::new();
    for (row, column, text) in runs {
        if text.is_empty() {
            continue;
        }
        let entry = extents.entry(row).or_insert(0);
        *entry = (*entry).max(column.saturating_sub(1) + display_width(&text));
    }
    extents
}

/// SGR parameters `TuiStyle::default()` paints the selected row's background
/// with, as they appear in a rendered frame.
const SELECTED_ROW_BG: &str = "48;2;52;58;70";

/// Replays a rendered frame and returns, for every character painted on
/// one-based terminal `row`, the SGR parameters of the background colour that
/// was in effect when the terminal printed it — `None` for the default.
///
/// Only the background is tracked, because that is what `bg+` paints and what a
/// record's own SGR sequences can steal from the row's padding. `0` and `49`
/// clear it, `40`–`47` and `100`–`107` set it, and `48` sets it from the
/// extended form whose arguments follow it and are not parameters of their own.
fn row_cell_backgrounds(rendered: &str, row: usize) -> Vec<(char, Option<String>)> {
    let mut cells = Vec::new();
    let mut current_row = 1usize;
    let mut background: Option<String> = None;
    let mut offset = 0usize;
    while offset < rendered.len() {
        if let Some((len, final_byte, params)) = csi_sequence(&rendered[offset..]) {
            match final_byte {
                b'H' => {
                    current_row = params
                        .split(';')
                        .next()
                        .and_then(|part| part.parse().ok())
                        .unwrap_or(1);
                }
                b'm' => apply_background_sgr(params, &mut background),
                _ => {}
            }
            offset += len;
            continue;
        }

        let ch = rendered[offset..]
            .chars()
            .next()
            .expect("valid character boundary");
        assert_ne!(ch, '\u{1b}', "unhandled escape sequence in {rendered:?}");
        if current_row == row {
            cells.push((ch, background.clone()));
        }
        offset += ch.len_utf8();
    }
    cells
}

fn apply_background_sgr(params: &str, background: &mut Option<String>) {
    let params: Vec<&str> = params.split(';').collect();
    let mut index = 0usize;
    while index < params.len() {
        let parameter = params[index];
        // An omitted parameter is a `0`, the full reset.
        let value = if parameter.is_empty() {
            Some(0u16)
        } else {
            parameter.parse::<u16>().ok()
        };
        match value {
            Some(0) | Some(49) => *background = None,
            Some(value) if (40..=47).contains(&value) || (100..=107).contains(&value) => {
                *background = Some(parameter.to_string());
            }
            Some(48) => {
                let arguments = match params
                    .get(index + 1)
                    .and_then(|part| part.parse::<u16>().ok())
                {
                    Some(5) => 2,
                    Some(2) => 4,
                    _ => 0,
                };
                let end = (index + 1 + arguments).min(params.len());
                *background = Some(params[index..end].join(";"));
                index = end - 1;
            }
            _ => {}
        }
        index += 1;
    }
}

/// Renders one wide-text result with `query` as the live query and returns the
/// raw frame.
fn render_wide_frame(query: &str, display: &str, viewport: Viewport, ellipsis: &str) -> String {
    force_test_color_output();
    let mut output = Vec::new();
    let state = TuiState::new(query);
    let results = vec![scored(display, KeyKind::Original)];
    render(
        &mut output,
        &state,
        &results,
        RenderContext {
            candidates: &[],
            prompt: "> ",
            header: None,
            footer: None,
            viewport,
            layout: TuiLayout::Default,
            preview: None,
            style: &TuiStyle::default(),
            highlight_line: true,
            case_sensitive: false,
            multi: false,
            no_input: false,
            pointer: ">",
            marker: "*",
            ellipsis,
            ansi: false,
        },
    )
    .unwrap();
    String::from_utf8(output).unwrap()
}

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
            ansi: false,
        },
    )
    .unwrap();

    let rendered = String::from_utf8(output).unwrap();
    assert!(rendered.contains("\u{1b}[4;1H> al"), "{rendered:?}");
    assert!(rendered.contains("\u{1b}[?25h"), "{rendered:?}");
    assert!(
        rendered.contains("\u{1b}[3;1H\u{1b}[48;2;52;58;70m> "),
        "{rendered:?}"
    );
    assert!(!rendered.contains("\u{1b}[7m"), "{rendered:?}");
}

#[test]
fn render_positions_cursor_by_display_width_and_handles_zero_width() {
    force_test_color_output();
    let state = TuiState::new("日本");

    for (width, expected_cursor) in [(40, "\u{1b}[3;7H"), (0, "\u{1b}[3;1H")] {
        let mut output = Vec::new();
        render(
            &mut output,
            &state,
            &[],
            RenderContext {
                candidates: &[],
                prompt: "> ",
                header: None,
                footer: None,
                viewport: Viewport { width, rows: 2 },
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
                ansi: false,
            },
        )
        .unwrap();

        let rendered = String::from_utf8(output).unwrap();
        assert!(rendered.contains(expected_cursor), "{rendered:?}");
    }
}

#[test]
fn render_positions_cursor_after_visible_control_pictures() {
    force_test_color_output();
    let mut output = Vec::new();
    let state = TuiState::new("\tA");

    render(
        &mut output,
        &state,
        &[],
        RenderContext {
            candidates: &[],
            prompt: "\r",
            header: None,
            footer: None,
            viewport: Viewport { width: 40, rows: 2 },
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
            ansi: false,
        },
    )
    .unwrap();

    let rendered = String::from_utf8(output).unwrap();
    assert!(rendered.contains("␍␉A"), "{rendered:?}");
    assert!(rendered.contains("\u{1b}[3;4H"), "{rendered:?}");
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
            ansi: false,
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
            ansi: false,
        },
    )
    .unwrap();

    let rendered = String::from_utf8(output).unwrap();
    assert!(rendered.contains("\u{1b}[1;1H> al"), "{rendered:?}");
}

#[test]
fn render_reverse_no_input_uses_first_row_for_results_and_preview() {
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
            layout: TuiLayout::Reverse,
            preview: Some(PreviewRender::Text {
                text: "preview alpha",
                scroll: 0,
            }),
            style: &TuiStyle::default(),
            highlight_line: true,
            case_sensitive: false,
            multi: false,
            no_input: true,
            pointer: ">",
            marker: "*",
            ellipsis: "..",
            ansi: false,
        },
    )
    .unwrap();

    let rendered = String::from_utf8(output).unwrap();
    assert!(
        rendered.contains("\u{1b}[1;1H\u{1b}[48;2;52;58;70m> "),
        "{rendered:?}"
    );
    assert!(
        rendered.contains("\u{1b}[1;41Hpreview alpha"),
        "{rendered:?}"
    );
}

#[test]
fn preview_geometry_tracks_prompt_presence() {
    let viewport = Viewport { width: 80, rows: 3 };

    let with_prompt = preview_geometry(viewport, TuiLayout::Reverse, true, true).unwrap();
    assert_eq!(with_prompt.top, 1);
    assert_eq!(with_prompt.lines, 3);

    let without_prompt = preview_geometry(viewport, TuiLayout::Reverse, false, true).unwrap();
    assert_eq!(without_prompt.top, 0);
    assert_eq!(without_prompt.lines, 3);
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
            ansi: false,
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
            ansi: false,
        },
    )
    .unwrap();

    let rendered = String::from_utf8(output).unwrap();
    assert!(!rendered.contains("first"), "{rendered:?}");
    assert!(rendered.contains("\u{1b}[1;41Hsecond"), "{rendered:?}");
    assert!(rendered.contains("\u{1b}[2;41Hthird"), "{rendered:?}");
}

#[test]
fn render_neutralizes_terminal_controls_from_all_untrusted_text_surfaces() {
    force_test_color_output();
    let payload = "evil\0\t\r\u{7}\u{7f}\u{1b}]52;c;Y2xpcGJvYXJk\u{7}\u{1b}[999C";
    let mut output = Vec::new();
    let state = TuiState::new("");
    let results = vec![scored(payload, KeyKind::Original)];

    render(
        &mut output,
        &state,
        &results,
        RenderContext {
            candidates: &[],
            prompt: "> ",
            header: Some(payload),
            footer: None,
            viewport: Viewport {
                width: 120,
                rows: 3,
            },
            layout: TuiLayout::Reverse,
            preview: Some(PreviewRender::Text {
                text: payload,
                scroll: 0,
            }),
            style: &TuiStyle::default(),
            highlight_line: true,
            case_sensitive: false,
            multi: false,
            no_input: true,
            pointer: ">",
            marker: "*",
            ellipsis: "..",
            ansi: false,
        },
    )
    .unwrap();

    let rendered = String::from_utf8(output).unwrap();
    assert!(!rendered.contains("\u{1b}]52;"), "{rendered:?}");
    assert!(!rendered.contains('\0'), "{rendered:?}");
    assert!(!rendered.contains('\t'), "{rendered:?}");
    assert!(!rendered.contains('\r'), "{rendered:?}");
    assert!(!rendered.contains('\u{7}'), "{rendered:?}");
    assert!(!rendered.contains('\u{7f}'), "{rendered:?}");
    assert!(!rendered.contains("\u{1b}[999C"), "{rendered:?}");
    assert!(rendered.contains("␀␉␍␇␡␛]52;c;Y2xpcGJvYXJk␇␛[999C"));
}

#[test]
fn render_ansi_mode_allows_only_sgr_sequences() {
    force_test_color_output();
    let display = "\u{1b}[31mred\u{1b}[0m\u{1b}]52;c;YQ==\u{7}";
    let mut output = Vec::new();
    let state = TuiState::new("");
    let results = vec![scored(display, KeyKind::Original)];

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
            layout: TuiLayout::Reverse,
            preview: None,
            style: &TuiStyle::default(),
            highlight_line: false,
            case_sensitive: false,
            multi: false,
            no_input: true,
            pointer: ">",
            marker: "*",
            ellipsis: "..",
            ansi: true,
        },
    )
    .unwrap();

    let rendered = String::from_utf8(output).unwrap();
    assert!(rendered.contains("\u{1b}[31mred\u{1b}[0m"), "{rendered:?}");
    assert!(!rendered.contains("\u{1b}]52;"), "{rendered:?}");
    assert!(rendered.contains("␛]52;c;YQ==␇"), "{rendered:?}");
}

#[test]
fn painted_rows_never_exceed_the_viewport_width_for_wide_text() {
    // The first case is the audit reproduction: 13 characters, 26 columns.
    let cases = [
        "日本語検索テスト用の候補行",
        "日本語 mixed ASCII テキスト mixed",
        "🎉🎊🎈 party 🎁🎀 time 🥳",
        "한국어 검색 테스트 후보 행",
        "中文搜索测试候选行",
    ];

    for display in cases {
        // Widths below the two-column gutter are degenerate and excluded.
        for width in [4usize, 5, 8, 13, 20, 21, 40] {
            let rendered = render_wide_frame(display, display, Viewport { width, rows: 3 }, "..");
            for (row, extent) in painted_row_extents(&rendered) {
                assert!(
                    extent <= width,
                    "row {row} of a {width}-column viewport reached column {extent} \
                     for {display:?}: {rendered:?}"
                );
            }
        }
    }
}

#[test]
fn a_wide_character_at_an_odd_boundary_is_dropped_not_split() {
    let display = "日本語検索テスト用の候補行";
    // Two gutter columns leave 19 for text. The tenth wide character would need
    // columns 19 and 20, so it must be dropped whole rather than half-emitted.
    let rendered = render_wide_frame("", display, Viewport { width: 21, rows: 3 }, "..");

    assert!(rendered.contains("日本語検索テスト用"), "{rendered:?}");
    assert!(!rendered.contains('の'), "{rendered:?}");
    // The selected-row background pads the one unusable column, so the row ends
    // exactly at the viewport edge instead of wrapping.
    assert_eq!(painted_row_extents(&rendered).get(&3).copied(), Some(21));
}

#[test]
fn preview_pane_truncates_a_cjk_line_by_display_columns() {
    force_test_color_output();
    let line = "日本語検索テスト用の候補行".repeat(3);
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
                text: &line,
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
            ansi: false,
        },
    )
    .unwrap();

    let rendered = String::from_utf8(output).unwrap();
    // The 40-column preview pane starts at column 41: 19 wide characters plus
    // the two-column ellipsis exactly fill it.
    assert!(
        rendered.contains("\u{1b}[1;41H日本語検索テスト用の候補行日本語検索テ.."),
        "{rendered:?}"
    );
    for (row, extent) in painted_row_extents(&rendered) {
        assert!(
            extent <= 80,
            "row {row} reached column {extent}: {rendered:?}"
        );
    }
}

#[test]
fn truncation_charges_the_ellipsis_in_display_columns() {
    let line = "日本語検索テスト";

    // ".." costs two columns, leaving nine for text, so four wide characters fit.
    assert_eq!(
        truncate_to_width_with_ellipsis(line, 11, ".."),
        "日本語検.."
    );
    // "…" costs one column, leaving ten, so a fifth character fits.
    assert_eq!(
        truncate_to_width_with_ellipsis(line, 11, "…"),
        "日本語検索…"
    );
    // Sixteen columns hold all eight characters, so nothing is elided.
    assert_eq!(truncate_to_width_with_ellipsis(line, 16, ".."), line);
    // One column cannot hold a wide character, and the ellipsis is clipped too.
    assert_eq!(truncate_to_width_with_ellipsis(line, 1, ".."), ".");
}

/// Every cluster here prints in exactly two columns, even though the per-scalar
/// width sum says four for the emoji modifier sequence and one for the keycap.
const TWO_COLUMN_CLUSTERS: [&str; 3] = ["日\u{301}", "👩\u{1f3fd}", "#\u{fe0f}\u{20e3}"];

#[test]
fn terminal_safe_prefix_budgets_by_grapheme_cluster() {
    for cluster in TWO_COLUMN_CLUSTERS {
        // The whole cluster fits a two-column budget: a combining mark or an
        // emoji modifier is never clipped off the base it belongs to.
        assert_eq!(
            terminal_safe_prefix(&format!("{cluster}x"), false, 2),
            (cluster.to_string(), true)
        );
        // One column cannot hold it, so it is dropped whole rather than
        // half-emitted as a bare base with its continuation scalars discarded.
        assert_eq!(
            terminal_safe_prefix(cluster, false, 1),
            (String::new(), true)
        );
    }

    // SGR escapes still consume no width budget: the two columns are spent on
    // the cluster alone, and the trailing reset — which costs nothing and is
    // the whole remainder of the text — is carried rather than counted as
    // dropped content, so the text does not report itself truncated.
    assert_eq!(
        terminal_safe_prefix("\u{1b}[31m日\u{301}\u{1b}[0m", true, 2),
        ("\u{1b}[31m日\u{301}\u{1b}[0m".to_string(), false)
    );
}

#[test]
fn display_width_stays_additive_at_every_cluster_boundary() {
    // Two flags joined by a ZWJ are two grapheme clusters, and a row may split
    // between them. Whatever the budget, the clipped prefix must cost no more
    // than the budget and must cost exactly what its pieces cost, or the
    // selected row's `bg+` padding disagrees with the truncation and overruns.
    for text in [
        "🇯🇵\u{200d}🇯🇵",
        "日\u{301}本\u{301}",
        "👩\u{1f3fd}👩\u{1f3fd}",
    ] {
        for width in 0..=8 {
            let (clipped, _) = terminal_safe_prefix(text, false, width);
            let clipped_width = display_width(&clipped);
            assert!(
                clipped_width <= width,
                "{clipped:?} costs {clipped_width} of a {width}-column budget"
            );
            let per_cluster: usize = clipped.graphemes(true).map(display_width).sum();
            assert_eq!(per_cluster, clipped_width, "{clipped:?}");
        }
    }
}

#[test]
fn a_grapheme_cluster_survives_truncation_or_is_dropped_whole() {
    // The audit reproduction: a four-column viewport leaves two columns for the
    // result after the gutter, exactly one cluster's worth.
    for display in TWO_COLUMN_CLUSTERS {
        let rendered = render_wide_frame("", display, Viewport { width: 4, rows: 3 }, "..");
        assert!(
            rendered.contains(display),
            "{display:?} was clipped mid-cluster: {rendered:?}"
        );

        // Three columns leave one, which no cluster fits, so every scalar of it
        // is dropped together.
        let rendered = render_wide_frame("", display, Viewport { width: 3, rows: 3 }, "..");
        for ch in display.chars() {
            assert!(
                !rendered.contains(ch),
                "{display:?} left {ch:?} behind: {rendered:?}"
            );
        }
    }

    // Two columns hold the first cluster only; the second goes with its mark.
    let rendered = render_wide_frame(
        "",
        "日\u{301}本\u{301}",
        Viewport { width: 4, rows: 3 },
        "..",
    );
    assert!(rendered.contains("日\u{301}"), "{rendered:?}");
    assert!(!rendered.contains('本'), "{rendered:?}");
}

/// Renders one `--multi` result with `marker` as the marker, marking the row
/// when `marked` is set, and returns the raw frame.
fn render_marked_frame(marked: bool, marker: &str, viewport: Viewport) -> String {
    force_test_color_output();
    let mut output = Vec::new();
    let mut state = TuiState::new("");
    let results = vec![scored("alpha", KeyKind::Original)];
    if marked {
        state.apply_with_results(TuiAction::ToggleMark, &results, false, true, None);
    }
    render(
        &mut output,
        &state,
        &results,
        RenderContext {
            candidates: &[],
            prompt: "> ",
            header: None,
            footer: None,
            viewport,
            layout: TuiLayout::Default,
            preview: None,
            style: &TuiStyle::default(),
            highlight_line: true,
            case_sensitive: false,
            multi: true,
            no_input: false,
            pointer: ">",
            marker,
            ellipsis: "..",
            ansi: false,
        },
    )
    .unwrap();
    String::from_utf8(output).unwrap()
}

#[test]
fn a_wide_custom_marker_reserves_only_the_columns_the_row_prints() {
    // `--multi --marker 界` with nothing marked prints `>` plus a one-column
    // blank, so the gutter costs two columns — not the three the two-column
    // marker would cost — and the selected row's `bg+` padding has to reach the
    // last cell of the viewport.
    let rendered = render_marked_frame(false, "界", Viewport { width: 10, rows: 3 });
    assert!(!rendered.contains('界'), "{rendered:?}");
    assert_eq!(painted_row_extents(&rendered).get(&3).copied(), Some(10));

    // Marking the row replaces that blank with the two-column marker, so the
    // gutter really is three columns and the row still ends at the edge.
    let rendered = render_marked_frame(true, "界", Viewport { width: 10, rows: 3 });
    assert!(rendered.contains('界'), "{rendered:?}");
    assert_eq!(painted_row_extents(&rendered).get(&3).copied(), Some(10));
}

#[test]
fn a_gutter_wider_than_the_viewport_is_clipped_to_it() {
    // A five-character marker of two-column characters costs ten columns, so
    // the pointer plus the marker want eleven of a ten-column row. Shrinking
    // the result width saturates at zero and stops constraining anything, so
    // the gutter has to be bounded by the list itself or the row wraps.
    let rendered = render_marked_frame(true, "界界界界界", Viewport { width: 10, rows: 3 });
    assert_eq!(painted_row_extents(&rendered).get(&3).copied(), Some(10));
    // The pointer keeps its column and the marker is clipped to the other nine,
    // which hold four of its five wide characters.
    assert!(rendered.contains(">界界界界\u{1b}"), "{rendered:?}");
    assert!(!rendered.contains("界界界界界"), "{rendered:?}");

    // A pointer alone can outgrow the row too, with nothing marked.
    let rendered = render_marked_frame(false, "*", Viewport { width: 2, rows: 3 });
    for (row, extent) in painted_row_extents(&rendered) {
        assert!(
            extent <= 2,
            "row {row} reached column {extent}: {rendered:?}"
        );
    }
}

/// Renders one result whose display text carries its own SGR sequences, with
/// `--ansi` handling enabled, and returns the raw frame.
fn render_ansi_frame(display: &str, viewport: Viewport) -> String {
    force_test_color_output();
    let mut output = Vec::new();
    let state = TuiState::new("");
    let results = vec![scored(display, KeyKind::Original)];
    render(
        &mut output,
        &state,
        &results,
        RenderContext {
            candidates: &[],
            prompt: "> ",
            header: None,
            footer: None,
            viewport,
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
            ansi: true,
        },
    )
    .unwrap();
    String::from_utf8(output).unwrap()
}

#[test]
fn a_combining_mark_reaches_its_base_across_an_sgr_sequence() {
    // A control byte ends a grapheme cluster, so `日` and the combining acute
    // after the reset are two clusters even though the terminal draws them as
    // one. Budgeting cluster by cluster is therefore not enough on its own: the
    // scan has to keep going past a full budget through everything that costs
    // no columns, or the accent is clipped off the character it belongs to.
    let text = "\u{1b}[31m日\u{1b}[0m\u{301}";
    assert_eq!(
        terminal_safe_prefix(text, true, 2),
        (text.to_string(), false)
    );

    // The same at the render path: a four-column viewport leaves the result two
    // columns, and the row keeps its colour, its reset, and its accent.
    let rendered = render_ansi_frame(text, Viewport { width: 4, rows: 3 });
    assert!(rendered.contains(text), "{rendered:?}");
    for (row, extent) in painted_row_extents(&rendered) {
        assert!(
            extent <= 4,
            "row {row} reached column {extent}: {rendered:?}"
        );
    }

    // SGR bytes still buy no columns: one column cannot hold the base, so the
    // cluster is dropped whole and the accent goes with it.
    let (clipped, truncated) = terminal_safe_prefix(text, true, 1);
    assert!(!clipped.contains('日'), "{clipped:?}");
    assert!(!clipped.contains('\u{301}'), "{clipped:?}");
    assert!(truncated);
}

/// Grapheme clusters an `--ansi` record can split with an SGR sequence: the
/// cluster written plainly, and the same cluster with a styling sequence
/// between its base and its continuation scalars. The terminal composes each
/// pair identically — the escape buys no columns and breaks nothing apart — so
/// whatever the plain form costs, the styled one costs too.
const SGR_SPLIT_CLUSTERS: [(&str, &str); 5] = [
    // Keycap: the sum of the parts is one column, the composed cluster two.
    ("#\u{fe0f}\u{20e3}", "#\u{1b}[31m\u{fe0f}\u{20e3}"),
    // Emoji modifier: the sum of the parts is four columns, the cluster two.
    ("👩\u{1f3fb}", "👩\u{1b}[0m\u{1f3fb}"),
    // ZWJ sequence.
    ("👩\u{200d}💻", "👩\u{1b}[1m\u{200d}💻"),
    // Regional indicator pair.
    ("🇯🇵", "🇯\u{1b}[32m🇵"),
    // Variation selector.
    ("☺\u{fe0f}", "☺\u{1b}[33m\u{fe0f}"),
];

#[test]
fn an_sgr_sequence_inside_a_cluster_changes_nothing_but_the_bytes() {
    for (plain, styled) in SGR_SPLIT_CLUSTERS {
        for width in 0..=6 {
            let (plain_clipped, plain_truncated) = terminal_safe_prefix(plain, false, width);
            let (styled_clipped, styled_truncated) = terminal_safe_prefix(styled, true, width);

            // The escape is carried through, and what it separated is charged
            // and kept — or dropped — exactly as if it had never been written.
            assert_eq!(
                without_escapes(&styled_clipped),
                plain_clipped,
                "{styled:?} at {width} columns"
            );
            assert_eq!(
                styled_truncated, plain_truncated,
                "{styled:?} at {width} columns"
            );
            let cost = display_width(&without_escapes(&styled_clipped));
            assert!(cost <= width, "{styled_clipped:?} costs {cost} of {width}");
        }
    }
}

#[test]
fn a_cluster_split_by_an_sgr_sequence_survives_the_row_or_is_dropped_whole() {
    for (plain, styled) in SGR_SPLIT_CLUSTERS {
        // The gutter — pointer plus a one-column blank — leaves the result
        // exactly the columns the composed cluster prints in.
        let fits = 2 + display_width(plain);
        let rendered = render_ansi_frame(
            styled,
            Viewport {
                width: fits,
                rows: 3,
            },
        );
        assert!(
            rendered.contains(styled),
            "{styled:?} lost part of its cluster: {rendered:?}"
        );
        for (row, extent) in painted_row_extents(&rendered) {
            assert!(
                extent <= fits,
                "row {row} reached column {extent} of a {fits}-column viewport: {rendered:?}"
            );
        }

        // One column short, the whole cluster goes: painting the base alone
        // would drop a modifier that costs nothing, and painting it with a
        // continuation the terminal composes into a wider glyph would overrun
        // the row.
        let rendered = render_ansi_frame(
            styled,
            Viewport {
                width: fits - 1,
                rows: 3,
            },
        );
        for ch in plain.chars() {
            assert!(
                !rendered.contains(ch),
                "{styled:?} left {ch:?} behind: {rendered:?}"
            );
        }
        for (row, extent) in painted_row_extents(&rendered) {
            assert!(
                extent < fits,
                "row {row} reached column {extent} of a {}-column viewport: {rendered:?}",
                fits - 1
            );
        }
    }
}

#[test]
fn a_keycap_split_by_an_sgr_sequence_does_not_overflow_a_three_column_row() {
    // The audit reproduction for the over-charging direction. The gutter leaves
    // one text column; `#` alone costs one, so charging it and its post-escape
    // tail separately let the row through — and the terminal then composed the
    // pieces into a two-column keycap and painted four columns into three.
    let rendered = render_ansi_frame(
        "#\u{1b}[31m\u{fe0f}\u{20e3}",
        Viewport { width: 3, rows: 3 },
    );
    // The result row is the gutter and the `bg+` padding that replaces the
    // keycap, and it ends at the last column rather than past it.
    assert_eq!(painted_row_extents(&rendered).get(&3).copied(), Some(3));
    assert!(!rendered.contains('#'), "{rendered:?}");
}

#[test]
fn an_emoji_modifier_split_by_an_sgr_sequence_is_not_dropped_from_a_row_it_fits() {
    // The audit reproduction for the under-charging direction. `👩` is charged
    // two columns and the lone skin-tone modifier was then measured as another
    // two-column cluster and dropped, so the row painted a bare `👩` where the
    // whole styled grapheme takes the two columns available.
    let text = "👩\u{1b}[0m\u{1f3fb}";
    assert_eq!(
        terminal_safe_prefix(text, true, 2),
        (text.to_string(), false)
    );

    let rendered = render_ansi_frame(text, Viewport { width: 4, rows: 3 });
    assert!(rendered.contains(text), "{rendered:?}");
}

/// SGR sequences that only ever clear styling. A sequence sitting exactly at
/// the clip point is the last thing the row emits, so one of these belongs to
/// the text that was retained and has to survive.
const CLEARING_BOUNDARY_SGRS: [&str; 18] = [
    // A full reset, written out and written as the empty default.
    "\u{1b}[0m",
    "\u{1b}[m",
    // Leading zeros and an omitted parameter are both a `0`.
    "\u{1b}[00m",
    "\u{1b}[0;m",
    // Partial resets: each turns one attribute back off.
    "\u{1b}[22m",
    "\u{1b}[23m",
    "\u{1b}[24m",
    "\u{1b}[25m",
    "\u{1b}[27m",
    "\u{1b}[28m",
    "\u{1b}[29m",
    "\u{1b}[39m",
    "\u{1b}[49m",
    // A sequence carrying several parameters, every one of them a reset.
    "\u{1b}[0;39;49m",
    // Judged by final state: reverse video set and then cleared ends with
    // nothing active. (Codex review of the first classifier.)
    "\u{1b}[7;27m",
    // Overline-off and framed-off, missing from the first whitelist.
    "\u{1b}[55m",
    "\u{1b}[54m",
    // The colon-form underline-off.
    "\u{1b}[4:0m",
];

/// SGR sequences that leave some styling set, so one at the clip point belongs
/// to the character that was just discarded and must go with it.
const SETTING_BOUNDARY_SGRS: [&str; 13] = [
    // The issue reproduction: a background opener.
    "\u{1b}[41m",
    "\u{1b}[31m",
    "\u{1b}[1m",
    // A reset followed by a set still ends with red active.
    "\u{1b}[0;31m",
    "\u{1b}[39;41m",
    // Extended colour. Its embedded numbers spell partial resets — `49`, `39`,
    // `22`, `29` — and reading them as parameters in their own right would keep
    // a sequence that paints a colour.
    "\u{1b}[38;5;49m",
    "\u{1b}[48;2;39;49;22m",
    // Overline on; its off-opcode 55 is in the clearing set above.
    "\u{1b}[53m",
    // Colon-form underline styles other than 0 select a style.
    "\u{1b}[4:3m",
    // Clearing reverse and then setting it again ends set.
    "\u{1b}[27;7m",
    // A malformed extended colour: whatever follows 38 is unknowable.
    "\u{1b}[38m",
    // The colon-delimited forms of the same thing.
    "\u{1b}[38:5:29m",
    "\u{1b}[48:2:39:49:22m",
];

#[test]
fn a_boundary_sgr_sequence_is_kept_only_when_it_clears_styling() {
    // `a` costs one of the two columns, `界` needs two and one remains, so `界`
    // is dropped whole — and the sequence written immediately before it sits
    // exactly at the clip point.
    for sequence in CLEARING_BOUNDARY_SGRS {
        assert_eq!(
            terminal_safe_prefix(&format!("a{sequence}\u{754c}"), true, 2),
            (format!("a{sequence}"), true),
            "{sequence:?} clears styling and belongs to the retained text"
        );
    }

    for sequence in SETTING_BOUNDARY_SGRS {
        assert_eq!(
            terminal_safe_prefix(&format!("a{sequence}\u{754c}"), true, 2),
            ("a".to_string(), true),
            "{sequence:?} sets styling and belongs to the discarded character"
        );
    }
}

#[test]
fn a_setting_sgr_sequence_inside_the_retained_text_is_untouched() {
    // Only the clip point distinguishes the two. Every sequence the retained
    // text was written around stays exactly where the record put it, opener or
    // not, or a row loses the styling it asked for.
    for sequence in SETTING_BOUNDARY_SGRS {
        let text = format!("{sequence}ab\u{754c}");
        assert_eq!(
            terminal_safe_prefix(&text, true, 2),
            (format!("{sequence}ab"), true),
            "{sequence:?} is inside the retained text"
        );
    }
}

#[test]
fn a_boundary_sgr_opener_does_not_recolour_the_selected_row_padding() {
    // The issue reproduction at the render path. The gutter — pointer plus a
    // one-column blank — takes two of the four columns, leaving the result two:
    // `a` costs one, `界` needs two, and the column `界` vacates is filled with
    // `bg+` padding. Keeping the red opener painted that padding red.
    let rendered = render_ansi_frame("a\u{1b}[41m\u{754c}", Viewport { width: 4, rows: 3 });
    assert!(
        !rendered.contains("\u{1b}[41m"),
        "the discarded character's opener reached the terminal: {rendered:?}"
    );
    assert_eq!(
        row_cell_backgrounds(&rendered, 3),
        vec![
            ('>', Some(SELECTED_ROW_BG.to_string())),
            (' ', Some(SELECTED_ROW_BG.to_string())),
            ('a', Some(SELECTED_ROW_BG.to_string())),
            (' ', Some(SELECTED_ROW_BG.to_string())),
        ],
        "{rendered:?}"
    );

    // Unstyled text pads the same row the same way, so the assertion above is
    // pinning the padding's colour rather than the absence of styling.
    let rendered = render_ansi_frame("a\u{754c}", Viewport { width: 4, rows: 3 });
    assert_eq!(
        row_cell_backgrounds(&rendered, 3),
        vec![
            ('>', Some(SELECTED_ROW_BG.to_string())),
            (' ', Some(SELECTED_ROW_BG.to_string())),
            ('a', Some(SELECTED_ROW_BG.to_string())),
            (' ', Some(SELECTED_ROW_BG.to_string())),
        ],
        "{rendered:?}"
    );
}

#[test]
fn a_boundary_reset_still_reaches_the_terminal() {
    // The other half of the rule, end to end: the reset belongs to the `a` that
    // was kept, and dropping it would let styling opened earlier in the record
    // run on into the rest of the interface.
    let rendered = render_ansi_frame(
        "\u{1b}[31ma\u{1b}[0m\u{754c}",
        Viewport { width: 4, rows: 3 },
    );
    assert!(rendered.contains("\u{1b}[31ma\u{1b}[0m"), "{rendered:?}");
    assert!(!rendered.contains('\u{754c}'), "{rendered:?}");
}

#[test]
fn free_content_is_followed_no_further_than_the_byte_cap() {
    // Nothing in a record made only of SGR sequences and zero-width clusters
    // costs a column, so the budget can never stop the scan and the byte cap
    // has to — including through the widening window that lets a cluster reach
    // its continuation, which must widen to the cap and stop rather than loop.
    for (text, allow_sgr) in [
        ("\u{200b}".repeat(100_000), false),
        ("\u{1b}[31m".repeat(100_000), true),
    ] {
        let (clipped, truncated) = terminal_safe_prefix(&text, allow_sgr, 4);
        assert!(truncated);
        assert!(clipped.len() <= 4 * 64, "{} bytes scanned", clipped.len());
    }

    // The cap bounds where a cluster may begin, not where its bytes may end:
    // `a` starts one byte inside a 64-byte cap and its combining acute lies
    // past it, and a cap that cut there would clip the base off its mark — the
    // same defect from the other side. It costs the one column it prints.
    let text = format!("{}a\u{301}b", "\u{200b}".repeat(21));
    let (clipped, truncated) = terminal_safe_prefix(&text, false, 1);
    assert!(clipped.ends_with("a\u{301}"), "{clipped:?}");
    assert!(truncated);
}

/// Renders a frame with `prompt` as the prompt and returns it.
fn render_prompt_frame(prompt: &str, viewport: Viewport) -> String {
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
            prompt,
            header: None,
            footer: None,
            viewport,
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
            ansi: false,
        },
    )
    .unwrap();
    String::from_utf8(output).unwrap()
}

#[test]
fn a_zero_width_tail_is_not_a_truncation() {
    // U+200B is a grapheme cluster of its own and costs no columns, so this
    // text occupies exactly two of them and nothing has to be dropped. Ending
    // the scan the moment the budget filled left the tail unconsumed and
    // reported the text as truncated, which spent the entire two-column budget
    // on an ellipsis for content that already fit.
    assert_eq!(
        terminal_safe_prefix("ab\u{200b}", false, 2),
        ("ab\u{200b}".to_string(), false)
    );
    assert_eq!(
        truncate_to_width_with_ellipsis("ab\u{200b}", 2, ".."),
        "ab\u{200b}"
    );
    // Anything past the tail that does cost a column still truncates.
    assert_eq!(
        truncate_to_width_with_ellipsis("ab\u{200b}c", 2, ".."),
        ".."
    );

    // At the render path, through the prompt: two columns of prompt in a
    // two-column viewport print as themselves, not as "..".
    let rendered = render_prompt_frame("ab\u{200b}", Viewport { width: 2, rows: 3 });
    assert!(rendered.contains("ab\u{200b}"), "{rendered:?}");
    assert!(!rendered.contains(".."), "{rendered:?}");
    for (row, extent) in painted_row_extents(&rendered) {
        assert!(
            extent <= 2,
            "row {row} reached column {extent}: {rendered:?}"
        );
    }
}
