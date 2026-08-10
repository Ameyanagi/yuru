use crate::{QueryVariant, SearchKey};
use proptest::prelude::*;

use super::*;

/// Scores through the general Unicode path, bypassing the ASCII fast path.
fn score_unicode_text_for_test(pattern: &str, text: &str, case_sensitive: bool) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }

    if case_sensitive {
        score_unicode_text::<true>(pattern, text)
    } else {
        score_unicode_text::<false>(pattern, text)
    }
}

#[test]
fn subsequence_match_basic() {
    assert!(score_text("abc", "a_b_c", false).is_some());
    assert!(score_text("abc", "acb", false).is_none());
}

#[test]
fn exact_match_scores_above_prefix_and_fuzzy() {
    let exact = score_text("abc", "abc", false).unwrap();
    let prefix = score_text("abc", "abcdef", false).unwrap();
    let fuzzy = score_text("abc", "a_b_c", false).unwrap();

    assert!(exact > prefix);
    assert!(prefix > fuzzy);
}

#[test]
fn case_insensitive_scoring_ignores_case_on_both_sides() {
    assert!(score_text("abc", "ABC", false).is_some());
    assert!(score_text("ABC", "abc", false).is_some());
    assert!(score_text("abc", "xxABCxx", false).is_some());
    // Folding reaches the same match with the same bonuses; the only difference is that the
    // literally spelled candidate also collects `BONUS_CASE_EXACT`.
    assert_eq!(
        score_text("abc", "ABC", false).unwrap() + BONUS_CASE_EXACT,
        score_text("abc", "abc", false).unwrap()
    );
}

#[test]
fn case_sensitive_scoring_keeps_case_significant() {
    assert!(score_text("abc", "ABC", true).is_none());
    assert!(score_text("ABC", "abc", true).is_none());
    assert!(score_text("abc", "abc", true).is_some());
}

#[test]
fn case_insensitive_scoring_folds_non_ascii_case() {
    assert!(score_text("éa", "ÉA.txt", false).is_some());
    assert!(score_text("éa", "ÉA.txt", true).is_none());
    assert!(score_text("ЖУК", "жук.txt", false).is_some());
}

/// Both candidates spell the query's `f` as `F`, so neither collects the exact-case bonus
/// and the camelCase hump is the only thing separating them. Comparing `fooBar` against
/// `foobar` instead would compare two effects at once, because `foobar` is spelled the way
/// the query is.
#[test]
fn case_insensitive_scoring_still_rewards_camel_boundaries() {
    let camel = score_text("fb", "FooBar", false).unwrap();
    let flat = score_text("fb", "Foobar", false).unwrap();

    assert_eq!(camel, flat + BONUS_CAMEL_OR_NUMBER);
    assert!(camel > flat);
}

/// The deliberate ordering decision: a literally spelled match outbids a camelCase hump, so
/// `foobar` beats `fooBar` for the lowercase query `fb`, but only by the 5 points that
/// separate the two bonuses.
#[test]
fn exact_case_bonus_outbids_a_camel_hump_by_five_points() {
    let literal = score_text("fb", "foobar", false).unwrap();
    let camel = score_text("fb", "fooBar", false).unwrap();

    assert!(literal > camel);
    assert_eq!(literal - camel, BONUS_CASE_EXACT - BONUS_CAMEL_OR_NUMBER);
}

/// Requirement: `--filter readme` over the four `readme` spellings must rank the literal
/// spelling first, and the camelCase bonus must keep `ReadMe.md` ahead of `README.md`.
#[test]
fn exact_case_bonus_ranks_case_variants_literal_first() {
    let literal = score_text("readme", "readme.md", false).unwrap();
    let camel = score_text("readme", "ReadMe.md", false).unwrap();
    let upper = score_text("readme", "README.md", false).unwrap();

    assert!(literal > camel, "{literal} should beat {camel}");
    assert!(camel > upper, "{camel} should beat {upper}");
    assert_eq!(literal, upper + BONUS_CASE_EXACT);
    assert_eq!(camel, upper + BONUS_CAMEL_OR_NUMBER);
}

/// The bonus is awarded once per match, not per matched character: a longer query must not
/// earn a larger case bonus, or a long query would swamp every boundary bonus.
#[test]
fn exact_case_bonus_does_not_scale_with_query_length() {
    let short =
        score_text("re", "re.md", false).unwrap() - score_text("re", "RE.md", false).unwrap();
    let long = score_text("readme", "readme.md", false).unwrap()
        - score_text("readme", "README.md", false).unwrap();

    assert_eq!(short, BONUS_CASE_EXACT);
    assert_eq!(long, BONUS_CASE_EXACT);
}

/// The bonus never outranks a real word boundary, so it can only break ties. The size
/// relationship itself is a compile-time assertion next to the constant.
#[test]
fn exact_case_bonus_never_outranks_a_word_boundary() {
    // Both match at positions 0 and 3 in a 6-character text, so the only difference is that
    // one earns the `_` boundary bonus and the other earns the exact-case bonus.
    let boundary = score_text("fb", "FO_bar", false).unwrap();
    let literal = score_text("fb", "foobar", false).unwrap();

    assert!(boundary > literal, "{boundary} should beat {literal}");
    assert_eq!(boundary - literal, BONUS_BOUNDARY - BONUS_CASE_EXACT);
}

/// A case-sensitive match is exact by construction, so its score must be exactly what it was
/// before the bonus existed: the case-insensitive literal score minus the bonus.
#[test]
fn case_sensitive_scoring_never_collects_the_exact_case_bonus() {
    // Substring pairs, so the same pairs exercise greedy and exact scoring.
    for (pattern, text) in [("readme", "readme.md"), ("bar", "foobar"), ("é", "ébc")] {
        assert_eq!(
            score_text(pattern, text, true).unwrap() + BONUS_CASE_EXACT,
            score_text(pattern, text, false).unwrap(),
            "greedy {pattern} vs {text}"
        );
        assert_eq!(
            score_exact_text(pattern, text, true).unwrap() + BONUS_CASE_EXACT,
            score_exact_text(pattern, text, false).unwrap(),
            "exact {pattern} vs {text}"
        );
    }
}

/// Exact mode lost the same literal-spelling preference, and gets it back the same way.
#[test]
fn exact_mode_prefers_the_literal_spelling() {
    let literal = score_exact_text("readme", "readme.md", false).unwrap();
    let camel = score_exact_text("readme", "ReadMe.md", false).unwrap();

    assert_eq!(literal, camel + BONUS_CASE_EXACT);
}

/// The general Unicode path awards the bonus too, not just the ASCII fast path.
#[test]
fn exact_case_bonus_applies_on_the_unicode_path() {
    let literal = score_unicode_text_for_test("жук", "жук.txt", false).unwrap();
    let folded = score_unicode_text_for_test("жук", "ЖУК.txt", false).unwrap();

    assert_eq!(literal, folded + BONUS_CASE_EXACT);
}

#[test]
fn case_insensitive_exact_scoring_ignores_case() {
    assert!(score_exact_text("abc", "xxABCxx", false).is_some());
    assert!(score_exact_text("abc", "xxABCxx", true).is_none());
    assert!(score_exact_text("abc", "a_b_c", false).is_none());
    assert_eq!(
        score_exact_text("abc", "ABC", false),
        score_exact_text("abc", "abc", true)
    );
}

#[test]
fn case_insensitive_exact_scoring_folds_non_ascii_case() {
    assert_eq!(
        score_exact_text("é", "xÉy", false),
        score_exact_text("é", "xéy", true)
    );
    assert!(score_exact_text("é", "xÉy", true).is_none());
}

/// `İ` (U+0130) lowercases to `i` followed by U+0307 COMBINING DOT ABOVE. Folding it to a
/// bare `i` dropped the dot and let the *substring* `ia` match `İa`, which does not contain
/// it: the dot sits between the two characters.
#[test]
fn case_insensitive_matching_keeps_multi_char_lowercase_tails() {
    assert!(score_exact_text("ia", "İa", false).is_none());
    assert!(eq_ignore_case("İa", "İa"));
    assert!(!eq_ignore_case("ia", "İa"));

    // The character still matches itself, and its written-out lowercase form still matches.
    assert!(score_exact_text("İa", "İA", false).is_some());
    assert!(score_exact_text("i\u{307}a", "i\u{307}A", false).is_some());
}

/// Keeping the dot is not the same as refusing to fold: the written-out lowercase form is
/// what `İ` compares as, so a pattern that is a prefix of *that* matches it.
///
/// This is what `İ` has always compared as everywhere else - the extended path's
/// `comparable` case-folds with `str::to_lowercase`, and the normalized key carries the
/// same expansion - so `yuru --filter i --exact` has matched `İ` since v0.1.11. These
/// assertions are the standard path being brought into line with that, not a new licence to
/// drop the dot: `ia` above is still rejected.
#[test]
fn case_insensitive_matching_folds_a_multi_char_lowercase_mapping() {
    // Only reachable by writing the mapping out; `İ` itself never folds to a bare `i`.
    assert_eq!(fold_case_char('İ'), 'İ');

    // Whichever side spells it as one character.
    assert!(score_exact_text("i\u{307}", "İ", false).is_some());
    assert!(score_exact_text("İ", "i\u{307}", false).is_some());
    assert!(score_text("i\u{307}a", "İa", false).is_some());
    assert!(score_text("İa", "i\u{307}a", false).is_some());

    // A prefix of the expansion, and a subsequence of it.
    assert!(score_exact_text("i", "İ", false).is_some());
    assert!(score_text("ia", "İa", false).is_some());

    // Case-sensitive matching folds nothing, so it must not expand anything either.
    assert!(score_exact_text("i\u{307}", "İ", true).is_none());
    assert!(score_text("i\u{307}a", "İa", true).is_none());
}

/// The expanded retry must not disturb a comparison that already answered.
///
/// It runs only after the as-written comparison found nothing, so wherever that comparison
/// did match, the public entry point returns its score untouched - which is what keeps the
/// expansion from re-scoring the `İ` matches that already worked.
#[test]
fn writing_out_a_multi_char_lowercase_mapping_only_adds_matches() {
    for case_sensitive in [false, true] {
        assert_eq!(
            score_text("İ", "xİy", case_sensitive),
            score_unicode_text_for_test("İ", "xİy", case_sensitive),
        );
        assert_eq!(
            score_exact_text("İ", "xİy", case_sensitive),
            score_exact_folded_text("İ", "xİy", case_sensitive),
        );
    }

    // And where it did not, only the expanded retry can answer.
    assert!(score_unicode_text_for_test("i\u{307}", "İ", false).is_none());
    assert!(score_exact_folded_text("i\u{307}", "İ", false).is_none());
    assert!(score_text("i\u{307}", "İ", false).is_some());
    assert!(score_exact_text("i\u{307}", "İ", false).is_some());
}

/// No character may fold to a single character while its full lowercase mapping is longer:
/// dropping the tail makes text compare equal to text it does not contain.
///
/// U+0130 is the only such character in the Unicode tables `char::to_lowercase` ships
/// today, so the scan is what keeps a future table addition from reintroducing the bug
/// instead of a hand-written list. It also pins that U+0130 is the only one, which is what
/// entitles [`MULTI_CHAR_LOWERCASE`] to be a single constant: a table update that adds a
/// second such character fails here rather than silently going unfolded.
#[test]
fn folding_never_truncates_a_multi_char_lowercase_mapping() {
    let mut seen = Vec::new();
    for code_point in 0..=0x10_FFFF_u32 {
        let Some(ch) = char::from_u32(code_point) else {
            continue;
        };
        if ch.to_lowercase().len() > 1 {
            seen.push(ch);
            assert_eq!(
                fold_case_char(ch),
                ch,
                "U+{code_point:04X} folds to a single character"
            );
        }
    }

    assert_eq!(
        seen,
        [MULTI_CHAR_LOWERCASE],
        "the set of characters with a multi-character lowercase mapping changed"
    );
    assert_eq!(
        MULTI_CHAR_LOWERCASE.to_lowercase().collect::<String>(),
        MULTI_CHAR_LOWERCASE_EXPANSION
    );
}

/// Non-ASCII characters with a one-to-one lowercase mapping must still fold.
#[test]
fn case_insensitive_matching_folds_single_char_lowercase_mappings() {
    // U+212A KELVIN SIGN and U+1E9E LATIN CAPITAL LETTER SHARP S.
    assert!(score_exact_text("k", "\u{212a}", false).is_some());
    assert!(score_exact_text("ß", "\u{1e9e}", false).is_some());
}

/// A repetitive pattern must not make the ASCII case-folded substring scan quadratic.
#[test]
fn ascii_case_insensitive_substring_search_is_not_quadratic() {
    let text = "A".repeat(40_000) + "B";
    let pattern = "a".repeat(20_000) + "b";
    assert!(!naive_folded_scan_affordable(text.len(), pattern.len()));

    let start = std::time::Instant::now();
    assert!(score_exact_text(&pattern, &text, false).is_some());
    assert!(score_exact_text(&pattern, &"A".repeat(40_001), false).is_none());
    let elapsed = start.elapsed();

    // A naive scan needs ~8e8 character comparisons for this input; a linear one is
    // instant even in a debug build.
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "case-folded ASCII substring search took {elapsed:?}"
    );
}

/// The same must hold for the non-ASCII substring and containment scans.
#[test]
fn unicode_case_insensitive_substring_search_is_not_quadratic() {
    let text = "あ".repeat(40_000) + "い";
    let pattern = "あ".repeat(20_000) + "い";
    assert!(!naive_folded_scan_affordable(text.len(), pattern.len()));

    let start = std::time::Instant::now();
    assert!(score_exact_text(&pattern, &text, false).is_some());
    assert!(score_text(&pattern, &text, false).is_some());
    assert!(score_exact_text(&pattern, &"あ".repeat(40_001), false).is_none());
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "case-folded Unicode substring search took {elapsed:?}"
    );
}

/// The linear-time fallback must return the same position the naive scan does.
#[test]
fn folded_index_agrees_with_the_naive_scan() {
    for (text, pattern) in [
        ("xxAbCyy", "abc"),
        ("aaaab", "aab"),
        ("abc", "abcd"),
        ("ÄÖÜ-STRAßE", "öü-stra"),
        ("İi\u{307}stanbul", "i\u{307}st"),
        ("İistanbul", "ist"),
        ("\u{212a}ELVIN", "kelvin"),
    ] {
        let linear = find_folded_index(
            text.chars().map(fold_char::<false>),
            pattern.chars().map(fold_char::<false>),
        );
        let naive = find_ignore_case(text, pattern).map(|offset| text[..offset].chars().count());

        assert_eq!(linear, naive, "searching {pattern:?} in {text:?}");
    }
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
        ("ABC", "abc"),
        ("abc", "xxABCxx"),
        ("fb", "fooBar"),
    ] {
        for case_sensitive in [false, true] {
            assert_eq!(
                score_text(pattern, text, case_sensitive),
                score_unicode_text_for_test(pattern, text, case_sensitive),
                "{pattern:?} in {text:?} case_sensitive={case_sensitive}"
            );
        }
    }
}

#[test]
fn exact_match_requires_contiguous_text() {
    assert!(score_exact_text("abc", "abc.txt", false).is_some());
    assert!(score_exact_text("abc", "a_b_c", false).is_none());
}

#[test]
fn reading_match_scores_below_original_exact() {
    let original = score_key(
        &QueryVariant::original("tokyo"),
        &SearchKey::original("tokyo"),
        false,
    )
    .unwrap();
    let reading = score_key(
        &QueryVariant::original("tokyo"),
        &SearchKey::romaji_reading("tokyoeki"),
        false,
    )
    .unwrap();

    assert!(original > reading);
}

#[test]
fn learned_alias_scores_high_enough() {
    let alias = score_key(
        &QueryVariant::original("nihonbashi"),
        &SearchKey::learned_alias("nihonbashi"),
        false,
    )
    .unwrap();
    let reading = score_key(
        &QueryVariant::original("nihonbashi"),
        &SearchKey::romaji_reading("nihonbashieki"),
        false,
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

/// `NucleoMatcher::default()` is the case-insensitive matcher, so `search`'s default path is
/// unchanged by the case policy becoming configurable.
#[test]
fn nucleo_matcher_default_ignores_case() {
    let mut matcher = NucleoMatcher::default();

    assert!(matcher.score("readme", "README.md").is_some());
    assert!(matcher.score("README", "readme.md").is_some());
}

#[test]
fn nucleo_matcher_case_sensitive_rejects_a_differently_cased_match() {
    let mut matcher = NucleoMatcher::new(true);

    assert!(matcher.score("README", "README.md").is_some());
    assert!(matcher.score("readme", "README.md").is_none());
    assert!(matcher.score("README", "readme.md").is_none());
}

/// nucleo requires an already-folded needle when `ignore_case` is set. With an unfolded one
/// its prefilter and its match matrix disagree and it panics from inside
/// `fuzzy_optimal.rs`, so an uppercase query used to abort the process rather than search.
#[test]
fn nucleo_matcher_ignoring_case_accepts_an_uppercase_pattern() {
    let mut matcher = NucleoMatcher::default();

    assert!(matcher.score("ReadMe", "lib/ReadMe1.md").is_some());
    assert!(matcher.score("ReadMe", "lib/readme1.md").is_some());
    assert!(matcher.score("READ", "src/reading.rs").is_some());
    assert!(matcher.score("ReadMe", "src/nothing.rs").is_none());
}

/// The needle is folded with nucleo's own table, so a character that table does not know
/// (`Ɤ` U+A7CB) is left alone rather than folded into something nucleo will not match.
#[test]
fn nucleo_matcher_leaves_a_pattern_its_fold_table_does_not_know_alone() {
    let mut matcher = NucleoMatcher::default();

    assert!(matcher.score("Ɤ", "Ɤx").is_some());
}

proptest! {
    /// Both case policies must survive an arbitrary pattern: `NucleoMatcher::score` has to
    /// satisfy nucleo's needle contract for every input, not just the ones the CLI produces.
    #[test]
    fn nucleo_matcher_never_panics(
        pattern in "\\PC{0,24}",
        text in "\\PC{0,64}",
        case_sensitive in any::<bool>(),
    ) {
        let mut matcher = NucleoMatcher::new(case_sensitive);
        let _ = matcher.score(&pattern, &text);
    }
}

proptest! {
    #[test]
    fn score_text_never_panics(
        pattern in "\\PC{0,24}",
        text in "\\PC{0,64}",
        case_sensitive in any::<bool>(),
    ) {
        let _ = score_text(&pattern, &text, case_sensitive);
        let _ = score_exact_text(&pattern, &text, case_sensitive);
    }

    /// The ASCII fast path must agree with the general Unicode path in both case modes.
    ///
    /// Printable ASCII only: `char::is_whitespace` and `u8::is_ascii_whitespace` disagree
    /// about `\u{b}`, which is a pre-existing difference between the two bonus functions.
    #[test]
    fn ascii_fast_path_agrees_with_unicode_path(
        pattern in "[ -~]{1,12}",
        text in "[ -~]{0,32}",
        case_sensitive in any::<bool>(),
    ) {
        prop_assert_eq!(
            score_text(&pattern, &text, case_sensitive),
            score_unicode_text_for_test(&pattern, &text, case_sensitive)
        );
    }

    /// Case-folded scoring must find the same matches however the query is cased, and the
    /// query's case may only move the score by the exact-case bonus.
    ///
    /// Every other term is computed from folded comparisons or from the text as written, so
    /// recasing the query cannot touch them.
    #[test]
    fn case_insensitive_scoring_only_lets_query_case_move_the_exact_case_bonus(
        pattern in "[ -~]{0,12}",
        text in "[ -~]{0,32}",
    ) {
        for scorer in [score_text as fn(&str, &str, bool) -> Option<i64>, score_exact_text] {
            let written = scorer(&pattern, &text, false);
            let upper = scorer(&pattern.to_ascii_uppercase(), &text, false);
            prop_assert_eq!(written.is_some(), upper.is_some());
            if let (Some(written), Some(upper)) = (written, upper) {
                prop_assert!(
                    (written - upper).abs() == 0 || (written - upper).abs() == BONUS_CASE_EXACT,
                    "{} vs {} differ by more than the exact-case bonus",
                    written,
                    upper
                );
            }
        }
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
