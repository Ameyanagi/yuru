//! Japanese phonetic matching backend for Yuru.
//!
//! The backend adds kana and romaji reading keys for Japanese candidates and
//! keeps source spans so matches in generated readings can highlight the
//! original display text.

mod numeric;
/// Kanji reading helpers backed by Lindera.
pub mod reading;
/// Romaji and kana conversion helpers.
pub mod romaji;

use yuru_core::{
    base_query_variants, normalize,
    normalize::{contains_kana, katakana_to_hiragana},
    KeyBudget, LangMode, LanguageBackend, QueryBudget, QueryVariant, SearchKey, SourceSpan,
};

const ROMAJI_TO_KANA_FANOUT_LIMIT: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Controls whether Japanese kanji readings are generated.
pub enum JapaneseReadingMode {
    /// Do not generate kanji reading keys.
    None,
    /// Generate kanji reading keys with Lindera.
    Lindera,
}

#[derive(Clone, Debug)]
/// Japanese language backend for kana, romaji, and optional kanji readings.
pub struct JapaneseBackend {
    reading: JapaneseReadingMode,
}

impl JapaneseBackend {
    /// Creates a Japanese backend with the selected reading mode.
    pub fn new(reading: JapaneseReadingMode) -> Self {
        Self { reading }
    }
}

impl Default for JapaneseBackend {
    fn default() -> Self {
        Self {
            reading: JapaneseReadingMode::Lindera,
        }
    }
}

impl LanguageBackend for JapaneseBackend {
    fn mode(&self) -> LangMode {
        LangMode::Japanese
    }

    fn build_candidate_keys(&self, text: &str, budget: KeyBudget) -> Vec<SearchKey> {
        let mut keys = Vec::new();

        if contains_kana(text) {
            let (hira, source_map) = hiragana_with_source_map(text);
            push_reading_keys_with_map(&mut keys, &hira, &source_map);
        }
        if self.reading != JapaneseReadingMode::None {
            for reading in reading::kanji_reading_candidates_with_sources(text, budget.max_keys) {
                let (hira, source_map) =
                    katakana_to_hiragana_with_source_map(&reading.text, &reading.source_map);
                push_reading_keys_with_map(&mut keys, &hira, &source_map);
                if keys.len() >= budget.max_keys {
                    break;
                }
            }
        }

        keys
    }

    fn expand_query(&self, query: &str, budget: QueryBudget) -> Vec<QueryVariant> {
        let mut variants = base_query_variants(query);
        let romaji_limit = budget.max_variants.max(ROMAJI_TO_KANA_FANOUT_LIMIT);
        let normalized = normalize::normalize(query);
        if contains_kana(&normalized) {
            variants.push(QueryVariant::kana(katakana_to_hiragana(&normalized)));
        }
        if let Some(numeric_romaji) = numeric::numeric_romaji_query(query) {
            for kana in romaji::romaji_to_kana_candidates(&numeric_romaji, romaji_limit) {
                variants.push(QueryVariant::romaji_to_kana(kana));
            }
        }
        for kana in romaji::romaji_to_kana_candidates(query, romaji_limit) {
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
            start_char: char_index,
            end_char: char_index + 1,
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
        let normalized = normalize::normalize(&ch.to_string());
        let hira = katakana_to_hiragana(&normalized);
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
        let backend = JapaneseBackend::default();
        let cand = build_candidate(0, "東京駅", &backend, &SearchConfig::default());
        assert!(!cand
            .keys
            .iter()
            .any(|k| matches!(k.kind, KeyKind::PinyinFull | KeyKind::PinyinJoined)));
    }

    #[test]
    fn japanese_mode_builds_kana_keys_for_katakana() {
        let backend = JapaneseBackend::default();
        let cand = build_candidate(0, "カメラ.txt", &backend, &SearchConfig::default());
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
        assert_eq!(
            map[0],
            Some(SourceSpan {
                start_char: 0,
                end_char: 1
            })
        );
        assert_eq!(
            map[2],
            Some(SourceSpan {
                start_char: 1,
                end_char: 2
            })
        );
    }

    #[test]
    fn japanese_mode_builds_lindera_reading_keys_for_common_kanji() {
        let cand = build_candidate(
            0,
            "tests/日本語.txt",
            &JapaneseBackend::default(),
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
    fn japanese_mode_folds_prolonged_sound_in_lindera_reading_keys() {
        let cand = build_candidate(
            0,
            "2025年8月　ハッピースマイル写真展示室コード.pdf",
            &JapaneseBackend::default(),
            &SearchConfig::default(),
        );

        assert!(cand.keys.iter().any(|key| {
            key.kind == KeyKind::KanaReading && key.text.contains("はっぴ-すまいるしゃしんてんじ")
        }));
    }

    #[test]
    fn japanese_mode_uses_numeric_context_for_date_reading_keys() {
        let cand = build_candidate(
            0,
            "2025年8月　ハッピースマイル写真展示室コード.pdf",
            &JapaneseBackend::default(),
            &SearchConfig::default(),
        );

        assert!(cand.keys.iter().any(|key| {
            key.kind == KeyKind::RomajiReading
                && key.text.contains("nisennijuugonenhachigatsu")
                && key.text.contains("happi-sumairu")
        }));
        assert!(cand.keys.iter().any(|key| {
            key.kind == KeyKind::RomajiReading
                && key.text.contains("2025nen8gatsu")
                && key.text.contains("happi-sumairu")
        }));
    }

    #[test]
    fn japanese_mode_maps_kanji_reading_to_source_span() {
        let cand = build_candidate(
            0,
            "tests/日本人の.txt",
            &JapaneseBackend::default(),
            &SearchConfig::default(),
        );
        let key = cand
            .keys
            .iter()
            .find(|key| {
                key.kind == KeyKind::RomajiReading
                    && (key.text.contains("nihonjinno") || key.text.contains("nipponjinno"))
            })
            .unwrap();
        let ni_index = key.text.chars().position(|ch| ch == 'n').unwrap();
        let no_index = key.text.rfind("no").unwrap();
        let no_char_index = key.text[..no_index].chars().count();
        let source_map = key.source_map.as_ref().unwrap();

        assert_eq!(
            source_map[ni_index],
            Some(SourceSpan {
                start_char: 6,
                end_char: 9
            })
        );
        assert_eq!(
            source_map[no_char_index],
            Some(SourceSpan {
                start_char: 9,
                end_char: 10
            })
        );
    }

    #[test]
    fn japanese_reading_none_skips_lindera_kanji_readings() {
        let backend = JapaneseBackend::new(JapaneseReadingMode::None);
        let cand = build_candidate(0, "tests/日本語.txt", &backend, &SearchConfig::default());

        assert!(!cand
            .keys
            .iter()
            .any(|k| k.kind == KeyKind::RomajiReading && k.text.contains("nihongo")));
    }
}
