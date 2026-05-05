pub mod pinyin;

use yomi_core::{base_query_variants, LangMode, LanguageBackend, QueryVariant, SearchKey};

#[derive(Clone, Debug, Default)]
pub struct ChineseBackend;

impl LanguageBackend for ChineseBackend {
    fn mode(&self) -> LangMode {
        LangMode::Chinese
    }

    fn build_candidate_keys(&self, text: &str) -> Vec<SearchKey> {
        pinyin::build_pinyin_keys_with_sources(text, 8)
            .into_iter()
            .map(|key| {
                let search_key = if key.text.contains(' ') {
                    SearchKey::pinyin_full(key.text)
                } else if key.text.chars().count() <= text.chars().count() {
                    SearchKey::pinyin_initials(key.text)
                } else {
                    SearchKey::pinyin_joined(key.text)
                };
                search_key.with_source_map(key.source_map)
            })
            .collect()
    }

    fn expand_query(&self, query: &str) -> Vec<QueryVariant> {
        let mut variants = base_query_variants(query);
        let normalized = yomi_core::normalize::normalize(query);
        if normalized.chars().all(|ch| ch.is_ascii_alphabetic()) && normalized.len() > 1 {
            variants.push(QueryVariant::initials(normalized.clone()));
            variants.push(QueryVariant::pinyin(normalized));
        }
        variants
    }
}

#[cfg(test)]
mod tests {
    use yomi_core::{build_candidate, KeyKind, SearchConfig};

    use super::*;

    #[test]
    fn chinese_mode_does_not_build_japanese_reading_keys() {
        let cand = build_candidate(0, "北京大学", &ChineseBackend, &SearchConfig::default());
        assert!(!cand
            .keys
            .iter()
            .any(|k| matches!(k.kind, KeyKind::KanaReading | KeyKind::RomajiReading)));
    }

    #[test]
    fn chinese_mode_maps_initials_to_source_spans() {
        let cand = build_candidate(0, "北京大学", &ChineseBackend, &SearchConfig::default());
        let key = cand
            .keys
            .iter()
            .find(|key| key.kind == KeyKind::PinyinInitials && key.text == "bjdx")
            .unwrap();

        assert_eq!(key.source_map.as_ref().unwrap()[0].unwrap().start, 0);
        assert_eq!(key.source_map.as_ref().unwrap()[1].unwrap().start, 1);
    }
}
