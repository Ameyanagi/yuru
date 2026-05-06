use crate::{
    dedup_and_limit_variants, key_kind_allowed, normalize, Candidate, KeyKind, LanguageBackend,
    MatcherBackend, QueryVariantKind, ScoredCandidate, SearchConfig, SearchStats,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExtendedQuery {
    groups: Vec<Vec<Term>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Term {
    text: String,
    negated: bool,
    mode: TermMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TermMode {
    Fuzzy,
    Exact,
    Prefix,
    Suffix,
    Equal,
    Boundary,
}

pub(crate) fn score_candidate(
    query: &str,
    candidate: &Candidate,
    backend: &dyn LanguageBackend,
    matcher: &mut dyn MatcherBackend,
    config: &SearchConfig,
    stats: &mut SearchStats,
) -> Option<ScoredCandidate> {
    let parsed = ExtendedQuery::parse(query, config.exact);
    if parsed.groups.is_empty() {
        return Some(ScoredCandidate {
            id: candidate.id,
            display: candidate.display.clone(),
            score: 0,
            key_kind: KeyKind::Original,
            key_index: 0,
        });
    }

    let mut best: Option<ScoredCandidate> = None;
    for group in parsed.groups {
        let mut group_score = 0i64;
        let mut group_kind = KeyKind::Original;
        let mut group_key_index = 0u32;
        let mut group_matches = true;

        for term in group {
            let matched = match_term(&term, candidate, backend, matcher, config, stats);
            if term.negated {
                if matched.is_some() {
                    group_matches = false;
                    break;
                }
                continue;
            }

            if let Some((score, kind, key_index)) = matched {
                group_score += score;
                group_kind = kind;
                group_key_index = key_index;
            } else {
                group_matches = false;
                break;
            }
        }

        if group_matches {
            let scored = ScoredCandidate {
                id: candidate.id,
                display: candidate.display.clone(),
                score: group_score,
                key_kind: group_kind,
                key_index: group_key_index,
            };
            if best
                .as_ref()
                .is_none_or(|current| scored.score > current.score)
            {
                best = Some(scored);
            }
        }
    }

    best
}

pub(crate) fn requires_extended_search(query: &str) -> bool {
    let mut escaped = false;

    for ch in query.chars() {
        if escaped {
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            ' ' | '\t' | '|' | '!' | '\'' | '^' | '$' => return true,
            _ => {}
        }
    }

    false
}

fn match_term(
    term: &Term,
    candidate: &Candidate,
    backend: &dyn LanguageBackend,
    matcher: &mut dyn MatcherBackend,
    config: &SearchConfig,
    stats: &mut SearchStats,
) -> Option<(i64, KeyKind, u32)> {
    match term.mode {
        TermMode::Fuzzy => match_fuzzy_term(term, candidate, backend, matcher, config, stats),
        TermMode::Exact
        | TermMode::Prefix
        | TermMode::Suffix
        | TermMode::Equal
        | TermMode::Boundary => match_exact_term(term, candidate, config),
    }
}

fn match_fuzzy_term(
    term: &Term,
    candidate: &Candidate,
    backend: &dyn LanguageBackend,
    matcher: &mut dyn MatcherBackend,
    config: &SearchConfig,
    stats: &mut SearchStats,
) -> Option<(i64, KeyKind, u32)> {
    let variants =
        dedup_and_limit_variants(backend.expand_query(&term.text), config.max_query_variants);
    stats.variants_seen += variants.len();

    let mut best: Option<(i64, KeyKind, u32)> = None;
    for variant in variants {
        if variant_blocked_by_config(variant.kind, config) {
            continue;
        }

        for (key_index, key) in candidate.keys.iter().enumerate() {
            if key_blocked_by_case_mode(key.kind, config) || !key_kind_allowed(&variant, key.kind) {
                continue;
            }

            stats.keys_seen += 1;
            stats.fuzzy_calls += 1;
            if let Some(base_score) = matcher.score(&variant.text, &key.text) {
                let score = base_score + i64::from(variant.weight + key.weight);
                if best.as_ref().is_none_or(|(current, _, _)| score > *current) {
                    best = Some((score, key.kind, key_index as u32));
                }
            }
        }
    }

    best
}

fn match_exact_term(
    term: &Term,
    candidate: &Candidate,
    config: &SearchConfig,
) -> Option<(i64, KeyKind, u32)> {
    let needle = comparable(&term.text, config);
    let mut best: Option<(i64, KeyKind, u32)> = None;

    for (key_index, key) in candidate.keys.iter().enumerate() {
        if key_blocked_by_case_mode(key.kind, config) {
            continue;
        }

        let haystack = comparable(&key.text, config);
        let Some(base_score) = exact_score(term.mode, &needle, &haystack) else {
            continue;
        };
        let score = base_score + i64::from(key.weight);
        if best.as_ref().is_none_or(|(current, _, _)| score > *current) {
            best = Some((score, key.kind, key_index as u32));
        }
    }

    best
}

fn exact_score(mode: TermMode, needle: &str, haystack: &str) -> Option<i64> {
    if needle.is_empty() {
        return Some(0);
    }

    match mode {
        TermMode::Exact => {
            let start = haystack.find(needle)?;
            Some(7000 - start as i64 * 5 - haystack.chars().count() as i64)
        }
        TermMode::Prefix => haystack
            .starts_with(needle)
            .then(|| 8500 - haystack.chars().count() as i64),
        TermMode::Suffix => haystack
            .ends_with(needle)
            .then(|| 8500 - haystack.chars().count() as i64),
        TermMode::Equal => (haystack == needle).then_some(10_000),
        TermMode::Boundary => boundary_match(needle, haystack)
            .map(|start| 8000 - start as i64 * 5 - haystack.chars().count() as i64),
        TermMode::Fuzzy => None,
    }
}

fn boundary_match(needle: &str, haystack: &str) -> Option<usize> {
    for (start, _) in haystack.match_indices(needle) {
        let end = start + needle.len();
        if is_boundary_at(haystack, start) && is_boundary_at(haystack, end) {
            return Some(start);
        }
    }
    None
}

fn is_boundary_at(text: &str, byte_index: usize) -> bool {
    if byte_index == 0 || byte_index >= text.len() {
        return true;
    }

    let prev = text[..byte_index].chars().next_back();
    let next = text[byte_index..].chars().next();
    match (prev, next) {
        (Some(left), Some(right)) => {
            (!left.is_alphanumeric() || left == '_') || (!right.is_alphanumeric() || right == '_')
        }
        _ => true,
    }
}

fn comparable(text: &str, config: &SearchConfig) -> String {
    if config.case_sensitive {
        text.to_string()
    } else if config.normalize {
        normalize::normalize(text)
    } else {
        text.to_lowercase()
    }
}

fn key_blocked_by_case_mode(kind: KeyKind, config: &SearchConfig) -> bool {
    kind == KeyKind::Normalized && (config.case_sensitive || !config.normalize)
}

fn variant_blocked_by_config(kind: QueryVariantKind, config: &SearchConfig) -> bool {
    kind == QueryVariantKind::Normalized && (config.case_sensitive || !config.normalize)
}

impl ExtendedQuery {
    fn parse(query: &str, exact_default: bool) -> Self {
        let tokens = split_terms(query);
        let mut groups = vec![Vec::new()];

        for token in tokens {
            if token == "|" {
                groups.push(Vec::new());
                continue;
            }

            if let Some(term) = Term::parse(&token, exact_default) {
                groups.last_mut().expect("group exists").push(term);
            }
        }

        groups.retain(|group| !group.is_empty());
        Self { groups }
    }
}

impl Term {
    fn parse(raw: &str, exact_default: bool) -> Option<Self> {
        let mut text = raw;
        let mut negated = false;

        if let Some(stripped) = text.strip_prefix('!') {
            negated = true;
            text = stripped;
        }

        if text.is_empty() {
            return None;
        }

        let mut mode = if negated || exact_default {
            TermMode::Exact
        } else {
            TermMode::Fuzzy
        };

        if let Some(stripped) = text.strip_prefix('\'') {
            text = stripped;
            mode = if exact_default {
                TermMode::Fuzzy
            } else if text.ends_with('\'') && text.len() > 1 {
                text = &text[..text.len() - 1];
                TermMode::Boundary
            } else {
                TermMode::Exact
            };
        }

        let starts_with_anchor = text.starts_with('^');
        let ends_with_anchor = text.ends_with('$') && text.len() > usize::from(starts_with_anchor);
        if starts_with_anchor {
            text = &text[1..];
        }
        if ends_with_anchor {
            text = &text[..text.len() - 1];
        }

        mode = match (starts_with_anchor, ends_with_anchor) {
            (true, true) => TermMode::Equal,
            (true, false) => TermMode::Prefix,
            (false, true) => TermMode::Suffix,
            (false, false) => mode,
        };

        (!text.is_empty()).then(|| Self {
            text: text.to_string(),
            negated,
            mode,
        })
    }
}

fn split_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut escaped = false;

    for ch in query.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            ' ' | '\t' => {
                if !current.is_empty() {
                    terms.push(std::mem::take(&mut current));
                }
            }
            '|' => {
                if !current.is_empty() {
                    terms.push(std::mem::take(&mut current));
                }
                terms.push("|".to_string());
            }
            _ => current.push(ch),
        }
    }

    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        terms.push(current);
    }

    terms
}

#[cfg(test)]
mod tests {
    use crate::{
        build_index, query::PlainBackend, rank::search, Candidate, GreedyMatcher, SearchConfig,
        SearchKey,
    };

    use super::*;

    #[test]
    fn split_escaped_space() {
        assert_eq!(split_terms("foo\\ bar baz"), vec!["foo bar", "baz"]);
    }

    #[test]
    fn simple_query_does_not_require_extended_search() {
        assert!(!requires_extended_search("kamera"));
        assert!(requires_extended_search("src !test"));
        assert!(requires_extended_search("^src"));
    }

    #[test]
    fn parse_extended_terms() {
        let parsed = ExtendedQuery::parse("'foo ^bar baz$ !qux | zip", false);
        assert_eq!(parsed.groups.len(), 2);
        assert_eq!(parsed.groups[0][0].mode, TermMode::Exact);
        assert_eq!(parsed.groups[0][1].mode, TermMode::Prefix);
        assert_eq!(parsed.groups[0][2].mode, TermMode::Suffix);
        assert!(parsed.groups[0][3].negated);
    }

    #[test]
    fn extended_negation_filters_candidates() {
        let cfg = SearchConfig::default();
        let index = build_index(["src/main.rs", "src/test.rs"], &PlainBackend, &cfg);
        let results = search("src !test", &index, &PlainBackend, &cfg);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].display, "src/main.rs");
    }

    #[test]
    fn exact_mode_disables_fuzzy_matching() {
        let cfg = SearchConfig {
            exact: true,
            ..SearchConfig::default()
        };
        let index = build_index(["a_b_c", "abc"], &PlainBackend, &cfg);
        let results = search("abc", &index, &PlainBackend, &cfg);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].display, "abc");
    }

    #[test]
    fn scoring_empty_query_matches_candidate() {
        let cfg = SearchConfig::default();
        let index = build_index(["abc"], &PlainBackend, &cfg);
        let mut matcher = GreedyMatcher;
        let mut stats = SearchStats::default();
        assert!(
            score_candidate("", &index[0], &PlainBackend, &mut matcher, &cfg, &mut stats).is_some()
        );
    }

    #[test]
    fn exact_term_checks_later_phonetic_keys() {
        let cfg = SearchConfig::default();
        let candidate = Candidate {
            id: 0,
            display: "北京大学".to_string(),
            keys: vec![
                SearchKey::original("北京大学"),
                SearchKey::normalized("北京大学"),
                SearchKey::pinyin_initials("bjdx"),
            ],
        };
        let mut matcher = GreedyMatcher;
        let mut stats = SearchStats::default();

        let scored = score_candidate(
            "'bjdx",
            &candidate,
            &PlainBackend,
            &mut matcher,
            &cfg,
            &mut stats,
        );

        assert!(scored.is_some());
        assert_eq!(scored.unwrap().key_index, 2);
    }
}
