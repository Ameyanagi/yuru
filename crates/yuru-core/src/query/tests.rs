use crate::{KeyKind, QueryVariantKind, SearchConfig, SearchKey};

use super::*;

#[test]
fn case_fold_only_normalized_key_is_blocked_only_for_case_insensitive_search() {
    let key = SearchKey::normalized("readme.md").with_case_fold_only(true);
    let insensitive = SearchConfig::default();
    let sensitive = SearchConfig {
        case_sensitive: true,
        ..SearchConfig::default()
    };

    // The scorer folds case itself, so the key repeats what the original key matches.
    assert!(key_blocked_by_config(&key, &insensitive, true));
    // Case-sensitive search blocks it too, for the opposite reason: it is lowercased.
    assert!(key_blocked_by_config(&key, &sensitive, true));
}

#[test]
fn case_fold_only_normalized_key_is_kept_when_the_scorer_does_not_fold_case() {
    let key = SearchKey::normalized("readme.md").with_case_fold_only(true);
    let config = SearchConfig::default();

    // A matcher that does not fold case reaches "README.md" only through this key.
    assert!(!key_blocked_by_config(&key, &config, false));
}

#[test]
fn normalized_key_that_does_more_than_fold_case_is_kept() {
    let key = SearchKey::normalized("abc.txt");
    let config = SearchConfig::default();

    assert!(!key_blocked_by_config(&key, &config, true));
    assert!(!key_blocked_by_config(&key, &config, false));
}

#[test]
fn normalized_key_is_blocked_when_normalization_is_disabled() {
    let key = SearchKey::normalized("abc.txt");
    let config = SearchConfig {
        normalize: false,
        ..SearchConfig::default()
    };

    assert!(key_blocked_by_config(&key, &config, true));
    assert!(key_blocked_by_config(&key, &config, false));
}

#[test]
fn plain_query_expansion_is_small() {
    let vars = PlainBackend.expand_query("Tokyo", QueryBudget::default());
    assert!(vars.iter().any(|v| v.text == "Tokyo"));
    assert!(vars.iter().any(|v| v.text == "tokyo"));
    assert!(vars.len() <= 2);
}

#[test]
fn empty_query_does_not_panic() {
    let vars = PlainBackend.expand_query("", QueryBudget::default());
    assert!(vars.len() <= 1);
}

#[test]
fn romaji_to_kana_variant_only_targets_kana_keys() {
    let variant = QueryVariant::romaji_to_kana("とうきょう");
    assert!(key_kind_allowed(&variant, KeyKind::KanaReading));
    assert!(!key_kind_allowed(&variant, KeyKind::PinyinJoined));
}

#[test]
fn kana_variant_only_targets_kana_keys() {
    let variant = QueryVariant::kana("はち");
    assert!(key_kind_allowed(&variant, KeyKind::KanaReading));
    assert!(!key_kind_allowed(&variant, KeyKind::RomajiReading));
}

#[test]
fn pinyin_initial_variant_only_targets_pinyin_initials_and_aliases() {
    let variant = QueryVariant {
        text: "bjdx".to_string(),
        kind: QueryVariantKind::Initials,
        weight: 0,
    };

    assert!(key_kind_allowed(&variant, KeyKind::PinyinInitials));
    assert!(key_kind_allowed(&variant, KeyKind::KoreanInitials));
    assert!(key_kind_allowed(&variant, KeyKind::LearnedAlias));
    assert!(!key_kind_allowed(&variant, KeyKind::KanaReading));
}

#[test]
fn dedup_preserves_same_text_when_it_adds_key_coverage() {
    let variants = dedup_and_limit_variants(
        vec![
            QueryVariant::original("bjdx"),
            QueryVariant::initials("bjdx"),
            QueryVariant::pinyin("bjdx"),
            QueryVariant::initials("bjdx"),
        ],
        8,
    );

    assert_eq!(variants.len(), 2);
    assert_eq!(variants[0].kind, QueryVariantKind::Original);
    assert_eq!(variants[1].kind, QueryVariantKind::Initials);
}
