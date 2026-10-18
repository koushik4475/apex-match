//! Core domain types for apex-match ultra-low-latency matching engine.

use serde::{Deserialize, Serialize};
use std::fmt;

pub type OrderId = u64;
pub type Price = u64; // Scaled integer price (e.g. 100_00 = $100.00 with 2 decimals)
pub type Qty = u64; // Fixed units/lots
pub type TraderId = u32;
pub type SymbolId = u32;
pub type BBO = (Option<(Price, Qty)>, Option<(Price, Qty)>);

/// Trading side: Buy (Bid) or Sell (Ask)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Side {
    Buy = 0,
    Sell = 1,
}

impl Side {
    #[inline(always)]
    pub fn opposite(&self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }

    #[inline(always)]
    pub fn is_buy(&self) -> bool {
        matches!(self, Side::Buy)
    }

    #[inline(always)]
    pub fn is_sell(&self) -> bool {
        matches!(self, Side::Sell)
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Buy => write!(f, "BUY"),
            Side::Sell => write!(f, "SELL"),
        }
    }
}

/// Execution order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum OrderType {
    Limit = 0,
    Market = 1,
    PostOnly = 2,
    ImmediateOrCancel = 3,
    FillOrKill = 4,
}

/// Time In Force instructions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum TimeInForce {
    GTC = 0, // Good 'Til Cancelled
    IOC = 1, // Immediate Or Cancel
    FOK = 2, // Fill Or Kill
    Day = 3, // Good For Day
}

/// Self-Trade Prevention (STP) policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum STPMode {
    None = 0,
    CancelOldest = 1,       // Rest (maker) order is cancelled
    CancelNewest = 2,       // Aggressor (taker) order is cancelled
    DecrementAndCancel = 3, // Decrement overlapping quantity, cancel smaller
}

/// Execution report lifecycle status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum ExecutionStatus {
    Accepted = 0,
    Filled = 1,
    PartiallyFilled = 2,
    Cancelled = 3,
    Rejected = 4,
    Expired = 5,
}

/// Reason for rejection or cancellation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RejectOrCancelReason {
    UserRequested,
    IOCUnfilled,
    FOKCannotFill,
    PostOnlyWouldCross,
    SelfTradePrevention,
    PriceCollarViolation,
    RiskLimitExceeded,
    OrderNotFound,
    DuplicateOrderId,
    InvalidQuantity,
    InvalidPrice,
    InsufficientMargin,
    PositionLimitExceeded,
    RateLimitThrottled,
}

impl fmt::Display for RejectOrCancelReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// In-memory Order representation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub trader_id: TraderId,
    pub symbol_id: SymbolId,
    pub side: Side,
    pub order_type: OrderType,
    pub tif: TimeInForce,
    pub stp_mode: STPMode,
    pub price: Price,
    pub original_qty: Qty,
    pub leaves_qty: Qty,
    pub filled_qty: Qty,
    pub timestamp_ns: u64,
}

impl Order {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: OrderId,
        trader_id: TraderId,
        symbol_id: SymbolId,
        side: Side,
        order_type: OrderType,
        tif: TimeInForce,
        stp_mode: STPMode,
        price: Price,
        qty: Qty,
        timestamp_ns: u64,
    ) -> Self {
        Self {
            id,
            trader_id,
            symbol_id,
            side,
            order_type,
            tif,
            stp_mode,
            price,
            original_qty: qty,
            leaves_qty: qty,
            filled_qty: 0,
            timestamp_ns,
        }
    }

    #[inline(always)]
    pub fn is_filled(&self) -> bool {
        self.leaves_qty == 0
    }

    #[inline(always)]
    pub fn is_ioc(&self) -> bool {
        self.order_type == OrderType::ImmediateOrCancel || self.tif == TimeInForce::IOC
    }

    #[inline(always)]
    pub fn is_fok(&self) -> bool {
        self.order_type == OrderType::FillOrKill || self.tif == TimeInForce::FOK
    }

    #[inline(always)]
    pub fn is_post_only(&self) -> bool {
        self.order_type == OrderType::PostOnly
    }
}

/// Trade execution match record
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fill {
    pub match_id: u64,
    pub maker_order_id: OrderId,
    pub taker_order_id: OrderId,
    pub maker_trader_id: TraderId,
    pub taker_trader_id: TraderId,
    pub maker_side: Side,
    pub price: Price,
    pub qty: Qty,
    pub timestamp_ns: u64,
}

/// Comprehensive execution report for order lifecycle tracking
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub order_id: OrderId,
    pub trader_id: TraderId,
    pub symbol_id: SymbolId,
    pub side: Side,
    pub status: ExecutionStatus,
    pub reason: Option<RejectOrCancelReason>,
    pub filled_qty: Qty,
    pub leaves_qty: Qty,
    pub last_fill_price: Option<Price>,
    pub last_fill_qty: Option<Qty>,
    pub avg_price: Price,
    pub timestamp_ns: u64,
}

/// Outcome of matching an incoming order
#[derive(Debug, Default, Clone)]
pub struct MatchResult {
    pub fills: Vec<Fill>,
    pub reports: Vec<ExecutionReport>,
    pub cancelled_maker_ids: Vec<OrderId>,
    pub order_placed: bool,
}

/// Single L2 price level representation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct L2Level {
    pub price: Price,
    pub qty: Qty,
    pub order_count: u32,
}

/// L2 Full Depth Snapshot
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct L2Snapshot {
    pub symbol_id: SymbolId,
    pub sequence: u64,
    pub timestamp_ns: u64,
    pub bids: Vec<L2Level>,
    pub asks: Vec<L2Level>,
}

/// Single L3 order entry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct L3Entry {
    pub order_id: OrderId,
    pub trader_id: TraderId,
    pub side: Side,
    pub price: Price,
    pub qty: Qty,
    pub queue_position: usize,
    pub timestamp_ns: u64,
}

/// L3 Orderbook Depth Snapshot
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct L3Snapshot {
    pub symbol_id: SymbolId,
    pub sequence: u64,
    pub timestamp_ns: u64,
    pub bids: Vec<L3Entry>,
    pub asks: Vec<L3Entry>,
}
