use yuru_core::{Candidate, KeyKind, SearchKey, SourceSpan};

use crate::render::{highlight_segments_for_result, HighlightSegment};

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
