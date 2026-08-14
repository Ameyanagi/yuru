use crate::{
    build_index, query::PlainBackend, rank::search, score_exact_text, Candidate, GreedyMatcher,
    SearchConfig, SearchKey,
};

use super::*;

/// Scores one extended query against one display text, as `search` would.
fn extended_score(query: &str, display: &str, config: &SearchConfig) -> Option<i64> {
    let index = build_index([display], &PlainBackend, config);
    let mut matcher = GreedyMatcher::new(config.case_sensitive);
    let mut stats = SearchStats::default();
    let prepared = PreparedQuery::new(query, &PlainBackend, config);
    score_candidate(&prepared, &index[0], &mut matcher, config, &mut stats).map(|hit| hit.score)
}

/// Returns the displays `query` selects, in ranked order.
fn ranked(query: &str, displays: &[&str], config: &SearchConfig) -> Vec<String> {
    let index = build_index(displays.iter().copied(), &PlainBackend, config);
    search(query, &index, &PlainBackend, config)
        .into_iter()
        .map(|hit| hit.display)
        .collect()
}

#[test]
fn split_escaped_space() {
    assert_eq!(split_terms("foo\\ bar baz"), vec!["foo bar", "baz"]);
}

#[test]
fn simple_query_does_not_require_extended_search() {
    assert!(!requires_extended_search("kamera"));
    assert!(requires_extended_search("src !test"));
    assert!(requires_extended_search("^src"));
}

#[test]
fn parse_extended_terms() {
    let parsed = ExtendedQuery::parse("'foo ^bar baz$ !qux | zip", false);
    assert_eq!(parsed.groups.len(), 2);
    assert_eq!(parsed.groups[0][0].mode, TermMode::Exact);
    assert_eq!(parsed.groups[0][1].mode, TermMode::Prefix);
    assert_eq!(parsed.groups[0][2].mode, TermMode::Suffix);
    assert!(parsed.groups[0][3].negated);
}

#[test]
fn extended_negation_filters_candidates() {
    let cfg = SearchConfig::default();
    let index = build_index(["src/main.rs", "src/test.rs"], &PlainBackend, &cfg);
    let results = search("src !test", &index, &PlainBackend, &cfg);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].display, "src/main.rs");
}

#[test]
fn exact_mode_disables_fuzzy_matching() {
    let cfg = SearchConfig {
        exact: true,
        ..SearchConfig::default()
    };
    let index = build_index(["a_b_c", "abc"], &PlainBackend, &cfg);
    let results = search("abc", &index, &PlainBackend, &cfg);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].display, "abc");
}

#[test]
fn scoring_empty_query_matches_candidate() {
    let cfg = SearchConfig::default();
    let index = build_index(["abc"], &PlainBackend, &cfg);
    let mut matcher = GreedyMatcher::default();
    let mut stats = SearchStats::default();
    let prepared = PreparedQuery::new("", &PlainBackend, &cfg);
    assert!(score_candidate(&prepared, &index[0], &mut matcher, &cfg, &mut stats).is_some());
}

/// An exact term used to score `FOO` and `foo` identically, so `'foo` returned whichever the
/// input happened to list first. The exact-case bonus reached the fuzzy path only, because
/// this path compares folded text on both sides.
#[test]
fn exact_term_prefers_the_spelling_the_query_was_typed_with() {
    let cfg = SearchConfig::default();
    assert_eq!(
        extended_score("'foo", "foo", &cfg).unwrap() - extended_score("'foo", "FOO", &cfg).unwrap(),
        BONUS_CASE_EXACT
    );
    assert_eq!(ranked("'foo", &["FOO", "foo"], &cfg), ["foo", "FOO"]);
    assert_eq!(ranked("'foo", &["foo", "FOO"], &cfg), ["foo", "FOO"]);
}

/// The bonus follows the query's own spelling, not lowercase: an uppercase term prefers the
/// uppercase candidate. Smart case is a CLI decision, so `--ignore-case` with an uppercase
/// query reaches this path.
#[test]
fn exact_term_prefers_an_uppercase_spelling_for_an_uppercase_term() {
    let cfg = SearchConfig::default();
    assert_eq!(ranked("'FOO", &["foo", "FOO"], &cfg), ["FOO", "foo"]);
    assert_eq!(ranked("'FOO", &["FOO", "foo"], &cfg), ["FOO", "foo"]);
}

/// `'readme` scored `README.md` and `readme.md` at 9991 apiece, while single-term
/// `--exact readme` ranked the literal spelling first. Both paths must agree.
#[test]
fn extended_exact_term_orders_case_variants_like_the_global_exact_path() {
    let cfg = SearchConfig::default();
    let displays = ["README.md", "readme.md"];
    let reversed = ["readme.md", "README.md"];

    for order in [displays, reversed] {
        assert_eq!(
            ranked("'readme", &order, &cfg),
            ["readme.md", "README.md"],
            "extended path, input order {order:?}"
        );
    }

    let literal = score_exact_text("readme", "readme.md", false).unwrap();
    let folded = score_exact_text("readme", "README.md", false).unwrap();
    assert_eq!(
        literal - folded,
        BONUS_CASE_EXACT,
        "the global path's spread is the one the extended path must reproduce"
    );
    assert_eq!(
        extended_score("'readme", "readme.md", &cfg).unwrap()
            - extended_score("'readme", "README.md", &cfg).unwrap(),
        literal - folded
    );
}

/// Anchors and the boundary form take the bonus too - all of them locate a folded occurrence
/// and so all of them lost the same information.
#[test]
fn every_exact_term_mode_awards_the_case_bonus() {
    let cfg = SearchConfig::default();
    for query in [
        "'readme",
        "^readme",
        "readme.md$",
        "^readme.md$",
        "'readme'",
    ] {
        assert_eq!(
            extended_score(query, "readme.md", &cfg).unwrap()
                - extended_score(query, "README.md", &cfg).unwrap(),
            BONUS_CASE_EXACT,
            "{query}"
        );
    }
}

/// The bonus describes the occurrence the score was computed from, exactly as
/// [`score_exact_text`] does: a folded hit at the start does not become exact because the text
/// happens to spell the term literally somewhere later.
#[test]
fn case_bonus_reads_the_matched_occurrence_and_not_a_later_one() {
    let cfg = SearchConfig::default();
    assert_eq!(
        score_exact_text("foo", "FOO-foo", false),
        score_exact_text("foo", "FOO-bar", false),
        "the global path scores the first folded occurrence and awards no bonus for a later one"
    );
    assert_eq!(
        extended_score("'foo", "FOO-foo", &cfg),
        extended_score("'foo", "FOO-bar", &cfg)
    );
}

#[test]
fn case_sensitive_exact_term_scores_are_unchanged_by_the_case_bonus() {
    let cfg = SearchConfig {
        case_sensitive: true,
        ..SearchConfig::default()
    };
    // Both hits are exact by construction, so neither may collect anything.
    assert_eq!(extended_score("'foo", "foo", &cfg), Some(7000 - 3 + 3000));
    assert_eq!(extended_score("'foo", "FOO", &cfg), None);
}

/// A term normalization changed beyond case was not typed the way any candidate spells it, and
/// its folded positions need not line up with the text as written, so it forfeits the bonus
/// instead of guessing at one.
#[test]
fn a_term_needing_more_than_case_folding_forfeits_the_case_bonus() {
    let cfg = SearchConfig::default();
    assert_eq!(
        extended_score("'ｆｏｏ", "foo.txt", &cfg),
        extended_score("'ｆｏｏ", "FOO.txt", &cfg)
    );
    // Same for a candidate normalization changed beyond case: the full-width text is scored
    // through its normalized key, whose characters need not sit where the original's do.
    assert_eq!(
        extended_score("'foo", "ＦＯＯ.txt", &cfg),
        extended_score("'foo", "ｆｏｏ.txt", &cfg)
    );
}

/// Returns a config that folds case and nothing else, i.e. what `--literal --ignore-case`
/// asks for: no normalized key, so the exact path folds the original key itself.
fn literal_config() -> SearchConfig {
    SearchConfig {
        normalize: false,
        ..SearchConfig::default()
    }
}

/// A fold that resizes a character somewhere else in the key is not this occurrence's
/// business.
///
/// `İ` is the one character whose lowercase mapping is two characters, so a key holding it
/// does not fold to itself character for character. Testing that across the whole key cost
/// such a candidate the bonus for *every* term in the query - here a pure-ASCII term matching
/// an `a` the expansion is nowhere near, which the same word spelled `i` + U+0307 collected.
/// Two spellings of one text ranked 75 points apart on a term neither spelling touches.
#[test]
fn case_bonus_ignores_a_multi_character_fold_elsewhere_in_the_key() {
    for cfg in [SearchConfig::default(), literal_config()] {
        assert_eq!(
            extended_score("'a", "İ a", &cfg),
            extended_score("'a", "i\u{307} a", &cfg),
            "the expansion is before the match and changes nothing about it"
        );
        // And it is the bonus both collect, not the bonus both lost: the same key with the
        // match case-flipped scores exactly `BONUS_CASE_EXACT` lower.
        assert_eq!(
            extended_score("'a", "İ a", &cfg),
            extended_score("'a", "İ A", &cfg).map(|score| score + BONUS_CASE_EXACT)
        );
    }
}

/// A `U+212A` KELVIN SIGN control: it folds to a one-byte `k`, so the key resizes in bytes
/// without resizing in characters. Both the old whole-key test and the new per-occurrence one
/// accept it; it is here to keep the `İ` case above from being read as "non-ASCII forfeits".
#[test]
fn case_bonus_survives_a_fold_that_only_resizes_a_character_in_bytes() {
    let cfg = literal_config();
    assert_eq!(
        extended_score("'a", "\u{212a} a", &cfg),
        extended_score("'a", "K a", &cfg)
    );
}

/// An offset landing *inside* a character's folded form names no character of the key, so
/// there is nothing to check the term against and the bonus is refused.
///
/// The term is the combining dot alone, which the folded haystack spells at the second
/// character of `İ`'s expansion. Both keys fold to the same haystack and so score the same
/// match; only the key that spells the dot itself may collect the bonus.
#[test]
fn case_bonus_refuses_an_offset_inside_a_folded_expansion() {
    let cfg = literal_config();
    assert_eq!(
        extended_score("'\u{307}", "i\u{307}x", &cfg),
        extended_score("'\u{307}", "İx", &cfg).map(|score| score + BONUS_CASE_EXACT)
    );
}

/// A fold that expands *inside* the match cannot pass the as-written check, because the key
/// spells as one character what the term spells as two. This is the guard's stated contract:
/// never credit a spelling the score was not computed from.
#[test]
fn case_bonus_refuses_a_key_that_spells_the_match_as_one_character() {
    let cfg = literal_config();
    assert_eq!(
        extended_score("'i\u{307}", "i\u{307}", &cfg),
        extended_score("'i\u{307}", "İ", &cfg).map(|score| score + BONUS_CASE_EXACT)
    );
}

/// Normalization changing more than case *before* the occurrence leaves no way to name the
/// matched character, so the bonus is refused - but only for occurrences that sit behind it.
#[test]
fn case_bonus_refuses_an_occurrence_behind_a_non_case_normalization() {
    let cfg = SearchConfig::default();
    // `ｆ` normalizes to `f`, a change of width rather than case, so the walk cannot account
    // for the haystack and the `bar` occurrence past it forfeits the bonus.
    assert_eq!(
        extended_score("'bar", "ｆ bar", &cfg),
        extended_score("'bar", "ｆ BAR", &cfg)
    );
    // The same occurrence at the front of the key is named without walking anything.
    assert_eq!(
        extended_score("'bar", "bar ｆ", &cfg),
        extended_score("'bar", "BAR ｆ", &cfg).map(|score| score + BONUS_CASE_EXACT)
    );
}

#[test]
fn exact_term_checks_later_phonetic_keys() {
    let cfg = SearchConfig::default();
    let candidate = Candidate {
        id: 0,
        display: "北京大学".to_string(),
        keys: vec![
            SearchKey::original("北京大学"),
            SearchKey::normalized("北京大学"),
            SearchKey::pinyin_initials("bjdx"),
        ],
    };
    let mut matcher = GreedyMatcher::default();
    let mut stats = SearchStats::default();

    let prepared = PreparedQuery::new("'bjdx", &PlainBackend, &cfg);
    let scored = score_candidate(&prepared, &candidate, &mut matcher, &cfg, &mut stats);

    assert!(scored.is_some());
    assert_eq!(scored.unwrap().key_index, 2);
}

/// The un-memoized walk `KeyReplay` replaced, kept as the oracle it is compared
/// against. Byte-for-byte the pre-#8 `key_text_from` slow path.
fn fresh_walk_key_offset(key_text: &str, haystack: &str, byte_start: usize) -> Option<usize> {
    let mut folded_len = 0usize;
    for (offset, ch) in key_text.char_indices() {
        if folded_len >= byte_start {
            return (folded_len == byte_start).then_some(offset);
        }
        let rest = haystack.get(folded_len..)?;
        let folded = crate::matcher::fold_case_char(ch);
        if rest.starts_with(folded) {
            folded_len += folded.len_utf8();
        } else if ch == crate::matcher::MULTI_CHAR_LOWERCASE
            && rest.starts_with(crate::matcher::MULTI_CHAR_LOWERCASE_EXPANSION)
        {
            folded_len += crate::matcher::MULTI_CHAR_LOWERCASE_EXPANSION.len();
        } else {
            return None;
        }
    }
    (folded_len == byte_start).then_some(key_text.len())
}

#[test]
fn memoized_replay_agrees_with_a_fresh_walk() {
    // Keys mixing 1:1 folds, the İ expansion, KELVIN (shrinking fold), ASCII,
    // and text the fold cannot account for at all.
    let cases: &[&str] = &[
        "İ a b c",
        "abc",
        "AİBİC",
        "\u{212a}elvin İstanbul",
        "İİİ",
        "ｆＷ İ x", // width-normalized characters: the fold does not line up
    ];
    let config = SearchConfig::default();
    for key_text in cases {
        let haystack = comparable(key_text, &config);
        // Ask in an adversarial order: far offset first (forces the full walk),
        // then every offset from both ends, then repeats.
        let mut asks: Vec<usize> = (0..=haystack.len()).collect();
        asks.reverse();
        asks.extend(0..=haystack.len());
        let mut replay = KeyReplay::default();
        for byte_start in asks {
            assert_eq!(
                replay.key_offset_at(key_text, &haystack, byte_start),
                fresh_walk_key_offset(key_text, &haystack, byte_start),
                "key={key_text:?} byte_start={byte_start}"
            );
        }
    }
}

#[test]
fn replay_walks_a_key_prefix_once_however_many_terms_ask() {
    // Issue #8: eight exact terms against a candidate whose İ expansion sits
    // before every match used to replay the whole prefix once PER TERM. The
    // walk is character-counted; with the cache it is bounded by the key
    // length, not terms x length.
    let padding = "x".repeat(1000);
    let display = format!("İ{padding} a b c d e f g h");
    let cfg = SearchConfig::default();
    let index = build_index([display.as_str()], &PlainBackend, &cfg);
    let mut matcher = GreedyMatcher::new(cfg.case_sensitive);
    let mut stats = SearchStats::default();
    let prepared = PreparedQuery::new("'a 'b 'c 'd 'e 'f 'g 'h", &PlainBackend, &cfg);

    REPLAY_STEPS.with(|steps| steps.set(0));
    let scored = score_candidate(&prepared, &index[0], &mut matcher, &cfg, &mut stats);
    let steps = REPLAY_STEPS.with(|steps| steps.get());

    assert!(scored.is_some(), "all eight terms match");
    let key_chars = display.chars().count();
    // One walk of one key's prefix, shared. Without the cache this is ~8x the
    // key length per matching key; the bound is set between the two so the
    // regression cannot come back silently. Slack covers the walk running once
    // per distinct key of the candidate (original + normalized).
    assert!(
        steps <= key_chars * 2 + 16,
        "replay walked {steps} chars for a {key_chars}-char display: \
         the per-term recomputation is back"
    );
}

#[test]
fn ascii_and_offset_zero_hits_never_build_a_replay_cache() {
    // Issue #8 follow-up: the cache must be fetched only past the fast paths.
    // Fetching it as a call-site argument allocated one per exact hit, doubling
    // the allocations of a plain `^a` search over ASCII candidates.
    let cfg = SearchConfig::default();
    let index = build_index(["alpha beta", "Alpha Beta"], &PlainBackend, &cfg);
    let mut matcher = GreedyMatcher::new(cfg.case_sensitive);
    let mut stats = SearchStats::default();
    let prepared = PreparedQuery::new("^a 'beta", &PlainBackend, &cfg);

    REPLAY_FETCHES.with(|fetches| fetches.set(0));
    for candidate in &index {
        score_candidate(&prepared, candidate, &mut matcher, &cfg, &mut stats);
    }

    // Pure-ASCII candidates resolve every bonus on the fast paths. Fetches, not
    // walk steps, are what the eager-argument regression produced: the cache
    // was allocated per hit and then never consulted, so a step counter reads
    // zero either way and only a fetch counter can see it.
    assert_eq!(
        REPLAY_FETCHES.with(|fetches| fetches.get()),
        0,
        "an ASCII exact hit fetched (and so allocated) a replay cache"
    );
}
