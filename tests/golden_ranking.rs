use yuru_core::{build_index, search, PlainBackend, SearchConfig, Tiebreak};
use yuru_ja::{JapaneseBackend, JapaneseReadingMode};
use yuru_zh::{ChineseBackend, ChinesePolyphoneMode, ChineseScriptMode};

fn top_display(
    query: &str,
    candidates: &[&str],
    backend: &dyn yuru_core::LanguageBackend,
    config: SearchConfig,
) -> String {
    let index = build_index(candidates.iter().copied(), backend, &config);
    search(query, &index, backend, &config)
        .into_iter()
        .next()
        .map(|result| result.display)
        .expect("query should produce at least one result")
}

/// Whole ranked order, not just the winner: a reordering that leaves the top result alone
/// still changes what the user sees.
fn ranked_displays(
    query: &str,
    candidates: &[&str],
    backend: &dyn yuru_core::LanguageBackend,
    config: SearchConfig,
) -> Vec<String> {
    let index = build_index(candidates.iter().copied(), backend, &config);
    search(query, &index, backend, &config)
        .into_iter()
        .map(|result| result.display)
        .collect()
}

/// Mixed-case ranking guard. These goldens carried no mixed-case data, so the whole
/// case-insensitive ordering could change without failing one of them.
///
/// The literal spelling wins on the exact-case bonus, `ReadMe.md` stays ahead of `README.md`
/// on the camelCase bonus, and nothing here depends on input order.
#[test]
fn golden_case_variants_rank_the_literal_spelling_first() {
    let config = SearchConfig::default();
    let candidates = ["readme.md", "README.md", "ReadMe.md", "readme_old.md"];
    let expected = ["readme.md", "readme_old.md", "ReadMe.md", "README.md"];

    assert_eq!(
        ranked_displays("readme", &candidates, &PlainBackend, config.clone()),
        expected
    );

    let mut reversed = candidates;
    reversed.reverse();
    assert_eq!(
        ranked_displays("readme", &reversed, &PlainBackend, config),
        expected,
        "case-variant order must not depend on input order"
    );
}

/// Case-sensitive search must be unaffected by the exact-case bonus: only the two literally
/// spelled candidates match at all, shortest first.
#[test]
fn golden_case_sensitive_search_ignores_the_exact_case_bonus() {
    let config = SearchConfig {
        case_sensitive: true,
        ..SearchConfig::default()
    };
    let candidates = ["readme.md", "README.md", "ReadMe.md", "readme_old.md"];
    let expected = ["readme.md", "readme_old.md"];

    assert_eq!(
        ranked_displays("readme", &candidates, &PlainBackend, config.clone()),
        expected
    );

    let mut reversed = candidates;
    reversed.reverse();
    assert_eq!(
        ranked_displays("readme", &reversed, &PlainBackend, config),
        expected
    );
}

/// camelCase boundary guard. Query `fb` reaches `b` through five different kinds of
/// boundary, and the five scores are all distinct, so any change to the boundary bonuses or
/// to which key case-insensitive matching scores against moves this list.
///
/// The pinned order is the bonus table read top to bottom: whitespace boundary (100) beats a
/// non-alphanumeric boundary (80) beats the exact-case bonus (75) beats a camelCase boundary
/// (70) beats an uppercase run, which offers no boundary at all because `O` -> `B` is not a
/// lowercase-to-uppercase transition. `foobar` staying ahead of `fooBar` is the deliberate
/// consequence of pricing the exact-case bonus just above the camelCase bonus, and it is what
/// 0.1.11 also produced; the difference from 0.1.11 is that `fooBar` and `FOOBAR` used to tie
/// (both scored through the lowercased key, which destroys the camelCase bonus) and their
/// order was decided by input position alone.
#[test]
fn golden_camel_case_boundary_outranks_an_uppercase_run() {
    let config = SearchConfig::default();
    let candidates = ["foobar", "fooBar", "FOOBAR", "foo_bar", "foo bar"];
    let expected = ["foo bar", "foo_bar", "foobar", "fooBar", "FOOBAR"];

    assert_eq!(
        ranked_displays("fb", &candidates, &PlainBackend, config.clone()),
        expected
    );

    let mut reversed = candidates;
    reversed.reverse();
    assert_eq!(
        ranked_displays("fb", &reversed, &PlainBackend, config),
        expected,
        "camelCase ranking must not depend on input order"
    );
}

/// All-caps acronym guard, the case a lowercased search key cannot rank at all.
///
/// For query `hs`, `HttpServer.rs` earns a camelCase boundary on `S` (`p` -> `S`) while
/// `HTTPServer.rs` earns nothing on its `S` (`P` -> `S` is uppercase to uppercase), so the
/// camel-cased spelling is the better match and ranks ahead. That is the intended order: an
/// acronym run carries no word-boundary information, and pretending it does would rank
/// `HTTPServer` as if the `S` started a word. In 0.1.11 both spellings scored 4769 through
/// the `Normalized` key and their order was whichever came first in the input.
#[test]
fn golden_all_caps_acronym_ranks_below_the_camel_case_spelling() {
    let config = SearchConfig::default();
    let candidates = [
        "HTTPServer.rs",
        "HttpServer.rs",
        "httpserver.rs",
        "http_server.rs",
    ];
    let expected = [
        "http_server.rs",
        "httpserver.rs",
        "HttpServer.rs",
        "HTTPServer.rs",
    ];

    assert_eq!(
        ranked_displays("hs", &candidates, &PlainBackend, config.clone()),
        expected
    );

    let mut reversed = candidates;
    reversed.reverse();
    assert_eq!(
        ranked_displays("hs", &reversed, &PlainBackend, config),
        expected,
        "acronym ranking must not depend on input order"
    );
}

#[test]
fn golden_japanese_university_initials() {
    let backend = JapaneseBackend::new(JapaneseReadingMode::Lindera);
    let config = SearchConfig {
        limit: 1,
        ..SearchConfig::default()
    };

    let winner = top_display(
        "tokyodai",
        &["東京電機大学", "東北大学", "東京大学"],
        &backend,
        config,
    );

    assert_eq!(winner, "東京大学");
}

#[test]
fn golden_chinese_initials_prefer_exact_phrase() {
    let backend = ChineseBackend::new(
        true,
        true,
        ChinesePolyphoneMode::Common,
        ChineseScriptMode::Auto,
    );
    let config = SearchConfig {
        limit: 1,
        ..SearchConfig::default()
    };

    let winner = top_display(
        "bjdx",
        &["北京地铁", "北京大学", "北京大厦"],
        &backend,
        config,
    );

    assert_eq!(winner, "北京大学");
}

#[test]
fn golden_mixed_cjk_latin_path() {
    let backend = JapaneseBackend::new(JapaneseReadingMode::Lindera);
    let config = SearchConfig {
        limit: 1,
        ..SearchConfig::default()
    };

    let winner = top_display(
        "tki",
        &[
            "docs/tokyo_notes.md",
            "src/京都_index.rs",
            "src/東京_index.rs",
        ],
        &backend,
        config,
    );

    assert_eq!(winner, "src/東京_index.rs");
}

#[test]
fn golden_path_scheme_prefers_basename_match() {
    let config = SearchConfig {
        limit: 1,
        disabled: true,
        tiebreaks: vec![Tiebreak::Pathname, Tiebreak::Length, Tiebreak::Index],
        ..SearchConfig::default()
    };

    let winner = top_display(
        "foo",
        &["foo/file.txt", "src/foo.txt"],
        &PlainBackend,
        config,
    );

    assert_eq!(winner, "src/foo.txt");
}

#[test]
fn golden_history_scheme_preserves_input_order_for_equal_scores() {
    let config = SearchConfig {
        limit: 1,
        disabled: true,
        tiebreaks: vec![Tiebreak::Index],
        ..SearchConfig::default()
    };

    let winner = top_display(
        "git",
        &["git checkout main", "git status", "git commit"],
        &PlainBackend,
        config,
    );

    assert_eq!(winner, "git checkout main");
}
