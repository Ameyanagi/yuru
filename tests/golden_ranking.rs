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
