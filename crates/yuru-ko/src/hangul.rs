use yuru_core::{MappedTextBuilder, SourceSpan};

const S_BASE: u32 = 0xac00;
const S_END: u32 = 0xd7a3;
const L_COUNT: usize = 19;
const V_COUNT: usize = 21;
const T_COUNT: usize = 28;
const N_COUNT: usize = V_COUNT * T_COUNT;

const CHOSEONG: [char; L_COUNT] = [
    'ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ',
    'ㅌ', 'ㅍ', 'ㅎ',
];

const INITIAL_ROMANIZATION: [&str; L_COUNT] = [
    "g", "kk", "n", "d", "tt", "r", "m", "b", "pp", "s", "ss", "", "j", "jj", "ch", "k", "t", "p",
    "h",
];

const VOWEL_ROMANIZATION: [&str; V_COUNT] = [
    "a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wae", "oe", "yo", "u", "wo", "we",
    "wi", "yu", "eu", "ui", "i",
];

const FINAL_ROMANIZATION: [&str; T_COUNT] = [
    "", "k", "k", "k", "n", "n", "n", "t", "l", "k", "m", "p", "l", "l", "p", "l", "m", "p", "p",
    "t", "t", "ng", "t", "t", "k", "t", "p", "t",
];

const INITIAL_KEYS: [&str; L_COUNT] = [
    "r", "r", "s", "e", "e", "f", "a", "q", "q", "t", "t", "d", "w", "w", "c", "z", "x", "v", "g",
];

const VOWEL_KEYS: [&str; V_COUNT] = [
    "k", "o", "i", "o", "j", "p", "u", "p", "h", "hk", "ho", "hl", "y", "n", "nj", "np", "nl", "b",
    "m", "ml", "l",
];

const FINAL_KEYS: [&str; T_COUNT] = [
    "", "r", "r", "rt", "s", "sw", "sg", "e", "f", "fr", "fa", "fq", "ft", "fx", "fv", "fg", "a",
    "q", "qt", "t", "t", "d", "w", "c", "z", "x", "v", "g",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Kind of generated Korean search key.
pub enum KoreanKeyKind {
    /// Deterministic Hangul romanization.
    Romanized,
    /// Choseong initials.
    Initials,
    /// Korean 2-set keyboard input sequence.
    Keyboard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// A generated Korean search key and its source map.
pub struct KoreanKey {
    /// Generated key text.
    pub text: String,
    /// Generated key family.
    pub kind: KoreanKeyKind,
    /// Source span for each generated character, when known.
    pub source_map: Vec<Option<SourceSpan>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HangulSyllable {
    initial: usize,
    vowel: usize,
    final_consonant: usize,
    source: SourceSpan,
}

/// Returns true when `text` contains Hangul syllables or jamo.
pub fn contains_hangul(text: &str) -> bool {
    text.chars().any(is_hangul)
}

/// Builds Korean search keys for `text`, capped at `max`.
pub fn build_korean_keys(text: &str, max: usize) -> Vec<String> {
    build_korean_keys_with_sources(text, max)
        .into_iter()
        .map(|key| key.text)
        .collect()
}

/// Builds Korean search keys with source maps, capped at `max`.
pub fn build_korean_keys_with_sources(text: &str, max: usize) -> Vec<KoreanKey> {
    build_korean_keys_with_sources_with_budget(text, max, usize::MAX)
}

/// Builds Korean search keys while enforcing text and source-map budgets
/// during construction.
pub fn build_korean_keys_with_sources_with_budget(
    text: &str,
    max: usize,
    max_bytes: usize,
) -> Vec<KoreanKey> {
    if text.is_empty() || max == 0 || max_bytes == 0 {
        return Vec::new();
    }

    let Some(syllables) = extract_syllables(text, max_bytes) else {
        return Vec::new();
    };
    if syllables.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    if let Some(key) = romanized_key(&syllables, true, remaining_bytes(&out, max_bytes)) {
        push_unique(&mut out, key, max);
    }
    if let Some(key) = romanized_key(&syllables, false, remaining_bytes(&out, max_bytes)) {
        push_unique(&mut out, key, max);
    }
    if let Some(key) = initials_key(&syllables, remaining_bytes(&out, max_bytes)) {
        push_unique(&mut out, key, max);
    }
    if let Some(key) = keyboard_key(&syllables, remaining_bytes(&out, max_bytes)) {
        push_unique(&mut out, key, max);
    }
    out
}

fn extract_syllables(text: &str, max_syllables: usize) -> Option<Vec<HangulSyllable>> {
    let mut syllables = Vec::new();
    for (index, ch) in text.chars().enumerate() {
        if let Some((initial, vowel, final_consonant)) = decompose_hangul(ch) {
            if syllables.len() >= max_syllables {
                return None;
            }
            syllables.push(HangulSyllable {
                initial,
                vowel,
                final_consonant,
                source: SourceSpan {
                    start_char: index,
                    end_char: index + 1,
                },
            });
        }
    }
    Some(syllables)
}

fn decompose_hangul(ch: char) -> Option<(usize, usize, usize)> {
    let code = ch as u32;
    if !(S_BASE..=S_END).contains(&code) {
        return None;
    }

    let syllable_index = (code - S_BASE) as usize;
    let initial = syllable_index / N_COUNT;
    let vowel = (syllable_index % N_COUNT) / T_COUNT;
    let final_consonant = syllable_index % T_COUNT;
    Some((initial, vowel, final_consonant))
}

fn is_hangul(ch: char) -> bool {
    decompose_hangul(ch).is_some()
        || ('\u{1100}'..='\u{11ff}').contains(&ch)
        || ('\u{3130}'..='\u{318f}').contains(&ch)
        || ('\u{a960}'..='\u{a97f}').contains(&ch)
        || ('\u{d7b0}'..='\u{d7ff}').contains(&ch)
}

fn romanized_key(
    syllables: &[HangulSyllable],
    spaced: bool,
    max_bytes: usize,
) -> Option<KoreanKey> {
    let mut mapped = MappedTextBuilder::new();
    let mut text_bytes = 0usize;
    let mut mapped_chars = 0usize;

    for (index, syllable) in syllables.iter().enumerate() {
        let separator = usize::from(spaced && index > 0);
        let romanized = romanized_syllable(*syllable);
        if !can_append(
            text_bytes,
            mapped_chars,
            separator + romanized.len(),
            separator + romanized.chars().count(),
            max_bytes,
        ) {
            return None;
        }
        if spaced && index > 0 {
            mapped.push_unmapped_char(' ');
        }

        mapped.push_str(&romanized, Some(syllable.source));
        text_bytes += separator + romanized.len();
        mapped_chars += separator + romanized.chars().count();
    }

    let mapped = mapped.finish();
    Some(KoreanKey {
        text: mapped.text,
        kind: KoreanKeyKind::Romanized,
        source_map: mapped.source_map,
    })
}

fn romanized_syllable(syllable: HangulSyllable) -> String {
    let mut out = String::new();
    out.push_str(INITIAL_ROMANIZATION[syllable.initial]);
    out.push_str(VOWEL_ROMANIZATION[syllable.vowel]);
    out.push_str(FINAL_ROMANIZATION[syllable.final_consonant]);
    out
}

fn initials_key(syllables: &[HangulSyllable], max_bytes: usize) -> Option<KoreanKey> {
    let mut mapped = MappedTextBuilder::new();
    let mut text_bytes = 0usize;

    for (mapped_chars, syllable) in syllables.iter().enumerate() {
        let initial = CHOSEONG[syllable.initial];
        if !can_append(text_bytes, mapped_chars, initial.len_utf8(), 1, max_bytes) {
            return None;
        }
        mapped.push_char(initial, Some(syllable.source));
        text_bytes += initial.len_utf8();
    }

    let mapped = mapped.finish();
    Some(KoreanKey {
        text: mapped.text,
        kind: KoreanKeyKind::Initials,
        source_map: mapped.source_map,
    })
}

fn keyboard_key(syllables: &[HangulSyllable], max_bytes: usize) -> Option<KoreanKey> {
    let mut mapped = MappedTextBuilder::new();
    let mut text_bytes = 0usize;
    let mut mapped_chars = 0usize;

    for syllable in syllables {
        for token in [
            INITIAL_KEYS[syllable.initial],
            VOWEL_KEYS[syllable.vowel],
            FINAL_KEYS[syllable.final_consonant],
        ] {
            if !can_append(
                text_bytes,
                mapped_chars,
                token.len(),
                token.chars().count(),
                max_bytes,
            ) {
                return None;
            }
            mapped.push_str(token, Some(syllable.source));
            text_bytes += token.len();
            mapped_chars += token.chars().count();
        }
    }

    let mapped = mapped.finish();
    Some(KoreanKey {
        text: mapped.text,
        kind: KoreanKeyKind::Keyboard,
        source_map: mapped.source_map,
    })
}

fn can_append(
    text_bytes: usize,
    mapped_chars: usize,
    added_bytes: usize,
    added_chars: usize,
    max_bytes: usize,
) -> bool {
    text_bytes.saturating_add(added_bytes) <= max_bytes
        && mapped_chars.saturating_add(added_chars) <= max_bytes
}

fn remaining_bytes(out: &[KoreanKey], max_bytes: usize) -> usize {
    max_bytes.saturating_sub(out.iter().map(|key| key.text.len()).sum::<usize>())
}

fn push_unique(out: &mut Vec<KoreanKey>, value: KoreanKey, max: usize) {
    if out.len() < max
        && !out
            .iter()
            .any(|key| key.kind == value.kind && key.text == value.text)
    {
        out.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hangul_keys_for_hangeul() {
        let keys = build_korean_keys("한글", 8);
        assert!(keys.contains(&"han geul".to_string()));
        assert!(keys.contains(&"hangeul".to_string()));
        assert!(keys.contains(&"ㅎㄱ".to_string()));
        assert!(keys.contains(&"gksrmf".to_string()));
    }

    #[test]
    fn romanization_uses_final_consonant_values() {
        let keys = build_korean_keys("한국", 8);
        assert!(keys.contains(&"han guk".to_string()));
        assert!(keys.contains(&"hanguk".to_string()));
    }

    #[test]
    fn romanization_handles_seoul() {
        let keys = build_korean_keys("서울", 8);
        assert!(keys.contains(&"seo ul".to_string()));
        assert!(keys.contains(&"seoul".to_string()));
    }

    #[test]
    fn keys_are_capped() {
        let keys = build_korean_keys("한글서울한국", 2);
        assert!(keys.len() <= 2);
    }

    #[test]
    fn key_construction_enforces_the_output_budget() {
        let input = "한".repeat(10);
        let keys = build_korean_keys_with_sources_with_budget(&input, 8, input.len());

        assert!(keys.iter().map(|key| key.text.len()).sum::<usize>() <= input.len());
        assert!(keys.iter().all(|key| key.source_map.len() <= input.len()));
    }

    #[test]
    fn keys_include_source_maps() {
        let keys = build_korean_keys_with_sources("docs/한글.txt", 8);
        let initials = keys.iter().find(|key| key.text == "ㅎㄱ").unwrap();
        assert_eq!(initials.source_map.len(), 2);
        assert_eq!(
            initials.source_map[0],
            Some(SourceSpan {
                start_char: 5,
                end_char: 6
            })
        );
        assert_eq!(
            initials.source_map[1],
            Some(SourceSpan {
                start_char: 6,
                end_char: 7
            })
        );

        let joined = keys.iter().find(|key| key.text == "hangeul").unwrap();
        assert_eq!(
            joined.source_map[0],
            Some(SourceSpan {
                start_char: 5,
                end_char: 6
            })
        );
        assert_eq!(
            joined.source_map[3],
            Some(SourceSpan {
                start_char: 6,
                end_char: 7
            })
        );

        let spaced = keys.iter().find(|key| key.text == "han geul").unwrap();
        assert_eq!(spaced.source_map[3], None);
        assert_eq!(
            spaced.source_map[4],
            Some(SourceSpan {
                start_char: 6,
                end_char: 7
            })
        );
    }

    #[test]
    fn non_hangul_input_is_empty() {
        assert!(build_korean_keys("README.md", 8).is_empty());
    }

    #[test]
    fn contains_hangul_detects_syllables_and_jamo() {
        assert!(contains_hangul("한글"));
        assert!(contains_hangul("ㅎㄱ"));
        assert!(!contains_hangul("hangeul"));
    }
}
