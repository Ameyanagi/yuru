use std::sync::Arc;

use yuru_core::{LanguageBackend, SearchConfig};

use crate::search_worker::{SearchIdentity, SearchWorker};

use super::helpers::wait_for_search_response;

#[test]
fn search_worker_searches_owned_streamed_candidates() {
    let backend: Arc<dyn LanguageBackend> = Arc::new(yuru_core::PlainBackend);
    let config = SearchConfig::default();
    let candidate = yuru_core::build_candidate(0, "alpha.txt", backend.as_ref(), &config);
    let mut worker = SearchWorker::new(backend);

    worker.append(vec![candidate]);
    worker.request_owned(1, SearchIdentity::new("alp", &config), config.clone());

    let response = wait_for_search_response(&mut worker);
    assert_eq!(response.seq, 1);
    assert_eq!(response.identity, SearchIdentity::new("alp", &config));
    assert_eq!(
        response
            .results
            .first()
            .map(|result| result.display.as_str()),
        Some("alpha.txt")
    );
}

#[test]
fn search_responses_carry_the_case_policy_they_were_computed_under() {
    let backend: Arc<dyn LanguageBackend> = Arc::new(yuru_core::PlainBackend);
    let insensitive = SearchConfig {
        case_sensitive: false,
        ..SearchConfig::default()
    };
    let sensitive = SearchConfig {
        case_sensitive: true,
        ..SearchConfig::default()
    };
    let candidate = yuru_core::build_candidate(0, "ABC-match", backend.as_ref(), &insensitive);
    let mut worker = SearchWorker::new(backend);
    worker.append(vec![candidate]);

    // Same query text, two different policies: the tag has to tell the responses apart,
    // because the results do not correspond to the same live query state.
    worker.request_owned(
        1,
        SearchIdentity::new("abc", &insensitive),
        insensitive.clone(),
    );
    let loose = wait_for_search_response(&mut worker);
    worker.request_owned(2, SearchIdentity::new("abc", &sensitive), sensitive.clone());
    let strict = wait_for_search_response(&mut worker);

    assert_eq!(loose.identity, SearchIdentity::new("abc", &insensitive));
    assert_eq!(strict.identity, SearchIdentity::new("abc", &sensitive));
    assert_ne!(loose.identity, strict.identity);
    assert_eq!(loose.identity.query, strict.identity.query);
    assert_eq!(loose.results.len(), 1);
    assert!(strict.results.is_empty());
}
