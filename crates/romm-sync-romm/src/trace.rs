use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceEvent {
    pub method: &'static str,
    /// Route template only. Query strings, IDs, bodies, and credentials are
    /// deliberately excluded.
    pub route: &'static str,
    pub status: Option<u16>,
}

pub trait TraceSink: Send + Sync {
    fn record(&self, event: TraceEvent);
}

#[derive(Debug, Default)]
pub struct NoopTrace;

impl TraceSink for NoopTrace {
    fn record(&self, _event: TraceEvent) {}
}

#[derive(Clone, Debug, Default)]
pub struct MemoryTrace {
    events: Arc<Mutex<Vec<TraceEvent>>>,
}

impl MemoryTrace {
    #[must_use]
    pub fn events(&self) -> Vec<TraceEvent> {
        self.events
            .lock()
            .map_or_else(|_| Vec::new(), |events| events.clone())
    }
}

impl TraceSink for MemoryTrace {
    fn record(&self, event: TraceEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }
}
