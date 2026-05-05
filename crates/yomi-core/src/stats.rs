#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchStats {
    pub candidates_seen: usize,
    pub keys_seen: usize,
    pub variants_seen: usize,
    pub fuzzy_calls: usize,
    pub quality_score_calls: usize,
    pub reading_generation_calls: usize,
}
