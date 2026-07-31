use pinyin::{ToPinyin, ToPinyinMulti};
use yuru_core::{MappedTextBuilder, SourceSpan};

use crate::ChinesePolyphoneMode;

const MAX_COMMON_READINGS_PER_CHAR: usize = 3;

#[derive(Clone, Debug, Eq, PartialEq)]
/// A generated pinyin search key and its source map.
pub struct PinyinKey {
    /// Generated key text.
    pub text: String,
    /// Source span for each generated character, when known.
    pub source_map: Vec<Option<SourceSpan>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SyllableAlternatives {
    readings: Vec<String>,
    source: SourceSpan,
}

/// Builds pinyin search keys for `text`, capped at `max`.
pub fn build_pinyin_keys(text: &str, max: usize) -> Vec<String> {
    build_pinyin_keys_with_sources(text, max)
        .into_iter()
        .map(|key| key.text)
        .collect()
}

/// Builds pinyin search keys with source maps, capped at `max`.
pub fn build_pinyin_keys_with_sources(text: &str, max: usize) -> Vec<PinyinKey> {
    build_pinyin_keys_with_sources_for_mode(text, max, ChinesePolyphoneMode::None)
}

/// Builds pinyin search keys with source maps using the selected polyphone mode.
pub fn build_pinyin_keys_with_sources_for_mode(
    text: &str,
    max: usize,
    polyphone: ChinesePolyphoneMode,
) -> Vec<PinyinKey> {
    build_pinyin_keys_with_sources_for_mode_and_budget(text, max, usize::MAX, polyphone)
}

/// Builds pinyin keys while enforcing text and source-map budgets during
/// extraction and construction.
pub fn build_pinyin_keys_with_sources_for_mode_and_budget(
    text: &str,
    max: usize,
    max_bytes: usize,
    polyphone: ChinesePolyphoneMode,
) -> Vec<PinyinKey> {
    if text.is_empty() || max == 0 || max_bytes == 0 {
        return Vec::new();
    }

    let Some(alternatives) = extract_syllable_alternatives(text, polyphone, max_bytes) else {
        return Vec::new();
    };
    if alternatives.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let primary = primary_syllables(&alternatives);
    push_sequence_keys(&mut out, &primary, max, max_bytes);

    if !matches!(polyphone, ChinesePolyphoneMode::None) {
        push_common_polyphone_keys(&mut out, &alternatives, max, max_bytes);
    }

    out
}

fn extract_syllable_alternatives(
    text: &str,
    polyphone: ChinesePolyphoneMode,
    max_syllables: usize,
) -> Option<Vec<SyllableAlternatives>> {
    let mut out = Vec::new();
    let mut chars = text.chars().enumerate().peekable();

    while let Some((index, ch)) = chars.next() {
        if ch == '重' && chars.peek().is_some_and(|(_, next)| *next == '庆') {
            if out.len().saturating_add(2) > max_syllables {
                return None;
            }
            let _ = chars.next().expect("peeked Chongqing suffix");
            out.push(SyllableAlternatives {
                readings: vec!["chong".to_string()],
                source: SourceSpan {
                    start_char: index,
                    end_char: index + 1,
                },
            });
            out.push(SyllableAlternatives {
                readings: vec!["qing".to_string()],
                source: SourceSpan {
                    start_char: index + 1,
                    end_char: index + 2,
                },
            });
            continue;
        }

        let readings = match polyphone {
            ChinesePolyphoneMode::None => primary_reading(ch).into_iter().collect(),
            ChinesePolyphoneMode::Common | ChinesePolyphoneMode::Phrase => common_readings(ch),
        };
        if !readings.is_empty() {
            if out.len() >= max_syllables {
                return None;
            }
            out.push(SyllableAlternatives {
                readings,
                source: SourceSpan {
                    start_char: index,
                    end_char: index + 1,
                },
            });
        }
    }

    Some(out)
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
    max_bytes: usize,
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
                push_sequence_keys(out, &syllables, max, max_bytes);
                syllables[syllable_index].0 = alternative.readings[0].clone();
                if out.len() >= max {
                    return;
                }
            }
        }
    }
}

fn push_sequence_keys(
    out: &mut Vec<PinyinKey>,
    syllables: &[(String, SourceSpan)],
    max: usize,
    max_bytes: usize,
) {
    let mut remaining = remaining_bytes(out, max_bytes);
    if let Some(key) = full_pinyin_key(syllables, remaining) {
        push_unique(out, key, max);
    }
    remaining = remaining_bytes(out, max_bytes);
    if let Some(key) = joined_pinyin_key(syllables, remaining) {
        push_unique(out, key, max);
    }
    remaining = remaining_bytes(out, max_bytes);
    if let Some(key) = initials_pinyin_key(syllables, remaining) {
        push_unique(out, key, max);
    }
}

fn full_pinyin_key(syllables: &[(String, SourceSpan)], max_bytes: usize) -> Option<PinyinKey> {
    let mut mapped = MappedTextBuilder::new();
    let mut text_bytes = 0usize;
    let mut mapped_chars = 0usize;

    for (index, (syllable, source)) in syllables.iter().enumerate() {
        let separator = usize::from(index > 0);
        let syllable_chars = syllable.chars().count();
        if text_bytes
            .saturating_add(separator)
            .saturating_add(syllable.len())
            > max_bytes
            || mapped_chars
                .saturating_add(separator)
                .saturating_add(syllable_chars)
                > max_bytes
        {
            return None;
        }
        if index > 0 {
            mapped.push_unmapped_char(' ');
        }
        mapped.push_str(syllable, Some(*source));
        text_bytes += separator + syllable.len();
        mapped_chars += separator + syllable_chars;
    }

    let mapped = mapped.finish();
    Some(PinyinKey {
        text: mapped.text,
        source_map: mapped.source_map,
    })
}

fn joined_pinyin_key(syllables: &[(String, SourceSpan)], max_bytes: usize) -> Option<PinyinKey> {
    let mut mapped = MappedTextBuilder::new();
    let mut text_bytes = 0usize;
    let mut mapped_chars = 0usize;

    for (syllable, source) in syllables {
        text_bytes = text_bytes.saturating_add(syllable.len());
        mapped_chars = mapped_chars.saturating_add(syllable.chars().count());
        if text_bytes > max_bytes || mapped_chars > max_bytes {
            return None;
        }
        mapped.push_str(syllable, Some(*source));
    }

    let mapped = mapped.finish();
    Some(PinyinKey {
        text: mapped.text,
        source_map: mapped.source_map,
    })
}

fn initials_pinyin_key(syllables: &[(String, SourceSpan)], max_bytes: usize) -> Option<PinyinKey> {
    let mut mapped = MappedTextBuilder::new();
    let mut mapped_chars = 0usize;

    for (syllable, source) in syllables {
        if let Some(initial) = syllable.chars().next() {
            if mapped_chars.saturating_add(1) > max_bytes
                || mapped_chars.saturating_add(initial.len_utf8()) > max_bytes
            {
                return None;
            }
            mapped.push_char(initial, Some(*source));
            mapped_chars += 1;
        }
    }

    let mapped = mapped.finish();
    Some(PinyinKey {
        text: mapped.text,
        source_map: mapped.source_map,
    })
}

fn remaining_bytes(out: &[PinyinKey], max_bytes: usize) -> usize {
    max_bytes.saturating_sub(out.iter().map(|key| key.text.len()).sum::<usize>())
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
            Some(SourceSpan {
                start_char: 0,
                end_char: 1
            })
        );
        assert_eq!(
            initials.source_map[1],
            Some(SourceSpan {
                start_char: 1,
                end_char: 2
            })
        );
        assert_eq!(
            initials.source_map[2],
            Some(SourceSpan {
                start_char: 2,
                end_char: 3
            })
        );
        assert_eq!(
            initials.source_map[3],
            Some(SourceSpan {
                start_char: 3,
                end_char: 4
            })
        );

        let full = keys
            .iter()
            .find(|key| key.text == "bei jing da xue")
            .unwrap();
        assert_eq!(full.source_map[3], None);
        assert_eq!(
            full.source_map[4],
            Some(SourceSpan {
                start_char: 1,
                end_char: 2
            })
        );
    }

    #[test]
    fn pinyin_generation_enforces_budget_during_construction() {
        let keys = build_pinyin_keys_with_sources_for_mode_and_budget(
            "北京大学",
            8,
            32,
            ChinesePolyphoneMode::Common,
        );
        assert!(keys.iter().map(|key| key.text.len()).sum::<usize>() <= 32);
        assert!(keys.iter().all(|key| key.source_map.len() <= 32));

        let oversized = "中".repeat(10_000);
        assert!(build_pinyin_keys_with_sources_for_mode_and_budget(
            &oversized,
            8,
            64,
            ChinesePolyphoneMode::Common,
        )
        .is_empty());
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
        assert_eq!(
            joined.source_map[0],
            Some(SourceSpan {
                start_char: 0,
                end_char: 1
            })
        );
        assert_eq!(
            joined.source_map[3],
            Some(SourceSpan {
                start_char: 0,
                end_char: 1
            })
        );
        assert_eq!(
            joined.source_map[4],
            Some(SourceSpan {
                start_char: 1,
                end_char: 2
            })
        );
        assert_eq!(
            joined.source_map[6],
            Some(SourceSpan {
                start_char: 1,
                end_char: 2
            })
        );
    }
}
