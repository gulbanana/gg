//! Progress reporting implementation for web mode - broadcasts progress to open tabs as SSE.

use crate::worker::EventSink;
use std::sync::Arc;
use tokio::sync::broadcast;

pub type SseEvent = (String, serde_json::Value);

pub struct SseSink {
    tx: broadcast::Sender<SseEvent>,
}

impl SseSink {
    pub fn new(tx: broadcast::Sender<SseEvent>) -> Arc<Self> {
        Arc::new(Self { tx })
    }
}

impl EventSink for SseSink {
    fn send(&self, event_name: &str, payload: serde_json::Value) {
        let _ = self.tx.send((event_name.to_owned(), payload)); // fails if there aren't any subscribers, and it's best-effort anyway
    }
}
