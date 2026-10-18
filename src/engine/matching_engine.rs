//! Multi-symbol High-Frequency Matching Engine.
//! Glues lock-free ring buffer command queue, pre-trade risk checks, price-time matching, and trade broadcast.

use crate::engine::broadcast::EventBroadcaster;
use crate::engine::ring_buffer::{make_spsc_ring, Consumer, Producer};
use crate::orderbook::tree::{OrderBook, OrderBookError};
use crate::risk::validator::RiskValidator;
use crate::types::{
    ExecutionReport, ExecutionStatus, L2Snapshot, L3Snapshot, MatchResult, Order, OrderId,
    RejectOrCancelReason, SymbolId, BBO,
};
use std::collections::HashMap;
use std::sync::Arc;

/// Commands submitted to the matching engine ring buffer
#[derive(Debug, Clone)]
pub enum EngineCommand {
    Submit(Order),
    Cancel {
        symbol_id: SymbolId,
        order_id: OrderId,
        timestamp_ns: u64,
    },
    SnapshotRequest {
        symbol_id: SymbolId,
        max_depth: usize,
        timestamp_ns: u64,
    },
}

/// Statistics on engine operations
#[derive(Debug, Default, Clone)]
pub struct EngineMetrics {
    pub orders_processed: u64,
    pub orders_matched: u64,
    pub orders_cancelled: u64,
    pub orders_rejected: u64,
    pub trades_executed: u64,
    pub total_volume_traded: u64,
}

/// Core Matching Engine
pub struct MatchingEngine {
    orderbooks: HashMap<SymbolId, OrderBook>,
    pub risk_validator: RiskValidator,
    pub broadcaster: Arc<EventBroadcaster>,
    pub command_producer: Producer<EngineCommand>,
    pub command_consumer: Consumer<EngineCommand>,
    pub metrics: EngineMetrics,
}

impl MatchingEngine {
    pub fn new(ring_buffer_capacity: usize) -> Self {
        let (producer, consumer) = make_spsc_ring(ring_buffer_capacity);
        Self {
            orderbooks: HashMap::new(),
            risk_validator: RiskValidator::default(),
            broadcaster: Arc::new(EventBroadcaster::new()),
            command_producer: producer,
            command_consumer: consumer,
            metrics: EngineMetrics::default(),
        }
    }

    /// Register a trading instrument symbol
    pub fn register_symbol(&mut self, symbol_id: SymbolId) {
        self.orderbooks
            .entry(symbol_id)
            .or_insert_with(|| OrderBook::new(symbol_id));
    }

    /// Submit order directly (synchronous fast-path)
    pub fn submit_order_direct(
        &mut self,
        order: Order,
    ) -> Result<MatchResult, RejectOrCancelReason> {
        self.metrics.orders_processed += 1;

        // 1. Run Pre-trade Risk Validation
        if let Err(reason) = self.risk_validator.validate_order(&order) {
            self.metrics.orders_rejected += 1;
            let report = ExecutionReport {
                order_id: order.id,
                trader_id: order.trader_id,
                symbol_id: order.symbol_id,
                side: order.side,
                status: ExecutionStatus::Rejected,
                reason: Some(reason),
                filled_qty: 0,
                leaves_qty: order.leaves_qty,
                last_fill_price: None,
                last_fill_qty: None,
                avg_price: 0,
                timestamp_ns: order.timestamp_ns,
            };
            self.broadcaster.broadcast_report(report);
            return Err(reason);
        }

        // 2. Locate or create orderbook for the instrument
        let book = self
            .orderbooks
            .entry(order.symbol_id)
            .or_insert_with(|| OrderBook::new(order.symbol_id));

        // 3. Execute matching against the orderbook
        let trader_id = order.trader_id;
        match book.match_order(order) {
            Ok(match_res) => {
                if match_res.order_placed {
                    self.risk_validator.record_order_placed(trader_id);
                }

                // Broadcast fills and update risk limits/metrics
                for fill in &match_res.fills {
                    self.metrics.trades_executed += 1;
                    self.metrics.total_volume_traded += fill.qty;
                    self.broadcaster.broadcast_trade(fill.clone());
                }

                // Broadcast execution reports
                for report in &match_res.reports {
                    if report.status == ExecutionStatus::Filled
                        || report.status == ExecutionStatus::Cancelled
                    {
                        self.risk_validator.record_order_closed(report.trader_id);
                    }
                    self.broadcaster.broadcast_report(report.clone());
                }

                self.metrics.orders_matched += 1;
                Ok(match_res)
            }
            Err(e) => {
                self.metrics.orders_rejected += 1;
                let reason = match e {
                    OrderBookError::DuplicateOrderId(_) => RejectOrCancelReason::DuplicateOrderId,
                    OrderBookError::OrderNotFound(_) => RejectOrCancelReason::OrderNotFound,
                    OrderBookError::InvalidQuantity(_) => RejectOrCancelReason::InvalidQuantity,
                    OrderBookError::InvalidPrice(_) => RejectOrCancelReason::InvalidPrice,
                    OrderBookError::PostOnlyWouldCross(_) => {
                        RejectOrCancelReason::PostOnlyWouldCross
                    }
                    OrderBookError::FOKCannotFill => RejectOrCancelReason::FOKCannotFill,
                };
                Err(reason)
            }
        }
    }

    /// Cancel order directly (synchronous fast-path)
    pub fn cancel_order_direct(
        &mut self,
        symbol_id: SymbolId,
        order_id: OrderId,
        timestamp_ns: u64,
    ) -> Result<ExecutionReport, RejectOrCancelReason> {
        let book = self
            .orderbooks
            .get_mut(&symbol_id)
            .ok_or(RejectOrCancelReason::OrderNotFound)?;

        match book.cancel_order(order_id, timestamp_ns) {
            Ok(report) => {
                self.metrics.orders_cancelled += 1;
                self.risk_validator.record_order_closed(report.trader_id);
                self.broadcaster.broadcast_report(report.clone());
                Ok(report)
            }
            Err(_) => {
                self.metrics.orders_rejected += 1;
                Err(RejectOrCancelReason::OrderNotFound)
            }
        }
    }

    /// Process commands pending in the lock-free ring buffer
    pub fn poll_ring_buffer(&mut self, max_batch: usize) -> usize {
        let mut count = 0;
        while count < max_batch {
            if let Some(cmd) = self.command_consumer.try_pop() {
                match cmd {
                    EngineCommand::Submit(order) => {
                        let _ = self.submit_order_direct(order);
                    }
                    EngineCommand::Cancel {
                        symbol_id,
                        order_id,
                        timestamp_ns,
                    } => {
                        let _ = self.cancel_order_direct(symbol_id, order_id, timestamp_ns);
                    }
                    EngineCommand::SnapshotRequest {
                        symbol_id,
                        max_depth,
                        timestamp_ns,
                    } => {
                        if let Some(book) = self.orderbooks.get(&symbol_id) {
                            let snapshot = book.get_l2_snapshot(max_depth, timestamp_ns);
                            self.broadcaster.broadcast_snapshot(snapshot);
                        }
                    }
                }
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    /// Retrieve L2 snapshot for a symbol
    pub fn get_l2_snapshot(
        &self,
        symbol_id: SymbolId,
        max_depth: usize,
        timestamp_ns: u64,
    ) -> Option<L2Snapshot> {
        self.orderbooks
            .get(&symbol_id)
            .map(|b| b.get_l2_snapshot(max_depth, timestamp_ns))
    }

    /// Retrieve L3 snapshot for a symbol
    pub fn get_l3_snapshot(&self, symbol_id: SymbolId, timestamp_ns: u64) -> Option<L3Snapshot> {
        self.orderbooks
            .get(&symbol_id)
            .map(|b| b.get_l3_snapshot(timestamp_ns))
    }

    /// Retrieve Best Bid and Offer (BBO) for a symbol
    pub fn get_bbo(&self, symbol_id: SymbolId) -> Option<BBO> {
        self.orderbooks.get(&symbol_id).map(|b| b.bbo())
    }
}
