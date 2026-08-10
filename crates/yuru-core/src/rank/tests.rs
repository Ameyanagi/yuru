use crate::{build_index, query::PlainBackend};

use super::*;

#[test]
fn sorting_is_deterministic_on_equal_scores() {
    let cfg = SearchConfig::default();
    let candidates = build_index(["abc-one", "abc-two"], &PlainBackend, &cfg);
    let results = search("abc", &candidates, &PlainBackend, &cfg);

    assert_eq!(results[0].display, "abc-one");
    assert_eq!(results[1].display, "abc-two");
}

#[test]
fn search_hot_path_does_not_call_reading_generator() {
    let cfg = SearchConfig::default();
    let candidates = build_index(["東京駅"], &PlainBackend, &cfg);
    let mut matcher = GreedyMatcher::default();

    let (_results, stats) =
        search_with_stats("tokyo", &candidates, &PlainBackend, &mut matcher, &cfg);

    assert_eq!(stats.reading_generation_calls, 0);
}

#[test]
fn case_insensitive_search_matches_through_the_original_key() {
    let cfg = SearchConfig::default();
    let candidates = build_index(["README.md", "Cargo.toml"], &PlainBackend, &cfg);
    let results = search("read", &candidates, &PlainBackend, &cfg);

    assert_eq!(
        results
            .iter()
            .map(|result| (result.display.as_str(), result.key_kind))
            .collect::<Vec<_>>(),
        vec![("README.md", crate::KeyKind::Original)]
    );
}

#[test]
fn case_insensitive_search_survives_disabled_normalization() {
    let cfg = SearchConfig {
        normalize: false,
        ..SearchConfig::default()
    };
    let candidates = build_index(["README.md", "Cargo.toml"], &PlainBackend, &cfg);

    for query in ["read", "READ"] {
        let results = search(query, &candidates, &PlainBackend, &cfg);
        assert_eq!(results.len(), 1, "{query}");
        assert_eq!(results[0].display, "README.md", "{query}");
    }
}

#[test]
fn case_sensitive_search_keeps_case_significant() {
    let cfg = SearchConfig {
        case_sensitive: true,
        ..SearchConfig::default()
    };
    let candidates = build_index(["README.md", "readme.md"], &PlainBackend, &cfg);

    let results = search("read", &candidates, &PlainBackend, &cfg);
    assert_eq!(
        results
            .iter()
            .map(|result| result.display.as_str())
            .collect::<Vec<_>>(),
        vec!["readme.md"]
    );
}

#[test]
fn exact_mode_honours_the_case_policy() {
    let insensitive = SearchConfig {
        exact: true,
        ..SearchConfig::default()
    };
    let sensitive = SearchConfig {
        case_sensitive: true,
        ..insensitive.clone()
    };
    let candidates = build_index(["xxABCxx", "nope"], &PlainBackend, &insensitive);

    assert_eq!(
        search("abc", &candidates, &PlainBackend, &insensitive)
            .iter()
            .map(|result| result.display.as_str())
            .collect::<Vec<_>>(),
        vec!["xxABCxx"]
    );
    assert!(search("abc", &candidates, &PlainBackend, &sensitive).is_empty());
}

/// `İ` lowercases to `i` plus U+0307 COMBINING DOT ABOVE, so `İa` contains no plain `i`
/// and an exact `ia` must not match it. The fuzzy match, which reads the normalized key
/// holding the written-out expansion, is unaffected.
#[test]
fn exact_mode_does_not_match_across_a_multi_char_lowercase_mapping() {
    let exact = SearchConfig {
        exact: true,
        ..SearchConfig::default()
    };
    let candidates = build_index(["İa"], &PlainBackend, &exact);

    assert!(search("ia", &candidates, &PlainBackend, &exact).is_empty());
    assert!(!search("İa", &candidates, &PlainBackend, &exact).is_empty());

    let fuzzy = SearchConfig::default();
    let candidates = build_index(["İa"], &PlainBackend, &fuzzy);

    assert!(!search("ia", &candidates, &PlainBackend, &fuzzy).is_empty());
}

/// Case-sensitive by construction: it scores only when the pattern *is* the text, and it
/// never says it folds case, so [`MatcherBackend::folds_case`] keeps its `false` default.
/// An embedder's matcher is exactly this: something yuru knows nothing about.
struct EqualityMatcher;

impl MatcherBackend for EqualityMatcher {
    fn score(&mut self, pattern: &str, text: &str) -> Option<i64> {
        (pattern == text).then_some(1000)
    }
}

/// A caller-owned matcher is never told the case policy, so search must not assume it folds
/// case: the case-fold-only normalized key is the only key whose text a case-sensitive
/// matcher can equal, and dropping it would answer "no match" for `abc` against `ABC`.
#[test]
fn caller_owned_matcher_that_does_not_fold_case_still_gets_the_case_folded_key() {
    let cfg = SearchConfig::default();
    let candidates = build_index(["ABC"], &PlainBackend, &cfg);

    let mut matcher = EqualityMatcher;
    let (results, _stats) =
        search_with_stats("abc", &candidates, &PlainBackend, &mut matcher, &cfg);
    assert_eq!(
        results
            .iter()
            .map(|result| (result.display.as_str(), result.key_kind))
            .collect::<Vec<_>>(),
        vec![("ABC", crate::KeyKind::Normalized)]
    );
}

/// The extended path selects keys in `fzf_query::match_fuzzy_term`, so it needs the same
/// guard as the standard path above.
#[test]
fn caller_owned_matcher_gets_the_case_folded_key_on_the_extended_path_too() {
    let cfg = SearchConfig::default();
    let candidates = build_index(["ABC"], &PlainBackend, &cfg);

    let mut matcher = EqualityMatcher;
    let (results, _stats) =
        search_with_stats("abc | nope", &candidates, &PlainBackend, &mut matcher, &cfg);
    assert_eq!(
        results
            .iter()
            .map(|result| (result.display.as_str(), result.key_kind))
            .collect::<Vec<_>>(),
        vec![("ABC", crate::KeyKind::Normalized)]
    );
}

/// `nucleo-matcher` folds case with its own table, which does not know `Ɤ` (U+A7CB, one of
/// 55 characters where it disagrees with yuru's index-time folding). It therefore must not
/// claim to fold case: the normalized key is the only one that reaches `Ɤ` from `ɤ`.
#[test]
fn nucleo_search_matches_a_character_its_own_case_fold_table_does_not_know() {
    for matcher_algo in [MatcherAlgo::FzfV2, MatcherAlgo::Nucleo] {
        let cfg = SearchConfig {
            matcher_algo,
            ..SearchConfig::default()
        };
        let candidates = build_index(["Ɤx"], &PlainBackend, &cfg);

        assert_eq!(
            search("ɤ", &candidates, &PlainBackend, &cfg)
                .iter()
                .map(|result| result.display.as_str())
                .collect::<Vec<_>>(),
            vec!["Ɤx"],
            "{matcher_algo:?}"
        );
    }
}

/// `matcher_for_config` used to build `NucleoMatcher::default()` unconditionally, so
/// `--algo nucleo` / `--algo fzf-v2` matched case-insensitively even under
/// `--no-ignore-case` or an uppercase smart-case query. The nucleo paths must honour the
/// same case policy as the greedy path.
#[test]
fn nucleo_case_sensitive_search_rejects_a_differently_cased_candidate() {
    for matcher_algo in [MatcherAlgo::FzfV2, MatcherAlgo::Nucleo] {
        let cfg = SearchConfig {
            matcher_algo,
            case_sensitive: true,
            ..SearchConfig::default()
        };
        // Above `PARALLEL_SEARCH_THRESHOLD` under `cfg(test)`, so the parallel nucleo path
        // is covered too - it builds its own per-chunk matcher.
        let candidates = build_index(["ABC", "abc", "aBc", "xyz", "ABc"], &PlainBackend, &cfg);

        assert_eq!(
            search("abc", &candidates, &PlainBackend, &cfg)
                .iter()
                .map(|result| result.display.as_str())
                .collect::<Vec<_>>(),
            vec!["abc"],
            "{matcher_algo:?}"
        );
    }
}

/// The other half of the policy: case-insensitive nucleo search must keep reaching every
/// spelling, which is what stayed byte-identical to v0.1.11.
#[test]
fn nucleo_case_insensitive_search_still_matches_every_spelling() {
    for matcher_algo in [MatcherAlgo::FzfV2, MatcherAlgo::Nucleo] {
        let cfg = SearchConfig {
            matcher_algo,
            ..SearchConfig::default()
        };
        let candidates = build_index(["ABC", "abc", "aBc", "xyz", "ABc"], &PlainBackend, &cfg);

        let mut displays = search("abc", &candidates, &PlainBackend, &cfg)
            .iter()
            .map(|result| result.display.clone())
            .collect::<Vec<_>>();
        displays.sort();
        assert_eq!(displays, ["ABC", "ABc", "aBc", "abc"], "{matcher_algo:?}");
    }
}

/// An uppercase query is what smart case turns into `case_sensitive: true`, and it is also
/// the input that made nucleo panic from inside `fuzzy_optimal.rs`, so pin both policies.
#[test]
fn nucleo_search_handles_an_uppercase_query_under_both_case_policies() {
    for matcher_algo in [MatcherAlgo::FzfV2, MatcherAlgo::Nucleo] {
        for case_sensitive in [false, true] {
            let cfg = SearchConfig {
                matcher_algo,
                case_sensitive,
                ..SearchConfig::default()
            };
            let candidates = build_index(
                [
                    "lib/ReadMe1.md",
                    "lib/readme2.md",
                    "lib/README3.md",
                    "x",
                    "y",
                ],
                &PlainBackend,
                &cfg,
            );

            let mut displays = search("ReadMe", &candidates, &PlainBackend, &cfg)
                .iter()
                .map(|result| result.display.clone())
                .collect::<Vec<_>>();
            displays.sort();

            let expected: &[&str] = if case_sensitive {
                &["lib/ReadMe1.md"]
            } else {
                &["lib/README3.md", "lib/ReadMe1.md", "lib/readme2.md"]
            };
            assert_eq!(displays, expected, "{matcher_algo:?} {case_sensitive}");
        }
    }
}

/// The extended path builds its matcher through the same `matcher_for_config`, and its fuzzy
/// terms are the ones that reach nucleo (exact terms fold in `fzf_query`).
#[test]
fn nucleo_case_sensitive_extended_search_rejects_a_differently_cased_candidate() {
    for matcher_algo in [MatcherAlgo::FzfV2, MatcherAlgo::Nucleo] {
        let cfg = SearchConfig {
            matcher_algo,
            case_sensitive: true,
            extended: true,
            ..SearchConfig::default()
        };
        let candidates = build_index(["ABC", "abc", "aBc", "xyz", "ABc"], &PlainBackend, &cfg);

        assert_eq!(
            search("abc | nope", &candidates, &PlainBackend, &cfg)
                .iter()
                .map(|result| result.display.as_str())
                .collect::<Vec<_>>(),
            vec!["abc"],
            "{matcher_algo:?}"
        );
    }
}

#[test]
fn tiebreak_length_prefers_shorter_display_for_equal_scores() {
    let cfg = SearchConfig {
        disabled: true,
        tiebreaks: vec![Tiebreak::Length],
        ..SearchConfig::default()
    };
    let candidates = build_index(["aaaa", "aa"], &PlainBackend, &cfg);
    let results = search("", &candidates, &PlainBackend, &cfg);

    assert_eq!(results[0].display, "aa");
}

#[test]
fn tiebreak_index_prefers_input_order() {
    let cfg = SearchConfig {
        disabled: true,
        tiebreaks: vec![Tiebreak::Index],
        ..SearchConfig::default()
    };
    let candidates = build_index(["aaaa", "aa"], &PlainBackend, &cfg);
    let results = search("", &candidates, &PlainBackend, &cfg);

    assert_eq!(results[0].display, "aaaa");
}

#[test]
fn tiebreak_pathname_prefers_match_in_basename() {
    let cfg = SearchConfig {
        disabled: true,
        tiebreaks: vec![Tiebreak::Pathname],
        ..SearchConfig::default()
    };
    let candidates = build_index(["foo/file.txt", "src/foo.txt"], &PlainBackend, &cfg);
    let results = search("foo", &candidates, &PlainBackend, &cfg);

    assert_eq!(results[0].display, "src/foo.txt");
}

#[test]
fn no_sort_preserves_input_order_after_filtering() {
    let cfg = SearchConfig {
        no_sort: true,
        limit: 2,
        ..SearchConfig::default()
    };
    let candidates = build_index(["zzabc", "abc", "xxabc"], &PlainBackend, &cfg);
    let results = search("abc", &candidates, &PlainBackend, &cfg);

    assert_eq!(
        results
            .iter()
            .map(|result| result.display.as_str())
            .collect::<Vec<_>>(),
        vec!["zzabc", "abc"]
    );
}

#[test]
fn parallel_search_matches_sequential_matcher_results() {
    let cfg = SearchConfig {
        limit: 4,
        ..SearchConfig::default()
    };
    let candidates = build_index(
        [
            "zzabc",
            "abc",
            "src/abc.txt",
            "abc-long-name",
            "a/b/c",
            "prefix-abc",
        ],
        &PlainBackend,
        &cfg,
    );
    let parallel = search("abc", &candidates, &PlainBackend, &cfg);
    let mut matcher = GreedyMatcher::default();
    let sequential = search_with_stats("abc", &candidates, &PlainBackend, &mut matcher, &cfg).0;

    assert_eq!(parallel, sequential);
}

#[test]
fn parallel_nucleo_search_matches_sequential_matcher_results() {
    let cfg = SearchConfig {
        limit: 4,
        matcher_algo: MatcherAlgo::Nucleo,
        ..SearchConfig::default()
    };
    let candidates = build_index(
        [
            "zzabc",
            "abc",
            "src/abc.txt",
            "abc-long-name",
            "a/b/c",
            "prefix-abc",
        ],
        &PlainBackend,
        &cfg,
    );
    let parallel = search("abc", &candidates, &PlainBackend, &cfg);
    let mut matcher = NucleoMatcher::default();
    let sequential = search_with_stats("abc", &candidates, &PlainBackend, &mut matcher, &cfg).0;

    assert_eq!(parallel, sequential);
}

#[test]
fn parallel_nucleo_no_sort_multi_chunk_matches_sequential_matcher_results() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("test thread pool");
    pool.install(|| {
        let cfg = SearchConfig {
            limit: 5,
            matcher_algo: MatcherAlgo::Nucleo,
            no_sort: true,
            ..SearchConfig::default()
        };
        let candidates = build_index(
            [
                "zzabc",
                "nope",
                "abc",
                "xxabc",
                "a/b/c",
                "prefix-abc",
                "zzz",
            ],
            &PlainBackend,
            &cfg,
        );
        let (parallel, parallel_stats) =
            search_nucleo_with_stats("abc", &candidates, &PlainBackend, &cfg);
        let mut matcher = NucleoMatcher::default();
        let (sequential, sequential_stats) =
            search_with_stats("abc", &candidates, &PlainBackend, &mut matcher, &cfg);

        assert_eq!(parallel, sequential);
        assert_eq!(
            parallel
                .iter()
                .map(|result| result.display.as_str())
                .collect::<Vec<_>>(),
            vec!["zzabc", "abc", "xxabc", "a/b/c", "prefix-abc"]
        );
        assert_eq!(
            parallel_stats.candidates_seen,
            sequential_stats.candidates_seen
        );
        assert_eq!(parallel_stats.keys_seen, sequential_stats.keys_seen);
        assert_eq!(parallel_stats.fuzzy_calls, sequential_stats.fuzzy_calls);
        assert_eq!(parallel_stats.variants_seen, sequential_stats.variants_seen);
    });
}

#[test]
fn parallel_extended_search_matches_sequential_matcher_results() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .expect("test thread pool");
    pool.install(|| {
        let cfg = SearchConfig {
            limit: 4,
            ..SearchConfig::default()
        };
        let candidates = build_index(
            [
                "src/main.rs",
                "src/test_main.rs",
                "docs/main.md",
                "lib/main.rs",
                "src/util.rs",
                "src/^main$.rs",
            ],
            &PlainBackend,
            &cfg,
        );
        let query = "main !test ^src";
        let (parallel, parallel_stats) =
            search_extended_auto(query, &candidates, &PlainBackend, &cfg);
        let mut matcher = GreedyMatcher::default();
        let (sequential, sequential_stats) =
            search_with_stats(query, &candidates, &PlainBackend, &mut matcher, &cfg);

        assert_eq!(parallel, sequential);
        assert_eq!(
            parallel
                .iter()
                .map(|result| result.display.as_str())
                .collect::<Vec<_>>(),
            vec!["src/main.rs", "src/^main$.rs"]
        );
        assert_eq!(
            parallel_stats.candidates_seen,
            sequential_stats.candidates_seen
        );
        assert_eq!(parallel_stats.keys_seen, sequential_stats.keys_seen);
        assert_eq!(parallel_stats.fuzzy_calls, sequential_stats.fuzzy_calls);
        assert_eq!(parallel_stats.variants_seen, sequential_stats.variants_seen);
    });
}

#[test]
fn parallel_fzf_v2_search_matches_sequential_nucleo_results() {
    let cfg = SearchConfig {
        limit: 4,
        matcher_algo: MatcherAlgo::FzfV2,
        ..SearchConfig::default()
    };
    let candidates = build_index(
        [
            "zzabc",
            "abc",
            "src/abc.txt",
            "abc-long-name",
            "a/b/c",
            "prefix-abc",
        ],
        &PlainBackend,
        &cfg,
    );
    let parallel = search("abc", &candidates, &PlainBackend, &cfg);
    let mut matcher = NucleoMatcher::default();
    let sequential = search_with_stats("abc", &candidates, &PlainBackend, &mut matcher, &cfg).0;

    assert_eq!(parallel, sequential);
}

#[test]
fn streaming_top_results_match_full_sorted_results() {
    let limited_cfg = SearchConfig {
        limit: 3,
        ..SearchConfig::default()
    };
    let full_cfg = SearchConfig {
        limit: usize::MAX,
        ..SearchConfig::default()
    };
    let candidates = build_index(
        [
            "zzabc",
            "abc",
            "src/abc.txt",
            "abc-long-name",
            "a/b/c",
            "prefix-abc",
        ],
        &PlainBackend,
        &full_cfg,
    );

    let limited = search("abc", &candidates, &PlainBackend, &limited_cfg);
    let full = search("abc", &candidates, &PlainBackend, &full_cfg);

    assert_eq!(limited, full[..3]);
}

/// Candidates that all score identically for the query `abc`, ordered so that every
/// candidate after the first wins the `Length` tiebreak against everything already kept.
const TIED_SCORE_CANDIDATES: [&str; 5] = [
    "abc-zzzzzzzz",
    "abc-zzzzzzz",
    "abc-zzzzzz",
    "abc-zzzzz",
    "abc-zzzz",
];

#[test]
fn tie_on_score_evicts_the_tiebreak_loser() {
    let limited_cfg = SearchConfig {
        limit: 2,
        tiebreaks: vec![Tiebreak::Length, Tiebreak::Index],
        ..SearchConfig::default()
    };
    let full_cfg = SearchConfig {
        limit: usize::MAX,
        ..limited_cfg.clone()
    };
    let candidates = build_index(TIED_SCORE_CANDIDATES, &PlainBackend, &full_cfg);

    // Guard the premise: if scoring ever stops tying here the test would silently stop
    // exercising tiebreak-aware eviction.
    let mut matcher = GreedyMatcher::default();
    let full = search_with_stats("abc", &candidates, &PlainBackend, &mut matcher, &full_cfg).0;
    assert_eq!(full.len(), TIED_SCORE_CANDIDATES.len());
    assert!(full.iter().all(|result| result.score == full[0].score));

    // `search_with_stats` stays sequential, so this really goes through the streaming heap
    // with a limit smaller than the number of tied candidates.
    let limited = search_with_stats(
        "abc",
        &candidates,
        &PlainBackend,
        &mut matcher,
        &limited_cfg,
    )
    .0;
    assert_eq!(
        limited
            .iter()
            .map(|result| result.display.as_str())
            .collect::<Vec<_>>(),
        vec!["abc-zzzz", "abc-zzzzz"]
    );
    assert_eq!(limited, full[..2]);
}

#[test]
fn streaming_top_results_match_full_sort_across_tiebreaks() {
    let corpus = [
        "src/abc.rs",
        "src/abc/mod.rs",
        "abc",
        "zzabc",
        "prefix-abc-suffix",
        "a/b/c/abc.txt",
        "docs/abc.md",
        "abc abc",
        "the abc chunk here",
        "ABC",
        "abcabcabc",
        "src/main.rs",
        "xabcx",
        "abc-zzzz",
        "abc-zzzzz",
        "abc-zzzzzz",
        "nested/deep/path/abc",
        "abc.txt",
    ];
    let tiebreak_sets = [
        vec![Tiebreak::Length, Tiebreak::Index],
        vec![Tiebreak::Index],
        vec![Tiebreak::Begin, Tiebreak::Length],
        vec![Tiebreak::End],
        vec![Tiebreak::Chunk, Tiebreak::Length],
        vec![Tiebreak::Pathname, Tiebreak::Length, Tiebreak::Index],
        // Every criterion at once, which fills all `RANK_KEY_COUNT` rank key slots: adding a
        // `Tiebreak` variant without growing that constant makes this case panic.
        vec![
            Tiebreak::Pathname,
            Tiebreak::Chunk,
            Tiebreak::Begin,
            Tiebreak::End,
            Tiebreak::Length,
            Tiebreak::Index,
        ],
    ];

    for tiebreaks in tiebreak_sets {
        let full_cfg = SearchConfig {
            limit: usize::MAX,
            tiebreaks: tiebreaks.clone(),
            ..SearchConfig::default()
        };
        let candidates = build_index(corpus, &PlainBackend, &full_cfg);
        let mut matcher = GreedyMatcher::default();
        let full = search_with_stats("abc", &candidates, &PlainBackend, &mut matcher, &full_cfg).0;

        for limit in [1, 2, 3, 5, 9, corpus.len()] {
            let limited_cfg = SearchConfig {
                limit,
                ..full_cfg.clone()
            };
            let expected = &full[..limit.min(full.len())];

            let limited = search_with_stats(
                "abc",
                &candidates,
                &PlainBackend,
                &mut matcher,
                &limited_cfg,
            )
            .0;
            assert_eq!(limited, expected, "sequential {tiebreaks:?} limit {limit}");

            let parallel = search("abc", &candidates, &PlainBackend, &limited_cfg);
            assert_eq!(parallel, expected, "parallel {tiebreaks:?} limit {limit}");
        }
    }
}
