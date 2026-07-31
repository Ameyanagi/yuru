use std::borrow::Cow;
use std::collections::HashSet;

use yuru_core::{match_positions, Candidate, KeyKind, ScoredCandidate, SearchKey};

use super::layout::{safe_sgr_sequence_len, terminal_safe_prefix, terminal_visible_text};

const MAX_HIGHLIGHT_TEXT_CHARS: usize = 512;
const MAX_HIGHLIGHT_PATTERN_CHARS: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HighlightSegment {
    pub(crate) text: String,
    pub(crate) highlighted: bool,
}

#[cfg(test)]
pub(crate) fn highlight_segments_for_result(
    query: &str,
    result: &ScoredCandidate,
    candidates: &[Candidate],
    case_sensitive: bool,
    width: usize,
) -> Vec<HighlightSegment> {
    highlight_segments_for_result_with_ansi(query, result, candidates, case_sensitive, width, false)
}

pub(crate) fn highlight_segments_for_result_with_ansi(
    query: &str,
    result: &ScoredCandidate,
    candidates: &[Candidate],
    case_sensitive: bool,
    width: usize,
    allow_ansi: bool,
) -> Vec<HighlightSegment> {
    let (display, _) = terminal_safe_prefix(&result.display, allow_ansi, width);
    let visible_display = terminal_visible_text(&display);
    let match_width = width.min(MAX_HIGHLIGHT_TEXT_CHARS);
    let match_display = bounded_chars(&visible_display, match_width);
    let patterns = highlight_patterns(query)
        .into_iter()
        .map(|pattern| bounded_chars(&pattern, MAX_HIGHLIGHT_PATTERN_CHARS).into_owned())
        .collect::<Vec<_>>();
    let positions = highlight_positions(&patterns, &match_display, case_sensitive);
    if positions.is_empty()
        && !patterns.is_empty()
        && matches!(
            result.key_kind,
            KeyKind::KanaReading
                | KeyKind::RomajiReading
                | KeyKind::PinyinFull
                | KeyKind::PinyinJoined
                | KeyKind::PinyinInitials
                | KeyKind::KoreanRomanized
                | KeyKind::KoreanInitials
                | KeyKind::KoreanKeyboard
                | KeyKind::LearnedAlias
        )
    {
        if let Some(key) = matched_key(candidates, result) {
            let positions = source_map_highlight_positions(&patterns, key, case_sensitive, width);
            if !positions.is_empty() {
                return highlight_segments(&display, &positions, width);
            }
        }

        let positions = phonetic_fallback_positions(&visible_display, width);
        if !positions.is_empty() {
            return highlight_segments(&display, &positions, width);
        }

        return highlight_segments(
            &display,
            &(0..visible_display.chars().take(width).count()).collect(),
            width,
        );
    }

    highlight_segments(&display, &positions, width)
}

fn matched_key<'a>(candidates: &'a [Candidate], result: &ScoredCandidate) -> Option<&'a SearchKey> {
    candidates
        .get(result.id)
        .filter(|candidate| candidate.id == result.id)
        .or_else(|| {
            candidates
                .iter()
                .find(|candidate| candidate.id == result.id)
        })
        .and_then(|candidate| candidate.keys.get(result.key_index as usize))
}

fn highlight_patterns(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .filter_map(|raw| {
            if raw == "|" {
                return None;
            }

            let mut pattern = raw;
            if pattern.starts_with('!') {
                return None;
            }
            if let Some(stripped) = pattern.strip_prefix('\'') {
                pattern = stripped;
            }
            if let Some(stripped) = pattern.strip_prefix('^') {
                pattern = stripped;
            }
            if let Some(stripped) = pattern.strip_suffix('$') {
                pattern = stripped;
            }
            if let Some(stripped) = pattern.strip_suffix('\'') {
                pattern = stripped;
            }

            (!pattern.is_empty()).then(|| pattern.to_string())
        })
        .collect()
}

fn highlight_positions(patterns: &[String], text: &str, case_sensitive: bool) -> HashSet<usize> {
    let mut positions = HashSet::new();
    for pattern in patterns {
        if let Some(matched) = match_positions(pattern, text, case_sensitive) {
            positions.extend(matched.char_indices);
        }
    }
    positions
}

fn source_map_highlight_positions(
    patterns: &[String],
    key: &SearchKey,
    case_sensitive: bool,
    width: usize,
) -> HashSet<usize> {
    let Some(source_map) = &key.source_map else {
        return HashSet::new();
    };

    let mut positions = HashSet::new();
    for pattern in patterns {
        let bounded_key = bounded_chars(&key.text, MAX_HIGHLIGHT_TEXT_CHARS);
        let Some(matched) = match_positions(pattern, &bounded_key, case_sensitive) else {
            continue;
        };

        for key_char_index in matched.char_indices {
            let Some(Some(span)) = source_map.get(key_char_index) else {
                continue;
            };
            positions.extend((span.start_char..span.end_char).filter(|position| *position < width));
        }
    }

    positions
}

fn bounded_chars(text: &str, max_chars: usize) -> Cow<'_, str> {
    let Some((byte_index, _)) = text.char_indices().nth(max_chars) else {
        return Cow::Borrowed(text);
    };
    Cow::Owned(text[..byte_index].to_string())
}

fn highlight_segments(
    text: &str,
    highlighted_positions: &HashSet<usize>,
    width: usize,
) -> Vec<HighlightSegment> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_highlighted = None;

    let mut char_index = 0;
    let mut offset = 0;
    let mut pending_sgr = String::new();
    while offset < text.len() && char_index < width {
        if let Some(len) = safe_sgr_sequence_len(&text[offset..]) {
            pending_sgr.push_str(&text[offset..offset + len]);
            offset += len;
            continue;
        }

        let ch = text[offset..]
            .chars()
            .next()
            .expect("valid character boundary");
        let highlighted = highlighted_positions.contains(&char_index);
        if current_highlighted == Some(highlighted) {
            current.push_str(&pending_sgr);
            pending_sgr.clear();
            current.push(ch);
        } else {
            if let Some(highlighted) = current_highlighted {
                segments.push(HighlightSegment {
                    text: std::mem::take(&mut current),
                    highlighted,
                });
            }
            current.push_str(&pending_sgr);
            pending_sgr.clear();
            current.push(ch);
            current_highlighted = Some(highlighted);
        }
        char_index += 1;
        offset += ch.len_utf8();
    }

    // Retain a trailing reset or style sequence when it falls exactly at the
    // viewport boundary, preventing input styling from leaking into later UI.
    while offset < text.len() {
        let Some(len) = safe_sgr_sequence_len(&text[offset..]) else {
            break;
        };
        current.push_str(&text[offset..offset + len]);
        offset += len;
    }

    if let Some(highlighted) = current_highlighted {
        segments.push(HighlightSegment {
            text: current,
            highlighted,
        });
    }

    segments
}

fn phonetic_fallback_positions(text: &str, width: usize) -> HashSet<usize> {
    text.chars()
        .take(width)
        .enumerate()
        .filter_map(|(index, ch)| is_visible_phonetic_surface(ch).then_some(index))
        .collect()
}

fn is_visible_phonetic_surface(ch: char) -> bool {
    ('\u{3040}'..='\u{309f}').contains(&ch)
        || ('\u{30a0}'..='\u{30ff}').contains(&ch)
        || ('\u{3400}'..='\u{4dbf}').contains(&ch)
        || ('\u{4e00}'..='\u{9fff}').contains(&ch)
}
