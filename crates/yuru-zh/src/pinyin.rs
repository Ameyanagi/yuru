use pinyin::{ToPinyin, ToPinyinMulti};
use yuru_core::SourceSpan;

use crate::ChinesePolyphoneMode;

const MAX_COMMON_READINGS_PER_CHAR: usize = 3;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinyinKey {
    pub text: String,
    pub source_map: Vec<Option<SourceSpan>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SyllableAlternatives {
    readings: Vec<String>,
    source: SourceSpan,
}

pub fn build_pinyin_keys(text: &str, max: usize) -> Vec<String> {
    build_pinyin_keys_with_sources(text, max)
        .into_iter()
        .map(|key| key.text)
        .collect()
}

pub fn build_pinyin_keys_with_sources(text: &str, max: usize) -> Vec<PinyinKey> {
    build_pinyin_keys_with_sources_for_mode(text, max, ChinesePolyphoneMode::None)
}

pub fn build_pinyin_keys_with_sources_for_mode(
    text: &str,
    max: usize,
    polyphone: ChinesePolyphoneMode,
) -> Vec<PinyinKey> {
    if text.is_empty() || max == 0 {
        return Vec::new();
    }

    let alternatives = extract_syllable_alternatives(text, polyphone);
    if alternatives.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let primary = primary_syllables(&alternatives);
    push_sequence_keys(&mut out, &primary, max);

    if !matches!(polyphone, ChinesePolyphoneMode::None) {
        push_common_polyphone_keys(&mut out, &alternatives, max);
    }

    out
}

fn extract_syllable_alternatives(
    text: &str,
    polyphone: ChinesePolyphoneMode,
) -> Vec<SyllableAlternatives> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == '重' && chars.get(index + 1) == Some(&'庆') {
            out.push(SyllableAlternatives {
                readings: vec!["chong".to_string()],
                source: SourceSpan {
                    start: index,
                    end: index + 1,
                },
            });
            out.push(SyllableAlternatives {
                readings: vec!["qing".to_string()],
                source: SourceSpan {
                    start: index + 1,
                    end: index + 2,
                },
            });
            index += 2;
            continue;
        }

        let readings = match polyphone {
            ChinesePolyphoneMode::None => primary_reading(chars[index]).into_iter().collect(),
            ChinesePolyphoneMode::Common | ChinesePolyphoneMode::Phrase => {
                common_readings(chars[index])
            }
        };
        if !readings.is_empty() {
            out.push(SyllableAlternatives {
                readings,
                source: SourceSpan {
                    start: index,
                    end: index + 1,
                },
            });
        }
        index += 1;
    }

    out
}

fn primary_reading(ch: char) -> Option<String> {
    ch.to_pinyin().map(|pinyin| pinyin.plain().to_string())
}

fn common_readings(ch: char) -> Vec<String> {
    let mut out = Vec::new();

    if let Some(primary) = primary_reading(ch) {
        push_reading(&mut out, primary, MAX_COMMON_READINGS_PER_CHAR);
    }

    if out.len() < MAX_COMMON_READINGS_PER_CHAR {
        if let Some(pinyin_multi) = ch.to_pinyin_multi() {
            for pinyin in pinyin_multi {
                push_reading(
                    &mut out,
                    pinyin.plain().to_string(),
                    MAX_COMMON_READINGS_PER_CHAR,
                );
                if out.len() >= MAX_COMMON_READINGS_PER_CHAR {
                    break;
                }
            }
        }
    }

    out
}

fn push_reading(out: &mut Vec<String>, reading: String, max: usize) {
    if out.len() < max && !out.iter().any(|existing| existing == &reading) {
        out.push(reading);
    }
}

fn primary_syllables(alternatives: &[SyllableAlternatives]) -> Vec<(String, SourceSpan)> {
    alternatives
        .iter()
        .filter_map(|alternative| {
            alternative
                .readings
                .first()
                .map(|reading| (reading.clone(), alternative.source))
        })
        .collect()
}

fn push_common_polyphone_keys(
    out: &mut Vec<PinyinKey>,
    alternatives: &[SyllableAlternatives],
    max: usize,
) {
    let mut syllables = primary_syllables(alternatives);
    let max_readings = alternatives
        .iter()
        .map(|alternative| alternative.readings.len())
        .max()
        .unwrap_or(0);

    for reading_index in 1..max_readings {
        for (syllable_index, alternative) in alternatives.iter().enumerate() {
            if let Some(reading) = alternative.readings.get(reading_index) {
                syllables[syllable_index].0 = reading.clone();
                push_sequence_keys(out, &syllables, max);
                syllables[syllable_index].0 = alternative.readings[0].clone();
                if out.len() >= max {
                    return;
                }
            }
        }
    }
}

fn push_sequence_keys(out: &mut Vec<PinyinKey>, syllables: &[(String, SourceSpan)], max: usize) {
    push_unique(out, full_pinyin_key(syllables), max);
    push_unique(out, joined_pinyin_key(syllables), max);
    push_unique(out, initials_pinyin_key(syllables), max);
}

fn full_pinyin_key(syllables: &[(String, SourceSpan)]) -> PinyinKey {
    let mut text = String::new();
    let mut source_map = Vec::new();

    for (index, (syllable, source)) in syllables.iter().enumerate() {
        if index > 0 {
            text.push(' ');
            source_map.push(None);
        }
        text.push_str(syllable);
        source_map.extend(syllable.chars().map(|_| Some(*source)));
    }

    PinyinKey { text, source_map }
}

fn joined_pinyin_key(syllables: &[(String, SourceSpan)]) -> PinyinKey {
    let mut text = String::new();
    let mut source_map = Vec::new();

    for (syllable, source) in syllables {
        text.push_str(syllable);
        source_map.extend(syllable.chars().map(|_| Some(*source)));
    }

    PinyinKey { text, source_map }
}

fn initials_pinyin_key(syllables: &[(String, SourceSpan)]) -> PinyinKey {
    let mut text = String::new();
    let mut source_map = Vec::new();

    for (syllable, source) in syllables {
        if let Some(initial) = syllable.chars().next() {
            text.push(initial);
            source_map.push(Some(*source));
        }
    }

    PinyinKey { text, source_map }
}

fn push_unique(out: &mut Vec<PinyinKey>, value: PinyinKey, max: usize) {
    if out.len() < max && !out.iter().any(|key| key.text == value.text) {
        out.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinyin_beijing_university_keys() {
        let keys = build_pinyin_keys("北京大学", 8);
        assert!(keys.contains(&"bei jing da xue".to_string()));
        assert!(keys.contains(&"beijingdaxue".to_string()));
        assert!(keys.contains(&"bjdx".to_string()));
    }

    #[test]
    fn pinyin_chongqing_expected_common_reading() {
        let keys = build_pinyin_keys("重庆", 8);
        assert!(keys
            .iter()
            .any(|key| key.contains("chongqing") || key.contains("chong qing")));
    }

    #[test]
    fn pinyin_variants_are_capped() {
        let keys = build_pinyin_keys("重庆银行重庆分行", 4);
        assert!(keys.len() <= 4);
    }

    #[test]
    fn pinyin_empty_input_is_empty() {
        let keys = build_pinyin_keys("", 8);
        assert!(keys.is_empty());
    }

    #[test]
    fn pinyin_keys_include_source_maps() {
        let keys = build_pinyin_keys_with_sources("北京大学", 8);
        let initials = keys.iter().find(|key| key.text == "bjdx").unwrap();

        assert_eq!(initials.source_map.len(), 4);
        assert_eq!(
            initials.source_map[0],
            Some(SourceSpan { start: 0, end: 1 })
        );
        assert_eq!(
            initials.source_map[1],
            Some(SourceSpan { start: 1, end: 2 })
        );
        assert_eq!(
            initials.source_map[2],
            Some(SourceSpan { start: 2, end: 3 })
        );
        assert_eq!(
            initials.source_map[3],
            Some(SourceSpan { start: 3, end: 4 })
        );

        let full = keys
            .iter()
            .find(|key| key.text == "bei jing da xue")
            .unwrap();
        assert_eq!(full.source_map[3], None);
        assert_eq!(full.source_map[4], Some(SourceSpan { start: 1, end: 2 }));
    }

    #[test]
    fn polyphone_none_uses_single_reading() {
        let keys = build_pinyin_keys_with_sources_for_mode("还没", 8, ChinesePolyphoneMode::None);
        let texts: Vec<_> = keys.iter().map(|key| key.text.as_str()).collect();

        assert!(texts.contains(&"hai mei"));
        assert!(texts.contains(&"haimei"));
        assert!(!texts.contains(&"huan mei"));
        assert!(!texts.contains(&"huanmei"));
    }

    #[test]
    fn polyphone_common_adds_capped_alternate_readings() {
        let keys = build_pinyin_keys_with_sources_for_mode("还没", 8, ChinesePolyphoneMode::Common);
        let texts: Vec<_> = keys.iter().map(|key| key.text.as_str()).collect();

        assert!(texts.contains(&"hai mei"));
        assert!(texts.contains(&"haimei"));
        assert!(texts.contains(&"huan mei"));
        assert!(texts.contains(&"huanmei"));
        assert!(texts.contains(&"hai mo"));
        assert!(texts.contains(&"haimo"));
        assert!(keys.len() <= 8);
    }

    #[test]
    fn polyphone_phrase_matches_common_for_now() {
        let common =
            build_pinyin_keys_with_sources_for_mode("还没", 8, ChinesePolyphoneMode::Common);
        let phrase =
            build_pinyin_keys_with_sources_for_mode("还没", 8, ChinesePolyphoneMode::Phrase);

        assert_eq!(common, phrase);
    }

    #[test]
    fn polyphone_common_source_maps_alternate_joined_key() {
        let keys = build_pinyin_keys_with_sources_for_mode("还没", 8, ChinesePolyphoneMode::Common);
        let joined = keys.iter().find(|key| key.text == "huanmei").unwrap();

        assert_eq!(joined.source_map.len(), 7);
        assert_eq!(joined.source_map[0], Some(SourceSpan { start: 0, end: 1 }));
        assert_eq!(joined.source_map[3], Some(SourceSpan { start: 0, end: 1 }));
        assert_eq!(joined.source_map[4], Some(SourceSpan { start: 1, end: 2 }));
        assert_eq!(joined.source_map[6], Some(SourceSpan { start: 1, end: 2 }));
    }
}
