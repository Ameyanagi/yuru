/// Counters collected during search execution.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchStats {
    /// Number of candidates visited.
    pub candidates_seen: usize,
    /// Number of compatible search keys scored.
    pub keys_seen: usize,
    /// Number of query variants considered.
    pub variants_seen: usize,
    /// Number of matcher calls made.
    pub fuzzy_calls: usize,
    /// Number of secondary quality scoring calls made.
    pub quality_score_calls: usize,
    /// Number of language reading generation calls made.
    pub reading_generation_calls: usize,
}
