use crate::{QueryVariant, SearchKey};
use proptest::prelude::*;

use super::*;

fn score_unicode_text_for_test(pattern: &str, text: &str) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }

    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    let compact_score = compact_char_match_score(&pattern_chars, &text_chars)?;

    let exact_bonus = if pattern == text {
        10_000
    } else if text.starts_with(pattern) {
        8_000
    } else if text.contains(pattern) {
        6_000
    } else {
        0
    };

    Some(exact_bonus + compact_score)
}

#[test]
fn subsequence_match_basic() {
    assert!(score_text("abc", "a_b_c").is_some());
    assert!(score_text("abc", "acb").is_none());
}

#[test]
fn exact_match_scores_above_prefix_and_fuzzy() {
    let exact = score_text("abc", "abc").unwrap();
    let prefix = score_text("abc", "abcdef").unwrap();
    let fuzzy = score_text("abc", "a_b_c").unwrap();

    assert!(exact > prefix);
    assert!(prefix > fuzzy);
}

#[test]
fn match_positions_tracks_subsequence_char_indices() {
    let positions = match_positions("abc", "a_b_c", true).unwrap();
    assert_eq!(positions.char_indices, vec![0, 2, 4]);
}

#[test]
fn match_positions_can_ignore_case() {
    let positions = match_positions("read", "README.md", false).unwrap();
    assert_eq!(positions.char_indices, vec![0, 1, 2, 3]);
    assert!(match_positions("read", "README.md", true).is_none());
}

#[test]
fn match_positions_treats_hiragana_and_katakana_as_equivalent() {
    let positions = match_positions("かめら", "カメラ.txt", false).unwrap();
    assert_eq!(positions.char_indices, vec![0, 1, 2]);
}

#[test]
fn match_positions_treats_halfwidth_katakana_as_equivalent() {
    let positions = match_positions("かめら", "ｶﾒﾗ.txt", false).unwrap();
    assert_eq!(positions.char_indices, vec![0, 1, 2]);
}

#[test]
fn match_positions_treats_fullwidth_ascii_as_equivalent() {
    let positions = match_positions("abc", "ＡＢＣ.txt", false).unwrap();
    assert_eq!(positions.char_indices, vec![0, 1, 2]);
}

#[test]
fn match_positions_treats_dash_and_prolonged_sound_as_equivalent() {
    let positions = match_positions("-", "ハッピー.pdf", false).unwrap();
    assert_eq!(positions.char_indices, vec![3]);
}

#[test]
fn match_positions_prefers_better_chunk_over_first_subsequence() {
    let positions = match_positions("bsea", "benches/search.rs", false).unwrap();
    assert_eq!(positions.char_indices, vec![0, 8, 9, 10]);
}

#[test]
fn ascii_fast_path_matches_unicode_path_score() {
    for (pattern, text) in [
        ("abc", "abc"),
        ("abc", "abcdef"),
        ("abc", "a_b_c"),
        ("read", "src/module_42/README.md"),
    ] {
        assert_eq!(
            score_text(pattern, text),
            score_unicode_text_for_test(pattern, text)
        );
    }
}

#[test]
fn exact_match_requires_contiguous_text() {
    assert!(score_exact_text("abc", "abc.txt").is_some());
    assert!(score_exact_text("abc", "a_b_c").is_none());
}

#[test]
fn reading_match_scores_below_original_exact() {
    let original = score_key(
        &QueryVariant::original("tokyo"),
        &SearchKey::original("tokyo"),
    )
    .unwrap();
    let reading = score_key(
        &QueryVariant::original("tokyo"),
        &SearchKey::romaji_reading("tokyoeki"),
    )
    .unwrap();

    assert!(original > reading);
}

#[test]
fn learned_alias_scores_high_enough() {
    let alias = score_key(
        &QueryVariant::original("nihonbashi"),
        &SearchKey::learned_alias("nihonbashi"),
    )
    .unwrap();
    let reading = score_key(
        &QueryVariant::original("nihonbashi"),
        &SearchKey::romaji_reading("nihonbashieki"),
    )
    .unwrap();

    assert!(alias >= reading);
}

#[test]
fn nucleo_matcher_scores_subsequence() {
    let mut matcher = NucleoMatcher::default();

    assert!(matcher.score("rdme", "README.md").is_some());
    assert!(matcher.score("zz", "README.md").is_none());
}

proptest! {
    #[test]
    fn score_text_never_panics(pattern in "\\PC{0,24}", text in "\\PC{0,64}") {
        let _ = score_text(&pattern, &text);
        let _ = score_exact_text(&pattern, &text);
    }

    #[test]
    fn match_positions_are_ordered_and_in_bounds(
        pattern in "\\PC{0,24}",
        text in "\\PC{0,64}",
        case_sensitive in any::<bool>(),
    ) {
        if let Some(positions) = match_positions(&pattern, &text, case_sensitive) {
            let text_len = text.chars().count();
            prop_assert!(positions.char_indices.windows(2).all(|window| window[0] < window[1]));
            prop_assert!(positions.char_indices.iter().all(|index| *index < text_len));
        }
    }
}
