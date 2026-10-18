//! Event broadcasting infrastructure for market data and trade execution reports.

use crate::types::{ExecutionReport, Fill, L2Snapshot};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

/// Global event stream envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineEvent {
    Trade(Fill),
    Execution(ExecutionReport),
    BookSnapshot(L2Snapshot),
}

/// Handler trait for event stream consumers
pub trait EventListener: Send + Sync {
    fn on_event(&self, event: &EngineEvent);
}

/// Zero-alloc fast event broadcaster
pub struct EventBroadcaster {
    listeners: RwLock<Vec<Arc<dyn EventListener>>>,
    sequence: AtomicU64,
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBroadcaster {
    pub fn new() -> Self {
        Self {
            listeners: RwLock::new(Vec::new()),
            sequence: AtomicU64::new(0),
        }
    }

    pub fn subscribe(&self, listener: Arc<dyn EventListener>) {
        if let Ok(mut w) = self.listeners.write() {
            w.push(listener);
        }
    }

    #[inline(always)]
    pub fn broadcast(&self, event: EngineEvent) {
        self.sequence.fetch_add(1, Ordering::Relaxed);
        if let Ok(listeners) = self.listeners.read() {
            for listener in listeners.iter() {
                listener.on_event(&event);
            }
        }
    }

    #[inline(always)]
    pub fn broadcast_trade(&self, fill: Fill) {
        self.broadcast(EngineEvent::Trade(fill));
    }

    #[inline(always)]
    pub fn broadcast_report(&self, report: ExecutionReport) {
        self.broadcast(EngineEvent::Execution(report));
    }

    #[inline(always)]
    pub fn broadcast_snapshot(&self, snapshot: L2Snapshot) {
        self.broadcast(EngineEvent::BookSnapshot(snapshot));
    }
}

/// In-memory queue collector for testing and validation
pub struct TestEventCollector {
    pub trades: Mutex<Vec<Fill>>,
    pub reports: Mutex<Vec<ExecutionReport>>,
}

impl Default for TestEventCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl TestEventCollector {
    pub fn new() -> Self {
        Self {
            trades: Mutex::new(Vec::new()),
            reports: Mutex::new(Vec::new()),
        }
    }
}

impl EventListener for TestEventCollector {
    fn on_event(&self, event: &EngineEvent) {
        match event {
            EngineEvent::Trade(fill) => {
                if let Ok(mut t) = self.trades.lock() {
                    t.push(fill.clone());
                }
            }
            EngineEvent::Execution(rep) => {
                if let Ok(mut r) = self.reports.lock() {
                    r.push(rep.clone());
                }
            }
            EngineEvent::BookSnapshot(_) => {}
        }
    }
}
