pub mod reading;
pub mod romaji;

use yuru_core::{
    base_query_variants, normalize,
    normalize::{contains_kana, katakana_to_hiragana},
    LangMode, LanguageBackend, QueryVariant, SearchKey, SourceSpan,
};

#[derive(Clone, Debug, Default)]
pub struct JapaneseBackend;

impl LanguageBackend for JapaneseBackend {
    fn mode(&self) -> LangMode {
        LangMode::Japanese
    }

    fn build_candidate_keys(&self, text: &str) -> Vec<SearchKey> {
        let mut keys = Vec::new();

        if contains_kana(text) {
            let (hira, source_map) = hiragana_with_source_map(text);
            push_reading_keys_with_map(&mut keys, &hira, &source_map);
        }
        for reading in reading::kanji_reading_candidates_with_sources(text, 8) {
            let (hira, source_map) =
                katakana_to_hiragana_with_source_map(&reading.text, &reading.source_map);
            push_reading_keys_with_map(&mut keys, &hira, &source_map);
        }

        keys
    }

    fn expand_query(&self, query: &str) -> Vec<QueryVariant> {
        let mut variants = base_query_variants(query);
        for kana in romaji::romaji_to_kana_candidates(query, 16) {
            variants.push(QueryVariant::romaji_to_kana(kana));
        }
        variants
    }
}

fn hiragana_with_source_map(text: &str) -> (String, Vec<Option<SourceSpan>>) {
    let mut out = String::new();
    let mut source_map = Vec::new();

    for (char_index, ch) in text.chars().enumerate() {
        let normalized = normalize::normalize(&ch.to_string());
        let hira = katakana_to_hiragana(&normalized);
        let source = Some(SourceSpan {
            start: char_index,
            end: char_index + 1,
        });
        out.push_str(&hira);
        source_map.extend(hira.chars().map(|_| source));
    }

    (out, source_map)
}

fn katakana_to_hiragana_with_source_map(
    text: &str,
    source_map: &[Option<SourceSpan>],
) -> (String, Vec<Option<SourceSpan>>) {
    let mut out = String::new();
    let mut out_map = Vec::new();

    for (index, ch) in text.chars().enumerate() {
        let hira = katakana_to_hiragana(&ch.to_string());
        let source = source_map.get(index).copied().flatten();
        out.push_str(&hira);
        out_map.extend(hira.chars().map(|_| source));
    }

    (out, out_map)
}

fn push_reading_keys_with_map(
    keys: &mut Vec<SearchKey>,
    hira: &str,
    source_map: &[Option<SourceSpan>],
) {
    keys.push(SearchKey::kana_reading(hira.to_string()).with_source_map(source_map.to_vec()));
    let (romaji, romaji_map) = romaji::kana_to_romaji_with_source_map(hira, source_map);
    if romaji != hira {
        keys.push(SearchKey::romaji_reading(romaji).with_source_map(romaji_map));
    }
}

#[cfg(test)]
mod tests {
    use yuru_core::{build_candidate, KeyKind, SearchConfig};

    use super::*;

    #[test]
    fn japanese_mode_does_not_build_pinyin_keys() {
        let cand = build_candidate(0, "東京駅", &JapaneseBackend, &SearchConfig::default());
        assert!(!cand
            .keys
            .iter()
            .any(|k| matches!(k.kind, KeyKind::PinyinFull | KeyKind::PinyinJoined)));
    }

    #[test]
    fn japanese_mode_builds_kana_keys_for_katakana() {
        let cand = build_candidate(0, "カメラ.txt", &JapaneseBackend, &SearchConfig::default());
        assert!(cand
            .keys
            .iter()
            .any(|k| k.kind == KeyKind::KanaReading && k.text.contains("かめら")));
        let key = cand
            .keys
            .iter()
            .find(|k| k.kind == KeyKind::RomajiReading && k.text.contains("kamera"))
            .unwrap();
        let map = key.source_map.as_ref().unwrap();
        assert_eq!(map[0], Some(SourceSpan { start: 0, end: 1 }));
        assert_eq!(map[2], Some(SourceSpan { start: 1, end: 2 }));
    }

    #[test]
    fn japanese_mode_builds_seed_reading_keys_for_common_kanji() {
        let cand = build_candidate(
            0,
            "tests/日本語.txt",
            &JapaneseBackend,
            &SearchConfig::default(),
        );

        assert!(cand
            .keys
            .iter()
            .any(|k| k.kind == KeyKind::KanaReading && k.text.contains("にほんご")));
        assert!(cand
            .keys
            .iter()
            .any(|k| k.kind == KeyKind::RomajiReading && k.text.contains("nihongo")));
    }

    #[test]
    fn japanese_mode_maps_kanji_reading_to_source_span() {
        let cand = build_candidate(
            0,
            "tests/日本人の.txt",
            &JapaneseBackend,
            &SearchConfig::default(),
        );
        let key = cand
            .keys
            .iter()
            .find(|key| key.kind == KeyKind::RomajiReading && key.text.contains("nihonjinno"))
            .unwrap();
        let ni_index = key.text.chars().position(|ch| ch == 'n').unwrap();
        let no_index = key.text.rfind("no").unwrap();
        let no_char_index = key.text[..no_index].chars().count();
        let source_map = key.source_map.as_ref().unwrap();

        assert_eq!(source_map[ni_index], Some(SourceSpan { start: 6, end: 9 }));
        assert_eq!(
            source_map[no_char_index],
            Some(SourceSpan { start: 9, end: 10 })
        );
    }
}
