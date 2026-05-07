use std::sync::{
    mpsc::{self, Receiver},
    Arc,
};
use std::thread;
use std::time::Duration;

use yuru_core::{search, Candidate, LanguageBackend, ScoredCandidate, SearchConfig};

pub(crate) const SEARCH_WORKER_POLL: Duration = Duration::from_millis(16);

struct SearchRequest {
    seq: u64,
    query: String,
    candidates: Option<Arc<Vec<Candidate>>>,
    config: SearchConfig,
}

pub(crate) struct SearchResponse {
    pub(crate) seq: u64,
    pub(crate) query: String,
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

                let results = if let Some(candidates) = &request.candidates {
                    search(
                        &request.query,
                        candidates.as_ref(),
                        backend.as_ref(),
                        &request.config,
                    )
                } else {
                    search(
                        &request.query,
                        &owned_candidates,
                        backend.as_ref(),
                        &request.config,
                    )
                };

                if response_sender
                    .send(SearchResponse {
                        seq: request.seq,
                        query: request.query,
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
        query: String,
        candidates: Arc<Vec<Candidate>>,
        config: SearchConfig,
    ) {
        let _ = self.sender.send(SearchCommand::Search(SearchRequest {
            seq,
            query,
            candidates: Some(candidates),
            config,
        }));
    }

    pub(crate) fn request_owned(&mut self, seq: u64, query: String, config: SearchConfig) {
        let _ = self.sender.send(SearchCommand::Search(SearchRequest {
            seq,
            query,
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
    worker.request(*search_seq, query.to_string(), candidates, config);
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
    worker.request_owned(*search_seq, query.to_string(), config);
}
