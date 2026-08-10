use yuru_core::{Candidate, KeyKind, SearchKey, SourceSpan};

use crate::render::{
    highlight_segments_for_result, highlight_segments_for_result_with_ansi, HighlightSegment,
};

use super::helpers::{japanese_romaji_source_map, scored};

#[test]
fn highlight_segments_mark_visible_fuzzy_positions() {
    let result = scored("src/module_42/README.md", KeyKind::Original);
    let segments = highlight_segments_for_result("read", &result, &[], false, 80);

    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "src/module_42/".to_string(),
                highlighted: false,
            },
            HighlightSegment {
                text: "READ".to_string(),
                highlighted: true,
            },
            HighlightSegment {
                text: "ME.md".to_string(),
                highlighted: false,
            },
        ]
    );
}

#[test]
fn highlight_work_is_bounded_to_the_visible_prefix() {
    let display = "a_".repeat(100_000);
    let result = scored(&display, KeyKind::Original);
    let segments = highlight_segments_for_result(&"a".repeat(32), &result, &[], false, 80);

    assert_eq!(
        segments
            .iter()
            .flat_map(|segment| segment.text.chars())
            .count(),
        80
    );
}

#[test]
fn over_limit_patterns_are_not_truncated_into_false_highlights() {
    let display = "a".repeat(64);
    let result = scored(&display, KeyKind::Original);
    let segments = highlight_segments_for_result(&"a".repeat(65), &result, &[], false, 80);

    assert!(segments.iter().all(|segment| !segment.highlighted));
}

#[test]
fn ansi_highlighting_retains_a_trailing_sgr_reset() {
    let result = scored("\u{1b}[31mred\u{1b}[0m", KeyKind::Original);
    let segments = highlight_segments_for_result_with_ansi("red", &result, &[], false, 80, true);

    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].text, "\u{1b}[31mred\u{1b}[0m");
    assert!(segments[0].highlighted);
}

#[test]
fn plain_mode_highlight_marks_direct_matches_from_normalized_key() {
    let result = scored("README.md", KeyKind::Normalized);
    let segments = highlight_segments_for_result("read", &result, &[], false, 80);

    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "READ".to_string(),
                highlighted: true,
            },
            HighlightSegment {
                text: "ME.md".to_string(),
                highlighted: false,
            },
        ]
    );
}

#[test]
fn highlight_segments_prefer_stronger_later_chunk() {
    let result = scored("benches/search.rs", KeyKind::Original);
    let segments = highlight_segments_for_result("bsea", &result, &[], false, 80);

    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "b".to_string(),
                highlighted: true,
            },
            HighlightSegment {
                text: "enches/".to_string(),
                highlighted: false,
            },
            HighlightSegment {
                text: "sea".to_string(),
                highlighted: true,
            },
            HighlightSegment {
                text: "rch.rs".to_string(),
                highlighted: false,
            },
        ]
    );
}

#[test]
fn highlight_segments_skip_negated_terms() {
    let result = scored("src/main.rs", KeyKind::Original);
    let segments = highlight_segments_for_result("src !main", &result, &[], false, 80);

    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "src".to_string(),
                highlighted: true,
            },
            HighlightSegment {
                text: "/main.rs".to_string(),
                highlighted: false,
            },
        ]
    );
}

#[test]
fn highlight_segments_mark_phonetic_matches_when_reading_is_not_visible() {
    let result = scored("北京大学.txt", KeyKind::PinyinInitials);
    let segments = highlight_segments_for_result("bjdx", &result, &[], false, 80);

    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "北京大学".to_string(),
                highlighted: true,
            },
            HighlightSegment {
                text: ".txt".to_string(),
                highlighted: false,
            },
        ]
    );
}

#[test]
fn highlight_segments_mark_japanese_surface_in_mixed_path() {
    let result = scored("tests/日本語.txt", KeyKind::RomajiReading);
    let segments = highlight_segments_for_result("ni", &result, &[], false, 80);

    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "tests/".to_string(),
                highlighted: false,
            },
            HighlightSegment {
                text: "日本語".to_string(),
                highlighted: true,
            },
            HighlightSegment {
                text: ".txt".to_string(),
                highlighted: false,
            },
        ]
    );
}

#[test]
fn highlight_segments_mark_kana_surface_for_romaji_query() {
    let result = scored("カメラ.txt", KeyKind::RomajiReading);
    let segments = highlight_segments_for_result("kamera", &result, &[], false, 80);

    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "カメラ".to_string(),
                highlighted: true,
            },
            HighlightSegment {
                text: ".txt".to_string(),
                highlighted: false,
            },
        ]
    );
}

#[test]
fn highlight_segments_use_source_map_for_japanese_reading() {
    let display = "tests/日本人の.txt";
    let key = SearchKey::romaji_reading("tests/nihonjinno.txt")
        .with_source_map(japanese_romaji_source_map());
    let candidates = vec![Candidate {
        id: 0,
        display: display.to_string(),
        keys: vec![key],
    }];
    let result = scored(display, KeyKind::RomajiReading);

    let segments = highlight_segments_for_result("ni", &result, &candidates, false, 80);
    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "tests/".to_string(),
                highlighted: false,
            },
            HighlightSegment {
                text: "日本人".to_string(),
                highlighted: true,
            },
            HighlightSegment {
                text: "の.txt".to_string(),
                highlighted: false,
            },
        ]
    );

    let segments = highlight_segments_for_result("no", &result, &candidates, false, 80);
    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "tests/日本人".to_string(),
                highlighted: false,
            },
            HighlightSegment {
                text: "の".to_string(),
                highlighted: true,
            },
            HighlightSegment {
                text: ".txt".to_string(),
                highlighted: false,
            },
        ]
    );
}

#[test]
fn highlight_segments_use_source_map_for_chinese_initials() {
    let display = "北京大学.txt";
    let key = SearchKey::pinyin_initials("bjdx").with_source_map(vec![
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
    ]);
    let candidates = vec![Candidate {
        id: 0,
        display: display.to_string(),
        keys: vec![key],
    }];
    let result = scored(display, KeyKind::PinyinInitials);

    let segments = highlight_segments_for_result("bj", &result, &candidates, false, 80);

    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "北京".to_string(),
                highlighted: true,
            },
            HighlightSegment {
                text: "大学.txt".to_string(),
                highlighted: false,
            },
        ]
    );
}

#[test]
fn highlight_segments_use_source_map_for_korean_romanized_keys() {
    let display = "한글.txt";
    let key = SearchKey::korean_romanized("hangeul").with_source_map(vec![
        Some(SourceSpan {
            start_char: 0,
            end_char: 1,
        }),
        Some(SourceSpan {
            start_char: 0,
            end_char: 1,
        }),
        Some(SourceSpan {
            start_char: 0,
            end_char: 1,
        }),
        Some(SourceSpan {
            start_char: 1,
            end_char: 2,
        }),
        Some(SourceSpan {
            start_char: 1,
            end_char: 2,
        }),
        Some(SourceSpan {
            start_char: 1,
            end_char: 2,
        }),
        Some(SourceSpan {
            start_char: 1,
            end_char: 2,
        }),
    ]);
    let candidates = vec![Candidate {
        id: 0,
        display: display.to_string(),
        keys: vec![key],
    }];
    let result = scored(display, KeyKind::KoreanRomanized);

    let segments = highlight_segments_for_result("hg", &result, &candidates, false, 80);

    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "한글".to_string(),
                highlighted: true,
            },
            HighlightSegment {
                text: ".txt".to_string(),
                highlighted: false,
            },
        ]
    );
}

#[test]
fn cjk_highlight_positions_stay_character_based_under_a_column_budget() {
    let display = "日本語検索";
    let result = scored(display, KeyKind::Original);

    // Eleven columns hold five wide characters with one column to spare, so the
    // whole string survives truncation.
    let segments = highlight_segments_for_result("検索", &result, &[], false, 11);
    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "日本語".to_string(),
                highlighted: false,
            },
            HighlightSegment {
                text: "検索".to_string(),
                highlighted: true,
            },
        ]
    );

    // Nine columns drop the fifth character. The fourth is still highlighted at
    // character index 3 — a column-indexed lookup would land on "語" instead.
    let segments = highlight_segments_for_result("検", &result, &[], false, 9);
    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "日本語".to_string(),
                highlighted: false,
            },
            HighlightSegment {
                text: "検".to_string(),
                highlighted: true,
            },
        ]
    );
}

#[test]
fn source_map_highlight_positions_survive_column_truncation() {
    let display = "tests/日本人の.txt";
    let key = SearchKey::romaji_reading("tests/nihonjinno.txt")
        .with_source_map(japanese_romaji_source_map());
    let candidates = vec![Candidate {
        id: 0,
        display: display.to_string(),
        keys: vec![key],
    }];
    let result = scored(display, KeyKind::RomajiReading);

    // Six ASCII columns plus three wide characters is twelve; the thirteenth
    // column cannot hold "の", so the display stops at "tests/日本人". The romaji
    // match still maps back to character indices 6..9.
    let segments = highlight_segments_for_result("ni", &result, &candidates, false, 13);
    assert_eq!(
        segments,
        vec![
            HighlightSegment {
                text: "tests/".to_string(),
                highlighted: false,
            },
            HighlightSegment {
                text: "日本人".to_string(),
                highlighted: true,
            },
        ]
    );
}

#[test]
fn highlighting_follows_the_live_query_case_policy() {
    // A row left on screen from the previous keystroke is highlighted with the case
    // policy the live query text resolves to. Under smart case `abC` is case-sensitive,
    // so `ABC-match` — a case-insensitive hit for the earlier `ab` — paints with no
    // highlight at all rather than claiming a match the live query does not have.
    let result = scored("ABC-match", KeyKind::Original);

    let sensitive = highlight_segments_for_result("abC", &result, &[], true, 80);
    assert!(
        sensitive.iter().all(|segment| !segment.highlighted),
        "{sensitive:?}"
    );

    let insensitive = highlight_segments_for_result("abC", &result, &[], false, 80);
    assert!(
        insensitive.iter().any(|segment| segment.highlighted),
        "{insensitive:?}"
    );
}

#[test]
fn a_phonetic_row_is_not_painted_as_a_match_once_its_key_stops_matching() {
    // Same situation one layer down: the row is a Korean-romanized hit for the earlier
    // query and is still on screen while its replacement runs. Nothing on the surface
    // spells the query, so the phonetic fallbacks would otherwise paint the whole row —
    // a stale row claiming to be a live match. The key is what decides.
    let display = "한글";
    let candidates = vec![Candidate {
        id: 0,
        display: display.to_string(),
        keys: vec![SearchKey::korean_romanized("hangeul")],
    }];
    let result = scored(display, KeyKind::KoreanRomanized);

    let live = highlight_segments_for_result("h", &result, &candidates, false, 80);
    assert_eq!(
        live,
        vec![HighlightSegment {
            text: "한글".to_string(),
            highlighted: true,
        }]
    );

    // `hG` is case-sensitive under smart case and the key `hangeul` does not contain it.
    let stale = highlight_segments_for_result("hG", &result, &candidates, true, 80);
    assert_eq!(
        stale,
        vec![HighlightSegment {
            text: "한글".to_string(),
            highlighted: false,
        }]
    );
}

#[test]
fn a_phonetic_row_whose_key_still_matches_keeps_its_source_map_highlight() {
    // The gate must not cost a genuine match its highlighting: the key matches, only the
    // surface cannot be pointed at character by character.
    let display = "한글.txt";
    let candidates = vec![Candidate {
        id: 0,
        display: display.to_string(),
        keys: vec![SearchKey::korean_romanized("hangeul.txt")],
    }];
    let result = scored(display, KeyKind::KoreanRomanized);

    let segments = highlight_segments_for_result("hng", &result, &candidates, false, 80);
    assert!(
        segments.iter().any(|segment| segment.highlighted),
        "{segments:?}"
    );
}
