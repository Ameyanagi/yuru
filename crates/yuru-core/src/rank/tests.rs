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
    let mut matcher = GreedyMatcher;

    let (_results, stats) =
        search_with_stats("tokyo", &candidates, &PlainBackend, &mut matcher, &cfg);

    assert_eq!(stats.reading_generation_calls, 0);
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
    let mut matcher = GreedyMatcher;
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
