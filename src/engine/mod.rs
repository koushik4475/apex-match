//! Engine module: Lock-free ring buffer, matching engine orchestration, and trade event broadcasting.

pub mod broadcast;
pub mod matching_engine;
pub mod ring_buffer;

pub use broadcast::{EngineEvent, EventBroadcaster, EventListener, TestEventCollector};
pub use matching_engine::{EngineCommand, EngineMetrics, MatchingEngine};
pub use ring_buffer::{make_spsc_ring, Consumer, Producer, SpscRingBuffer};
