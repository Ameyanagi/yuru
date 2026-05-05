use std::{
    collections::HashMap,
    sync::{OnceLock, RwLock},
};

use lindera::tokenizer::{Tokenizer, TokenizerBuilder};
use yuru_core::SourceSpan;

const IPADIC_READING_INDEX: usize = 7;
const MAX_CACHED_RUN_CHARS: usize = 128;
const MAX_READING_CACHE_ENTRIES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadingCandidate {
    pub text: String,
    pub source_map: Vec<Option<SourceSpan>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CachedReading {
    text: String,
    source_map: Vec<Option<SourceSpan>>,
    used_reading: bool,
}

pub fn kanji_reading_candidates(input: &str, max: usize) -> Vec<String> {
    kanji_reading_candidates_with_sources(input, max)
        .into_iter()
        .map(|candidate| candidate.text)
        .collect()
}

pub fn kanji_reading_candidates_with_sources(input: &str, max: usize) -> Vec<ReadingCandidate> {
    if max == 0 || !contains_han(input) {
        return Vec::new();
    }

    let mut text = String::with_capacity(input.len());
    let mut source_map = Vec::with_capacity(input.chars().count());
    let mut used_reading = false;

    let mut segment_start_byte = 0usize;
    let mut segment_start_char = 0usize;
    let mut segment_kind = None;
    for (char_index, (byte_index, ch)) in input.char_indices().enumerate() {
        let is_japanese = is_japanese_text(ch);
        if let Some(current_kind) = segment_kind {
            if current_kind != is_japanese {
                let segment = &input[segment_start_byte..byte_index];
                if !append_segment(
                    segment,
                    segment_start_char,
                    current_kind,
                    &mut text,
                    &mut source_map,
                    &mut used_reading,
                ) {
                    return Vec::new();
                }
                segment_start_byte = byte_index;
                segment_start_char = char_index;
                segment_kind = Some(is_japanese);
            }
        } else {
            segment_start_byte = byte_index;
            segment_start_char = char_index;
            segment_kind = Some(is_japanese);
        }
    }

    if let Some(is_japanese) = segment_kind {
        let segment = &input[segment_start_byte..];
        if !append_segment(
            segment,
            segment_start_char,
            is_japanese,
            &mut text,
            &mut source_map,
            &mut used_reading,
        ) {
            return Vec::new();
        }
    }

    if used_reading {
        vec![ReadingCandidate { text, source_map }]
    } else {
        Vec::new()
    }
}

fn append_segment(
    segment: &str,
    start_char: usize,
    is_japanese: bool,
    text: &mut String,
    source_map: &mut Vec<Option<SourceSpan>>,
    used_reading: &mut bool,
) -> bool {
    if is_japanese && contains_han(segment) {
        let Some(reading) = cached_run_reading(segment) else {
            return false;
        };
        *used_reading |= reading.used_reading;
        text.push_str(&reading.text);
        source_map.extend(reading.source_map.into_iter().map(|span| {
            span.map(|span| SourceSpan {
                start: span.start + start_char,
                end: span.end + start_char,
            })
        }));
    } else {
        push_surface_segment(segment, start_char, text, source_map);
    }

    true
}

fn cached_run_reading(run: &str) -> Option<CachedReading> {
    let cacheable = run.chars().count() <= MAX_CACHED_RUN_CHARS;
    if cacheable {
        if let Ok(cache) = reading_cache().read() {
            if let Some(reading) = cache.get(run) {
                return Some(reading.clone());
            }
        }
    }

    let reading = compute_run_reading(run)?;
    if cacheable {
        if let Ok(mut cache) = reading_cache().write() {
            if cache.len() < MAX_READING_CACHE_ENTRIES {
                cache.insert(run.to_owned(), reading.clone());
            }
        }
    }

    Some(reading)
}

fn compute_run_reading(run: &str) -> Option<CachedReading> {
    let tokenizer = tokenizer()?;
    let mut tokens = tokenizer.tokenize(run).ok()?;

    let char_starts = char_start_byte_indices(run);
    let mut text = String::new();
    let mut source_map = Vec::new();
    let mut used_reading = false;

    for token in tokens.iter_mut() {
        let surface = token.surface.as_ref().to_owned();
        let reading = token
            .get_detail(IPADIC_READING_INDEX)
            .map(str::to_owned)
            .filter(|value| valid_reading(value))
            .unwrap_or_else(|| surface.clone());
        let span = Some(SourceSpan {
            start: byte_to_char_index(&char_starts, token.byte_start),
            end: byte_to_char_index(&char_starts, token.byte_end),
        });

        used_reading |= reading != surface;
        text.push_str(&reading);
        source_map.extend(reading.chars().map(|_| span));
    }

    Some(CachedReading {
        text,
        source_map,
        used_reading,
    })
}

fn push_surface_segment(
    segment: &str,
    start_char: usize,
    text: &mut String,
    source_map: &mut Vec<Option<SourceSpan>>,
) {
    for (offset, ch) in segment.chars().enumerate() {
        text.push(ch);
        source_map.push(Some(SourceSpan {
            start: start_char + offset,
            end: start_char + offset + 1,
        }));
    }
}

fn reading_cache() -> &'static RwLock<HashMap<String, CachedReading>> {
    static CACHE: OnceLock<RwLock<HashMap<String, CachedReading>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn tokenizer() -> Option<&'static Tokenizer> {
    static TOKENIZER: OnceLock<Option<Tokenizer>> = OnceLock::new();

    TOKENIZER
        .get_or_init(|| {
            let mut builder = TokenizerBuilder::new().ok()?;
            builder.set_segmenter_dictionary("embedded://ipadic");
            builder.build().ok()
        })
        .as_ref()
}

fn valid_reading(value: &str) -> bool {
    !value.is_empty() && value != "*"
}

fn contains_han(text: &str) -> bool {
    text.chars().any(is_han)
}

fn is_japanese_text(ch: char) -> bool {
    is_han(ch)
        || ('\u{3040}'..='\u{309f}').contains(&ch)
        || ('\u{30a0}'..='\u{30ff}').contains(&ch)
        || ('\u{31f0}'..='\u{31ff}').contains(&ch)
        || matches!(ch, '々' | '〆' | '〇')
}

fn is_han(ch: char) -> bool {
    ('\u{3400}'..='\u{4dbf}').contains(&ch)
        || ('\u{4e00}'..='\u{9fff}').contains(&ch)
        || ('\u{f900}'..='\u{faff}').contains(&ch)
        || ('\u{20000}'..='\u{2a6df}').contains(&ch)
        || ('\u{2a700}'..='\u{2b73f}').contains(&ch)
        || ('\u{2b740}'..='\u{2b81f}').contains(&ch)
        || ('\u{2b820}'..='\u{2ceaf}').contains(&ch)
        || ('\u{2ceb0}'..='\u{2ebef}').contains(&ch)
        || ('\u{30000}'..='\u{3134f}').contains(&ch)
}

fn char_start_byte_indices(input: &str) -> Vec<usize> {
    input.char_indices().map(|(index, _)| index).collect()
}

fn byte_to_char_index(char_starts: &[usize], byte_index: usize) -> usize {
    char_starts
        .binary_search(&byte_index)
        .unwrap_or_else(|index| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_candidates_for_japanese_language_files() {
        let candidates = kanji_reading_candidates("tests/日本語.txt", 8);
        assert!(candidates
            .iter()
            .any(|candidate| candidate.contains("ニホンゴ")));

        let candidates = kanji_reading_candidates("tests/日本人の.txt", 8);
        assert!(candidates.iter().any(|candidate| {
            candidate.contains("ニホンジンノ") || candidate.contains("ニッポンジンノ")
        }));
    }

    #[test]
    fn reading_candidates_include_source_spans() {
        let candidate = kanji_reading_candidates_with_sources("tests/日本人の.txt", 8)
            .into_iter()
            .find(|candidate| {
                candidate.text.contains("ニホンジンノ") || candidate.text.contains("ニッポンジンノ")
            })
            .unwrap();
        let ni_index = candidate.text.chars().position(|ch| ch == 'ニ').unwrap();
        let no_index = candidate.text.chars().position(|ch| ch == 'ノ').unwrap();

        assert_eq!(
            candidate.source_map[ni_index],
            Some(SourceSpan { start: 6, end: 9 })
        );
        assert_eq!(
            candidate.source_map[no_index],
            Some(SourceSpan { start: 9, end: 10 })
        );
    }

    #[test]
    fn lindera_reads_general_words() {
        let candidates = kanji_reading_candidates("形態素解析.txt", 8);
        assert!(candidates
            .iter()
            .any(|candidate| candidate.contains("ケイタイソ") && candidate.contains("カイセキ")));
    }

    #[test]
    fn reading_candidates_are_capped() {
        assert!(kanji_reading_candidates("tests/日本語.txt", 0).is_empty());
        assert!(kanji_reading_candidates("tests/日本語.txt", 1).len() <= 1);
    }

    #[test]
    fn repeated_reading_runs_keep_independent_source_spans() {
        let candidate = kanji_reading_candidates_with_sources("資料/日本語/日本語.txt", 8)
            .into_iter()
            .find(|candidate| candidate.text.matches("ニホンゴ").count() == 2)
            .unwrap();
        let mut starts = candidate.text.match_indices('ニ');
        let first = starts.next().unwrap().0;
        let second = starts.next().unwrap().0;
        let first_char = candidate.text[..first].chars().count();
        let second_char = candidate.text[..second].chars().count();

        assert_eq!(
            candidate.source_map[first_char],
            Some(SourceSpan { start: 3, end: 6 })
        );
        assert_eq!(
            candidate.source_map[second_char],
            Some(SourceSpan { start: 7, end: 10 })
        );
    }
}
