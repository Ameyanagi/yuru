use anyhow::{bail, Result};
use yuru_core::{dedup_and_limit_keys, SearchConfig, SearchKey};

use crate::fields::InputItem;

pub(crate) fn apply_aliases(
    candidates: &mut [yuru_core::Candidate],
    items: &[InputItem],
    aliases: &[String],
    config: &SearchConfig,
) -> Result<()> {
    for alias in aliases {
        let Some((query, display)) = alias.split_once('=') else {
            bail!("alias must use query=display format: {alias}");
        };
        if let Some(candidate) = candidates.iter_mut().find(|candidate| {
            let item = &items[candidate.id];
            item.original == display || item.display == display || candidate.display == display
        }) {
            candidate.keys.push(SearchKey::learned_alias(query));
            candidate.keys = dedup_and_limit_keys(std::mem::take(&mut candidate.keys), config);
        }
    }
    Ok(())
}

pub(crate) fn apply_aliases_to_candidate(
    candidate: &mut yuru_core::Candidate,
    item: &InputItem,
    aliases: &[String],
    config: &SearchConfig,
) -> Result<()> {
    for alias in aliases {
        let Some((query, display)) = alias.split_once('=') else {
            bail!("alias must use query=display format: {alias}");
        };
        if item.original == display || item.display == display || candidate.display == display {
            candidate.keys.push(SearchKey::learned_alias(query));
        }
    }
    candidate.keys = dedup_and_limit_keys(std::mem::take(&mut candidate.keys), config);
    Ok(())
}
