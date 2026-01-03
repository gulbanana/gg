//! Progress reporting implementation for GUI mode - sends events to a Tauri window by emitting events.

use crate::worker::EventSink;
use std::sync::Arc;
use tauri::{Emitter, EventTarget, Window};

pub struct TauriSink {
    window: Window,
}

impl TauriSink {
    pub fn new(window: Window) -> Arc<Self> {
        Arc::new(Self { window })
    }
}

impl EventSink for TauriSink {
    fn send(&self, event_name: &str, payload: serde_json::Value) {
        let _ = self.window.emit_to(
            EventTarget::labeled(self.window.label()),
            event_name,
            payload,
        );
    }
}
