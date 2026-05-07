use crate::{
    build_index, query::PlainBackend, rank::search, Candidate, GreedyMatcher, SearchConfig,
    SearchKey,
};

use super::*;

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
    let mut matcher = GreedyMatcher;
    let mut stats = SearchStats::default();
    assert!(
        score_candidate("", &index[0], &PlainBackend, &mut matcher, &cfg, &mut stats).is_some()
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
    let mut matcher = GreedyMatcher;
    let mut stats = SearchStats::default();

    let scored = score_candidate(
        "'bjdx",
        &candidate,
        &PlainBackend,
        &mut matcher,
        &cfg,
        &mut stats,
    );

    assert!(scored.is_some());
    assert_eq!(scored.unwrap().key_index, 2);
}
