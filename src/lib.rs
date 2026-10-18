//! # apex-match
//!
//! Sub-300ns Ultra-Low-Latency In-Memory Financial Orderbook & Matching Engine.
//!
//! Designed for high-frequency trading (HFT) firms, electronic market makers,
//! and institutional cryptocurrency exchanges. Features price-time priority (FIFO),
//! L2/L3 orderbook depth tracking, lock-free ring buffers, and real-time pre-trade risk management.

pub mod engine;
pub mod ffi;
pub mod orderbook;
pub mod risk;
pub mod types;

pub use engine::{EngineCommand, MatchingEngine, SpscRingBuffer};
pub use orderbook::{OrderBook, PriceLevel};
pub use risk::{CreditManager, RiskValidator, TraderAccount};
pub use types::{
    ExecutionReport, ExecutionStatus, Fill, L2Level, L2Snapshot, L3Entry, L3Snapshot, MatchResult,
    Order, OrderId, OrderType, Price, Qty, RejectOrCancelReason, STPMode, Side, SymbolId,
    TimeInForce, TraderId,
};
