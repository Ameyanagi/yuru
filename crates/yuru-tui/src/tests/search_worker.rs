use std::sync::Arc;

use yuru_core::{LanguageBackend, SearchConfig};

use crate::search_worker::SearchWorker;

use super::helpers::wait_for_search_response;

#[test]
fn search_worker_searches_owned_streamed_candidates() {
    let backend: Arc<dyn LanguageBackend> = Arc::new(yuru_core::PlainBackend);
    let config = SearchConfig::default();
    let candidate = yuru_core::build_candidate(0, "alpha.txt", backend.as_ref(), &config);
    let mut worker = SearchWorker::new(backend);

    worker.append(vec![candidate]);
    worker.request_owned(1, "alp".to_string(), config);

    let response = wait_for_search_response(&mut worker);
    assert_eq!(response.seq, 1);
    assert_eq!(response.query, "alp");
    assert_eq!(
        response
            .results
            .first()
            .map(|result| result.display.as_str()),
        Some("alpha.txt")
    );
}
