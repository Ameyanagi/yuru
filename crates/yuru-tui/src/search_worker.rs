use std::sync::{
    mpsc::{self, Receiver},
    Arc,
};
use std::thread;
use std::time::Duration;

use yuru_core::{search, Candidate, LanguageBackend, ScoredCandidate, SearchConfig};

pub(crate) const SEARCH_WORKER_POLL: Duration = Duration::from_millis(16);

/// The search a result set answers.
///
/// Query text alone no longer identifies a search: with live smart case the same text is
/// searched under two different case policies depending on what has been typed, so
/// results for `ab` (case-insensitive) are not results for `abC` (case-sensitive). Any
/// further part of [`SearchConfig`] that starts following the live query belongs here
/// too, so that a result set can always be compared against the live search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SearchIdentity {
    pub(crate) query: String,
    pub(crate) case_sensitive: bool,
}

impl SearchIdentity {
    /// Returns the identity of searching `query` under `config`.
    pub(crate) fn new(query: &str, config: &SearchConfig) -> Self {
        Self {
            query: query.to_string(),
            case_sensitive: config.case_sensitive,
        }
    }
}

struct SearchRequest {
    seq: u64,
    identity: SearchIdentity,
    candidates: Option<Arc<Vec<Candidate>>>,
    config: SearchConfig,
}

pub(crate) struct SearchResponse {
    pub(crate) seq: u64,
    pub(crate) identity: SearchIdentity,
    pub(crate) results: Vec<ScoredCandidate>,
}

enum SearchCommand {
    Append(Vec<Candidate>),
    Search(SearchRequest),
}

pub(crate) struct SearchWorker {
    sender: mpsc::Sender<SearchCommand>,
    receiver: Receiver<SearchResponse>,
}

impl SearchWorker {
    pub(crate) fn new(backend: Arc<dyn LanguageBackend>) -> Self {
        let (request_sender, request_receiver) = mpsc::channel::<SearchCommand>();
        let (response_sender, response_receiver) = mpsc::channel::<SearchResponse>();

        thread::spawn(move || {
            let mut owned_candidates = Vec::new();
            while let Ok(command) = request_receiver.recv() {
                let mut request = None;
                apply_search_command(command, &mut owned_candidates, &mut request);
                while let Ok(command) = request_receiver.try_recv() {
                    apply_search_command(command, &mut owned_candidates, &mut request);
                }

                let Some(request) = request else {
                    continue;
                };

                let query = request.identity.query.as_str();
                let results = if let Some(candidates) = &request.candidates {
                    search(
                        query,
                        candidates.as_ref(),
                        backend.as_ref(),
                        &request.config,
                    )
                } else {
                    search(query, &owned_candidates, backend.as_ref(), &request.config)
                };

                if response_sender
                    .send(SearchResponse {
                        seq: request.seq,
                        identity: request.identity,
                        results,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Self {
            sender: request_sender,
            receiver: response_receiver,
        }
    }

    pub(crate) fn request(
        &mut self,
        seq: u64,
        identity: SearchIdentity,
        candidates: Arc<Vec<Candidate>>,
        config: SearchConfig,
    ) {
        let _ = self.sender.send(SearchCommand::Search(SearchRequest {
            seq,
            identity,
            candidates: Some(candidates),
            config,
        }));
    }

    pub(crate) fn request_owned(
        &mut self,
        seq: u64,
        identity: SearchIdentity,
        config: SearchConfig,
    ) {
        let _ = self.sender.send(SearchCommand::Search(SearchRequest {
            seq,
            identity,
            candidates: None,
            config,
        }));
    }

    pub(crate) fn append(&mut self, candidates: Vec<Candidate>) {
        if !candidates.is_empty() {
            let _ = self.sender.send(SearchCommand::Append(candidates));
        }
    }

    pub(crate) fn try_recv(&mut self) -> Option<SearchResponse> {
        self.receiver.try_recv().ok()
    }
}

fn apply_search_command(
    command: SearchCommand,
    owned_candidates: &mut Vec<Candidate>,
    request: &mut Option<SearchRequest>,
) {
    match command {
        SearchCommand::Append(mut candidates) => owned_candidates.append(&mut candidates),
        SearchCommand::Search(search_request) => *request = Some(search_request),
    }
}

pub(crate) fn request_snapshot_search(
    worker: &mut SearchWorker,
    search_seq: &mut u64,
    latest_requested_seq: &mut u64,
    query: &str,
    candidates: Arc<Vec<Candidate>>,
    config: SearchConfig,
) {
    *search_seq = search_seq.saturating_add(1);
    *latest_requested_seq = *search_seq;
    let identity = SearchIdentity::new(query, &config);
    worker.request(*search_seq, identity, candidates, config);
}

pub(crate) fn request_owned_search(
    worker: &mut SearchWorker,
    search_seq: &mut u64,
    latest_requested_seq: &mut u64,
    query: &str,
    config: SearchConfig,
) {
    *search_seq = search_seq.saturating_add(1);
    *latest_requested_seq = *search_seq;
    let identity = SearchIdentity::new(query, &config);
    worker.request_owned(*search_seq, identity, config);
}
