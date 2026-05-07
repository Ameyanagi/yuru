use std::{
    collections::HashMap,
    sync::{OnceLock, RwLock},
};

use lindera::tokenizer::{Tokenizer, TokenizerBuilder};
use yuru_core::SourceSpan;

use crate::numeric::{
    numeric_context_tokenizer_input, numeric_source_digits, NumericTokenizerInput,
};

const IPADIC_READING_INDEX: usize = 7;
const MAX_CACHED_RUN_CHARS: usize = 128;
const MAX_READING_CACHE_ENTRIES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
/// A generated Japanese reading and its per-character source map.
pub struct ReadingCandidate {
    /// Generated reading text.
    pub text: String,
    /// Source span for each generated character, when known.
    pub source_map: Vec<Option<SourceSpan>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CachedReading {
    text: String,
    source_map: Vec<Option<SourceSpan>>,
    used_reading: bool,
}

/// Returns kanji reading candidates for `input`, capped at `max`.
pub fn kanji_reading_candidates(input: &str, max: usize) -> Vec<String> {
    kanji_reading_candidates_with_sources(input, max)
        .into_iter()
        .map(|candidate| candidate.text)
        .collect()
}

/// Returns kanji reading candidates with source maps, capped at `max`.
pub fn kanji_reading_candidates_with_sources(input: &str, max: usize) -> Vec<ReadingCandidate> {
    if max == 0 || !contains_han(input) {
        return Vec::new();
    }

    let mut candidates = vec![CachedReading {
        text: String::new(),
        source_map: Vec::new(),
        used_reading: false,
    }];

    let mut segment_start_byte = 0usize;
    let mut segment_start_char = 0usize;
    let mut segment_kind = None;
    for (char_index, (byte_index, ch)) in input.char_indices().enumerate() {
        let is_reading_context = is_japanese_reading_context(ch);
        if let Some(current_kind) = segment_kind {
            if current_kind != is_reading_context {
                let segment = &input[segment_start_byte..byte_index];
                if !append_segment(
                    segment,
                    segment_start_char,
                    current_kind,
                    &mut candidates,
                    max,
                ) {
                    return Vec::new();
                }
                if candidates.is_empty() {
                    return Vec::new();
                }
                segment_start_byte = byte_index;
                segment_start_char = char_index;
                segment_kind = Some(is_reading_context);
            }
        } else {
            segment_start_byte = byte_index;
            segment_start_char = char_index;
            segment_kind = Some(is_reading_context);
        }
    }

    if let Some(is_reading_context) = segment_kind {
        let segment = &input[segment_start_byte..];
        if !append_segment(
            segment,
            segment_start_char,
            is_reading_context,
            &mut candidates,
            max,
        ) {
            return Vec::new();
        }
    }

    candidates
        .into_iter()
        .filter(|candidate| candidate.used_reading)
        .take(max)
        .map(|candidate| ReadingCandidate {
            text: candidate.text,
            source_map: candidate.source_map,
        })
        .collect()
}

fn append_segment(
    segment: &str,
    start_char: usize,
    is_reading_context: bool,
    candidates: &mut Vec<CachedReading>,
    max: usize,
) -> bool {
    if is_reading_context && contains_han(segment) {
        let Some(mut base) = cached_run_reading(segment) else {
            return false;
        };
        offset_source_map(&mut base.source_map, start_char);

        let Some(numeric_variants) = numeric_context_run_readings(segment) else {
            append_single_segment(candidates, &base);
            return true;
        };

        let mut variants = Vec::new();
        for mut numeric in numeric_variants {
            offset_source_map(&mut numeric.source_map, start_char);
            push_unique_reading(&mut variants, numeric);
        }
        push_unique_reading(&mut variants, base);

        if variants.len() == 1 {
            append_single_segment(candidates, &variants[0]);
        } else {
            append_segment_variants(candidates, &variants, max);
        }
    } else {
        append_surface_segment(candidates, segment, start_char);
    }

    true
}

fn append_single_segment(candidates: &mut [CachedReading], segment: &CachedReading) {
    for candidate in candidates {
        candidate.text.push_str(&segment.text);
        candidate
            .source_map
            .extend(segment.source_map.iter().copied());
        candidate.used_reading |= segment.used_reading;
    }
}

fn append_surface_segment(candidates: &mut [CachedReading], segment: &str, start_char: usize) {
    for candidate in candidates {
        for (offset, ch) in segment.chars().enumerate() {
            candidate.text.push(ch);
            candidate.source_map.push(Some(SourceSpan {
                start_char: start_char + offset,
                end_char: start_char + offset + 1,
            }));
        }
    }
}

fn append_segment_variants(
    candidates: &mut Vec<CachedReading>,
    segment_variants: &[CachedReading],
    max: usize,
) {
    if segment_variants.len() == 1 {
        append_single_segment(candidates, &segment_variants[0]);
        return;
    }

    let mut combined = Vec::new();
    for candidate in candidates.iter() {
        for segment in segment_variants {
            let mut next = CachedReading {
                text: candidate.text.clone(),
                source_map: candidate.source_map.clone(),
                used_reading: candidate.used_reading || segment.used_reading,
            };
            next.text.push_str(&segment.text);
            next.source_map.extend(segment.source_map.iter().copied());
            push_unique_reading(&mut combined, next);
            if combined.len() >= max {
                break;
            }
        }
        if combined.len() >= max {
            break;
        }
    }
    *candidates = combined;
}

fn push_unique_reading(readings: &mut Vec<CachedReading>, reading: CachedReading) {
    if !readings
        .iter()
        .any(|existing| existing.text == reading.text && existing.source_map == reading.source_map)
    {
        readings.push(reading);
    }
}

fn offset_source_map(source_map: &mut [Option<SourceSpan>], start_char: usize) {
    for span in source_map.iter_mut().flatten() {
        span.start_char += start_char;
        span.end_char += start_char;
    }
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
    compute_run_reading_with_source_map(run, None)
}

fn compute_run_reading_with_source_map(
    run: &str,
    transformed_source_map: Option<&[SourceSpan]>,
) -> Option<CachedReading> {
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
        let token_start = byte_to_char_index(&char_starts, token.byte_start);
        let token_end = byte_to_char_index(&char_starts, token.byte_end);
        let span = transformed_source_map
            .and_then(|source_map| merge_source_span_slice(source_map, token_start, token_end))
            .or(Some(SourceSpan {
                start_char: token_start,
                end_char: token_end,
            }));

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

fn numeric_context_run_readings(run: &str) -> Option<Vec<CachedReading>> {
    let input = numeric_context_tokenizer_input(run)?;
    let mut readings = Vec::new();

    if let Some(reading) = compute_numeric_preserving_run_reading(run, &input) {
        push_unique_reading(&mut readings, reading);
    }
    push_unique_reading(
        &mut readings,
        compute_run_reading_with_source_map(&input.text, Some(&input.source_map))?,
    );

    Some(readings)
}

fn compute_numeric_preserving_run_reading(
    original_run: &str,
    input: &NumericTokenizerInput,
) -> Option<CachedReading> {
    let tokenizer = tokenizer()?;
    let mut tokens = tokenizer.tokenize(&input.text).ok()?;

    let char_starts = char_start_byte_indices(&input.text);
    let mut text = String::new();
    let mut source_map = Vec::new();
    let mut used_reading = false;
    let mut emitted_numeric_spans = Vec::new();

    for token in tokens.iter_mut() {
        let surface = token.surface.as_ref().to_owned();
        let reading = token
            .get_detail(IPADIC_READING_INDEX)
            .map(str::to_owned)
            .filter(|value| valid_reading(value))
            .unwrap_or_else(|| surface.clone());
        let token_start = byte_to_char_index(&char_starts, token.byte_start);
        let token_end = byte_to_char_index(&char_starts, token.byte_end);
        let token_span = input
            .source_map
            .get(token_start..token_end)
            .and_then(|_| merge_source_span_slice(&input.source_map, token_start, token_end));

        if let Some(mixed) = numeric_preserving_token_reading(
            original_run,
            input,
            token_start,
            token_end,
            &reading,
            token_span,
            &mut emitted_numeric_spans,
        ) {
            text.push_str(&mixed.text);
            source_map.extend(mixed.source_map);
            used_reading = true;
            continue;
        }

        let span = token_span.or(Some(SourceSpan {
            start_char: token_start,
            end_char: token_end,
        }));
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

struct MixedTokenReading {
    text: String,
    source_map: Vec<Option<SourceSpan>>,
}

struct NumericChunk {
    start: usize,
    end: usize,
    text: String,
    source_map: Vec<Option<SourceSpan>>,
}

fn numeric_preserving_token_reading(
    original_run: &str,
    input: &NumericTokenizerInput,
    token_start: usize,
    token_end: usize,
    reading: &str,
    token_span: Option<SourceSpan>,
    emitted_numeric_spans: &mut Vec<SourceSpan>,
) -> Option<MixedTokenReading> {
    let chunks = numeric_chunks(
        original_run,
        input,
        token_start,
        token_end,
        emitted_numeric_spans,
    );
    if chunks.is_empty() {
        return None;
    }

    let mut text = String::new();
    let mut source_map = Vec::new();
    let mut remaining = reading;

    for chunk in chunks {
        let surface = char_range(&input.text, chunk.start, chunk.end);
        let numeric_reading = compute_run_reading(&surface)?.text;
        let position = remaining.find(&numeric_reading)?;
        let before = &remaining[..position];
        text.push_str(before);
        source_map.extend(before.chars().map(|_| token_span));
        text.push_str(&chunk.text);
        source_map.extend(chunk.source_map);
        remaining = &remaining[position + numeric_reading.len()..];
    }

    text.push_str(remaining);
    source_map.extend(remaining.chars().map(|_| token_span));

    Some(MixedTokenReading { text, source_map })
}

fn numeric_chunks(
    original_run: &str,
    input: &NumericTokenizerInput,
    token_start: usize,
    token_end: usize,
    emitted_numeric_spans: &mut Vec<SourceSpan>,
) -> Vec<NumericChunk> {
    let mut chunks = Vec::new();
    let mut index = token_start;

    while index < token_end {
        let Some(span) = input.source_map.get(index).copied() else {
            break;
        };
        if numeric_source_digits(original_run, span).is_none() {
            index += 1;
            continue;
        }

        let start = index;
        let mut text = String::new();
        let mut source_map = Vec::new();
        let mut last_span = None;

        while index < token_end {
            let Some(span) = input.source_map.get(index).copied() else {
                break;
            };
            let Some(digits) = numeric_source_digits(original_run, span) else {
                break;
            };

            if last_span != Some(span) {
                if !emitted_numeric_spans.contains(&span) {
                    for (digit, digit_span) in digits {
                        text.push(digit);
                        source_map.push(Some(digit_span));
                    }
                    emitted_numeric_spans.push(span);
                }
                last_span = Some(span);
            }
            index += 1;
        }

        chunks.push(NumericChunk {
            start,
            end: index,
            text,
            source_map,
        });
    }

    chunks
}

fn char_range(text: &str, start: usize, end: usize) -> String {
    text.chars().skip(start).take(end - start).collect()
}

fn merge_source_span_slice(
    source_map: &[SourceSpan],
    start: usize,
    end: usize,
) -> Option<SourceSpan> {
    let first = source_map.get(start)?;
    let mut merged = *first;
    for span in source_map.get(start + 1..end).unwrap_or_default() {
        merged.start_char = merged.start_char.min(span.start_char);
        merged.end_char = merged.end_char.max(span.end_char);
    }
    Some(merged)
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

fn is_japanese_reading_context(ch: char) -> bool {
    is_numeric_reading_context(ch) || is_japanese_text(ch)
}

fn is_numeric_reading_context(ch: char) -> bool {
    ch.is_numeric()
        || matches!(
            ch,
            ',' | '.' | '，' | '．' | '、' | '。' | '\u{ff0d}' | '\u{2212}' | '-'
        )
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
            Some(SourceSpan {
                start_char: 6,
                end_char: 9
            })
        );
        assert_eq!(
            candidate.source_map[no_index],
            Some(SourceSpan {
                start_char: 9,
                end_char: 10
            })
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
    fn lindera_keeps_numeric_context_for_date_readings() {
        let candidate = kanji_reading_candidates_with_sources("2025年8月　写真展示.pdf", 8)
            .into_iter()
            .find(|candidate| candidate.text.contains("ネン") && candidate.text.contains("ガツ"))
            .unwrap();
        let nen_index = candidate.text.find("ネン").unwrap();
        let gatsu_index = candidate.text.find("ガツ").unwrap();
        let nen_char = candidate.text[..nen_index].chars().count();
        let gatsu_char = candidate.text[..gatsu_index].chars().count();

        assert_eq!(
            candidate.source_map[nen_char],
            Some(SourceSpan {
                start_char: 4,
                end_char: 5
            })
        );
        assert_eq!(
            candidate.source_map[gatsu_char],
            Some(SourceSpan {
                start_char: 5,
                end_char: 7
            })
        );
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
            Some(SourceSpan {
                start_char: 3,
                end_char: 6
            })
        );
        assert_eq!(
            candidate.source_map[second_char],
            Some(SourceSpan {
                start_char: 7,
                end_char: 10
            })
        );
    }
}
