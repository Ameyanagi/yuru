use std::borrow::Cow;
use std::collections::HashSet;

use yuru_core::{match_positions, Candidate, KeyKind, ScoredCandidate, SearchKey};

use super::layout::{
    cluster_display_width, leading_cluster, terminal_safe_prefix, terminal_visible_text, SgrSplit,
};

const MAX_HIGHLIGHT_TEXT_CHARS: usize = 512;
const MAX_HIGHLIGHT_PATTERN_CHARS: usize = 64;
const MAX_HIGHLIGHT_QUERY_TERMS: usize = 32;

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
    // `display` is already clipped to `width` display columns. Highlight
    // positions are character indices into it, never column offsets, so they
    // are bounded by its character count instead of the column budget.
    let visible_chars = visible_display.chars().count();
    let match_display = bounded_chars(&visible_display, MAX_HIGHLIGHT_TEXT_CHARS);
    let patterns = highlight_patterns(query);
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
        // A phonetic key matched something the surface text does not spell, so the
        // fallbacks below highlight the surface without being able to point at the
        // matched characters. That is only honest while the key still matches what is
        // typed: the rows on screen may answer an earlier query while its replacement is
        // still running, and painting one of those as a match claims something false.
        // Where the key is unavailable there is nothing to check against, so the row is
        // highlighted as before.
        let key = matched_key(candidates, result);
        if key.is_some_and(|key| !key_matches(&patterns, key, case_sensitive)) {
            return highlight_segments(&display, &positions, width);
        }

        if let Some(key) = key {
            let positions =
                source_map_highlight_positions(&patterns, key, case_sensitive, visible_chars);
            if !positions.is_empty() {
                return highlight_segments(&display, &positions, width);
            }
        }

        let positions = phonetic_fallback_positions(&visible_display, visible_chars);
        if !positions.is_empty() {
            return highlight_segments(&display, &positions, width);
        }

        return highlight_segments(&display, &(0..visible_chars).collect(), width);
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

/// Returns whether the key a result was scored on still matches any typed pattern.
fn key_matches(patterns: &[&str], key: &SearchKey, case_sensitive: bool) -> bool {
    let bounded_key = bounded_chars(&key.text, MAX_HIGHLIGHT_TEXT_CHARS);
    patterns
        .iter()
        .any(|pattern| match_positions(pattern, &bounded_key, case_sensitive).is_some())
}

fn highlight_patterns(query: &str) -> Vec<&str> {
    query
        .split_whitespace()
        .take(MAX_HIGHLIGHT_QUERY_TERMS)
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

            (!pattern.is_empty()
                && pattern
                    .char_indices()
                    .nth(MAX_HIGHLIGHT_PATTERN_CHARS)
                    .is_none())
            .then_some(pattern)
        })
        .collect()
}

fn highlight_positions(patterns: &[&str], text: &str, case_sensitive: bool) -> HashSet<usize> {
    let mut positions = HashSet::new();
    for pattern in patterns {
        if let Some(matched) = match_positions(pattern, text, case_sensitive) {
            positions.extend(matched.char_indices);
        }
    }
    positions
}

/// Maps phonetic-key match positions back onto the surface text through the
/// key's source map. Every index here is a character position, both in the key
/// and in the display text; `max_chars` is the visible character count of the
/// already-truncated display, not a column budget.
fn source_map_highlight_positions(
    patterns: &[&str],
    key: &SearchKey,
    case_sensitive: bool,
    max_chars: usize,
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
            positions
                .extend((span.start_char..span.end_char).filter(|position| *position < max_chars));
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

/// Splits `text` into highlighted and plain runs. `highlighted_positions` holds
/// character indices, while `width` is a display-column budget. The two are not
/// interchangeable and neither is a cluster index: positions are looked up per
/// character, but the budget advances per grapheme cluster, and a cluster that
/// would straddle the boundary is dropped whole rather than split. A cluster
/// whose base is highlighted keeps its continuation scalars in the same run, so
/// this module never emits a styling escape in the middle of one.
///
/// Clusters come from the escape-free view of `text`, since a base character
/// and its continuation stay one cluster to the terminal however many SGR
/// sequences the record wrote between them; the record's own sequences are put
/// back where it wrote them, inside that cluster included. `text` arrives
/// already clipped by `terminal_safe_prefix`, so all of it is in budget to
/// scan.
fn highlight_segments(
    text: &str,
    highlighted_positions: &HashSet<usize>,
    width: usize,
) -> Vec<HighlightSegment> {
    let split = SgrSplit::new(text, true, false, text.len());
    let visible = split.visible();

    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_highlighted = None;

    let mut char_index = 0;
    let mut columns = 0usize;
    let mut offset = 0;
    let mut sgr_index = 0usize;
    let mut pending_sgr = String::new();
    while offset < visible.len() {
        // A sequence written before the cluster opens the run it lands in.
        while let Some((sgr_offset, sequence)) = split.sgr(sgr_index) {
            if sgr_offset > offset {
                break;
            }
            pending_sgr.push_str(sequence);
            sgr_index += 1;
        }

        let Some(cluster) = leading_cluster(&visible[offset..]) else {
            break;
        };
        let cluster_width = cluster_display_width(cluster);
        if columns.saturating_add(cluster_width) > width {
            break;
        }
        let cluster_chars = cluster.chars().count();
        let highlighted = (char_index..char_index + cluster_chars)
            .any(|position| highlighted_positions.contains(&position));
        if current_highlighted != Some(highlighted) {
            if let Some(highlighted) = current_highlighted {
                segments.push(HighlightSegment {
                    text: std::mem::take(&mut current),
                    highlighted,
                });
            }
            current_highlighted = Some(highlighted);
        }
        current.push_str(&pending_sgr);
        pending_sgr.clear();

        // A sequence written inside the cluster keeps its place in it.
        let cluster_end = offset + cluster.len();
        let mut cursor = offset;
        while let Some((sgr_offset, sequence)) = split.sgr(sgr_index) {
            if sgr_offset >= cluster_end {
                break;
            }
            current.push_str(&visible[cursor..sgr_offset]);
            current.push_str(sequence);
            cursor = sgr_offset;
            sgr_index += 1;
        }
        current.push_str(&visible[cursor..cluster_end]);
        char_index += cluster_chars;
        columns += cluster_width;
        offset = cluster_end;
    }

    // Retain a trailing sequence at the viewport boundary only when it CLEARS
    // styling: dropping a reset there would let the retained text's styling leak
    // into later UI, while keeping an opener would paint the row padding with a
    // colour that belonged to the discarded content. Same rule as
    // `layout::SgrSplit::render_prefix`.
    while let Some((sgr_offset, sequence)) = split.sgr(sgr_index) {
        if sgr_offset > offset {
            break;
        }
        if super::layout::sgr_only_clears(sequence) {
            current.push_str(sequence);
        }
        sgr_index += 1;
    }

    // `pending_sgr` still holding anything here means the loop consumed boundary
    // sequences at its top and then stopped before reaching another cluster - the
    // same boundary position as above, so the same rule applies. (Mid-loop, pending
    // sequences are flushed in front of the cluster they precede and are inside
    // retained content; this final flush is the only one at the boundary.)
    let mut rest = pending_sgr.as_str();
    while let Some(len) = super::layout::safe_sgr_sequence_len(rest) {
        let (sequence, remaining) = rest.split_at(len);
        if super::layout::sgr_only_clears(sequence) {
            current.push_str(sequence);
        }
        rest = remaining;
    }

    if let Some(highlighted) = current_highlighted {
        segments.push(HighlightSegment {
            text: current,
            highlighted,
        });
    }

    segments
}

fn phonetic_fallback_positions(text: &str, max_chars: usize) -> HashSet<usize> {
    text.chars()
        .take(max_chars)
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
