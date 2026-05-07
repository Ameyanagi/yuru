use std::collections::HashSet;

use crate::{query::PlainBackend, KeyKind};

use super::*;

#[test]
fn plain_mode_only_original_and_normalized() {
    let cand = build_candidate(0, "東京駅", &PlainBackend, &SearchConfig::default());

    assert!(cand.keys.iter().any(|k| k.kind == KeyKind::Original));
    assert!(cand.keys.iter().any(|k| k.kind == KeyKind::Normalized));
    assert!(!cand
        .keys
        .iter()
        .any(|k| matches!(k.kind, KeyKind::KanaReading | KeyKind::RomajiReading)));
}

#[test]
fn original_key_is_always_present() {
    let cand = build_candidate(0, "README.md", &PlainBackend, &SearchConfig::default());
    assert!(cand.keys.iter().any(|k| k.kind == KeyKind::Original));
}

#[test]
fn named_constructors_use_default_weights() {
    let key = SearchKey::learned_alias("nihonbashi");

    assert_eq!(key.kind, KeyKind::LearnedAlias);
    assert_eq!(key.weight, SearchKey::default_weight(KeyKind::LearnedAlias));
}

#[test]
fn search_keys_are_deduped_and_capped() {
    let cfg = SearchConfig {
        max_search_keys_per_candidate: 4,
        ..SearchConfig::default()
    };
    let keys = vec![
        SearchKey::original("a"),
        SearchKey::normalized("a"),
        SearchKey::normalized("a"),
        SearchKey::learned_alias("b"),
        SearchKey::learned_alias("c"),
        SearchKey::learned_alias("d"),
    ];

    let out = dedup_and_limit_keys(keys, &cfg);

    assert!(out.len() <= 4);
    assert_eq!(
        out.len(),
        out.iter()
            .map(|k| (k.kind, k.text.as_str()))
            .collect::<HashSet<_>>()
            .len()
    );
}

#[test]
fn parallel_index_preserves_input_order_and_ids() {
    let cfg = SearchConfig::default();
    let cand = build_index(["one", "two", "three", "four"], &PlainBackend, &cfg);

    assert_eq!(
        cand.iter()
            .map(|candidate| (candidate.id, candidate.display.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "one"), (1, "two"), (2, "three"), (3, "four")]
    );
}
