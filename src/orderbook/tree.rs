//! Price-time priority FIFO matching tree.
//! Manages continuous double-auction bids and asks ladders with sub-microsecond execution.

use crate::orderbook::l3::{L3OrderMeta, L3Tracker};
use crate::orderbook::level::PriceLevel;
use crate::types::{
    ExecutionReport, ExecutionStatus, Fill, L2Level, L2Snapshot, L3Entry, L3Snapshot, MatchResult,
    Order, OrderId, OrderType, Price, Qty, RejectOrCancelReason, STPMode, Side, SymbolId, BBO,
};
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum OrderBookError {
    #[error("Order ID {0} already exists in orderbook")]
    DuplicateOrderId(OrderId),
    #[error("Order ID {0} not found")]
    OrderNotFound(OrderId),
    #[error("Invalid quantity {0}")]
    InvalidQuantity(Qty),
    #[error("Invalid price {0}")]
    InvalidPrice(Price),
    #[error("Post-only order would cross existing book at price {0}")]
    PostOnlyWouldCross(Price),
    #[error("Fill-or-Kill order cannot be fully filled")]
    FOKCannotFill,
}

pub struct OrderBook {
    pub symbol_id: SymbolId,
    bids: BTreeMap<Price, PriceLevel>, // Price -> Level (Highest price is at .next_back())
    asks: BTreeMap<Price, PriceLevel>, // Price -> Level (Lowest price is at .next())
    tracker: L3Tracker,
    sequence: u64,
    next_match_id: u64,
}

impl OrderBook {
    pub fn new(symbol_id: SymbolId) -> Self {
        Self {
            symbol_id,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            tracker: L3Tracker::new(),
            sequence: 0,
            next_match_id: 1,
        }
    }

    #[inline(always)]
    pub fn best_bid(&self) -> Option<Price> {
        self.bids.iter().next_back().map(|(&p, _)| p)
    }

    #[inline(always)]
    pub fn best_ask(&self) -> Option<Price> {
        self.asks.iter().next().map(|(&p, _)| p)
    }

    #[inline(always)]
    pub fn bbo(&self) -> BBO {
        let bid = self.bids.iter().next_back().map(|(&p, l)| (p, l.total_qty));
        let ask = self.asks.iter().next().map(|(&p, l)| (p, l.total_qty));
        (bid, ask)
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.tracker.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.tracker.is_empty()
    }

    /// Match an incoming order against the resting orderbook
    pub fn match_order(&mut self, mut order: Order) -> Result<MatchResult, OrderBookError> {
        self.sequence += 1;

        if order.original_qty == 0 {
            return Err(OrderBookError::InvalidQuantity(0));
        }
        if self.tracker.contains(order.id) {
            return Err(OrderBookError::DuplicateOrderId(order.id));
        }

        let mut result = MatchResult::default();

        // Pre-check for Post-Only orders: must not cross opposite book
        if order.is_post_only() {
            let crosses = match order.side {
                Side::Buy => self.best_ask().is_some_and(|ask| order.price >= ask),
                Side::Sell => self.best_bid().is_some_and(|bid| order.price <= bid),
            };
            if crosses {
                result.reports.push(ExecutionReport {
                    order_id: order.id,
                    trader_id: order.trader_id,
                    symbol_id: self.symbol_id,
                    side: order.side,
                    status: ExecutionStatus::Rejected,
                    reason: Some(RejectOrCancelReason::PostOnlyWouldCross),
                    filled_qty: 0,
                    leaves_qty: order.leaves_qty,
                    last_fill_price: None,
                    last_fill_qty: None,
                    avg_price: 0,
                    timestamp_ns: order.timestamp_ns,
                });
                return Ok(result);
            }
        }

        // Pre-check for Fill-Or-Kill (FOK) orders: verify full fill availability
        if order.is_fok() && !self.can_fully_fill(&order) {
            result.reports.push(ExecutionReport {
                order_id: order.id,
                trader_id: order.trader_id,
                symbol_id: self.symbol_id,
                side: order.side,
                status: ExecutionStatus::Cancelled,
                reason: Some(RejectOrCancelReason::FOKCannotFill),
                filled_qty: 0,
                leaves_qty: order.leaves_qty,
                last_fill_price: None,
                last_fill_qty: None,
                avg_price: 0,
                timestamp_ns: order.timestamp_ns,
            });
            return Ok(result);
        }

        // Core matching execution loop
        match order.side {
            Side::Buy => self.match_buy(&mut order, &mut result),
            Side::Sell => self.match_sell(&mut order, &mut result),
        }

        // Post-matching handling based on order type
        if order.leaves_qty > 0 {
            if order.is_ioc() || order.order_type == OrderType::Market {
                // IOC or Market remaining quantity is cancelled
                result.reports.push(ExecutionReport {
                    order_id: order.id,
                    trader_id: order.trader_id,
                    symbol_id: self.symbol_id,
                    side: order.side,
                    status: if order.filled_qty > 0 {
                        ExecutionStatus::PartiallyFilled
                    } else {
                        ExecutionStatus::Cancelled
                    },
                    reason: Some(RejectOrCancelReason::IOCUnfilled),
                    filled_qty: order.filled_qty,
                    leaves_qty: 0,
                    last_fill_price: None,
                    last_fill_qty: None,
                    avg_price: self.calculate_avg_price(&result.fills, order.id),
                    timestamp_ns: order.timestamp_ns,
                });
            } else {
                // Limit or Post-Only order rests on the book
                let meta = L3OrderMeta {
                    order_id: order.id,
                    trader_id: order.trader_id,
                    symbol_id: self.symbol_id,
                    side: order.side,
                    price: order.price,
                    original_qty: order.original_qty,
                    leaves_qty: order.leaves_qty,
                    timestamp_ns: order.timestamp_ns,
                };
                self.tracker.insert(meta);

                let level = match order.side {
                    Side::Buy => self
                        .bids
                        .entry(order.price)
                        .or_insert_with(|| PriceLevel::new(order.price)),
                    Side::Sell => self
                        .asks
                        .entry(order.price)
                        .or_insert_with(|| PriceLevel::new(order.price)),
                };
                level.append(order.clone());
                result.order_placed = true;

                result.reports.push(ExecutionReport {
                    order_id: order.id,
                    trader_id: order.trader_id,
                    symbol_id: self.symbol_id,
                    side: order.side,
                    status: if order.filled_qty > 0 {
                        ExecutionStatus::PartiallyFilled
                    } else {
                        ExecutionStatus::Accepted
                    },
                    reason: None,
                    filled_qty: order.filled_qty,
                    leaves_qty: order.leaves_qty,
                    last_fill_price: None,
                    last_fill_qty: None,
                    avg_price: self.calculate_avg_price(&result.fills, order.id),
                    timestamp_ns: order.timestamp_ns,
                });
            }
        } else {
            // Completely filled
            result.reports.push(ExecutionReport {
                order_id: order.id,
                trader_id: order.trader_id,
                symbol_id: self.symbol_id,
                side: order.side,
                status: ExecutionStatus::Filled,
                reason: None,
                filled_qty: order.filled_qty,
                leaves_qty: 0,
                last_fill_price: result.fills.last().map(|f| f.price),
                last_fill_qty: result.fills.last().map(|f| f.qty),
                avg_price: self.calculate_avg_price(&result.fills, order.id),
                timestamp_ns: order.timestamp_ns,
            });
        }

        Ok(result)
    }

    /// Match an incoming Buy against resting Asks
    fn match_buy(&mut self, taker: &mut Order, result: &mut MatchResult) {
        let is_market = taker.order_type == OrderType::Market;
        let mut empty_levels = Vec::new();

        'level_loop: for (&ask_price, level) in self.asks.iter_mut() {
            if !is_market && taker.price < ask_price {
                break 'level_loop;
            }

            while !level.is_empty() && taker.leaves_qty > 0 {
                let maker = level.front_mut().unwrap();

                // Self-Trade Prevention (STP) check
                if maker.trader_id == taker.trader_id {
                    let stp = if taker.stp_mode != STPMode::None {
                        taker.stp_mode
                    } else {
                        maker.stp_mode
                    };

                    match stp {
                        STPMode::CancelNewest => {
                            result.reports.push(ExecutionReport {
                                order_id: taker.id,
                                trader_id: taker.trader_id,
                                symbol_id: self.symbol_id,
                                side: taker.side,
                                status: ExecutionStatus::Cancelled,
                                reason: Some(RejectOrCancelReason::SelfTradePrevention),
                                filled_qty: taker.filled_qty,
                                leaves_qty: 0,
                                last_fill_price: None,
                                last_fill_qty: None,
                                avg_price: self.calculate_avg_price(&result.fills, taker.id),
                                timestamp_ns: taker.timestamp_ns,
                            });
                            taker.leaves_qty = 0;
                            return;
                        }
                        STPMode::CancelOldest => {
                            let old_maker = level.pop_front().unwrap();
                            self.tracker.remove(old_maker.id);
                            result.cancelled_maker_ids.push(old_maker.id);
                            result.reports.push(ExecutionReport {
                                order_id: old_maker.id,
                                trader_id: old_maker.trader_id,
                                symbol_id: self.symbol_id,
                                side: old_maker.side,
                                status: ExecutionStatus::Cancelled,
                                reason: Some(RejectOrCancelReason::SelfTradePrevention),
                                filled_qty: old_maker.filled_qty,
                                leaves_qty: 0,
                                last_fill_price: None,
                                last_fill_qty: None,
                                avg_price: 0,
                                timestamp_ns: taker.timestamp_ns,
                            });
                            continue;
                        }
                        STPMode::DecrementAndCancel => {
                            let decrement_qty = {
                                let maker = level.front_mut().unwrap();
                                let dec = maker.leaves_qty.min(taker.leaves_qty);
                                maker.leaves_qty -= dec;
                                dec
                            };
                            taker.leaves_qty -= decrement_qty;
                            level.total_qty = level.total_qty.saturating_sub(decrement_qty);

                            let maker_depleted = level.front().is_some_and(|m| m.leaves_qty == 0);
                            if maker_depleted {
                                let old_maker = level.pop_front().unwrap();
                                self.tracker.remove(old_maker.id);
                                result.cancelled_maker_ids.push(old_maker.id);
                            }
                            if taker.leaves_qty == 0 {
                                return;
                            }
                            continue;
                        }
                        STPMode::None => {}
                    }
                }

                // Execute trade fill
                let fill_qty = maker.leaves_qty.min(taker.leaves_qty);
                let match_price = ask_price; // Maker establishes match price
                let match_id = self.next_match_id;
                self.next_match_id += 1;

                let fill = Fill {
                    match_id,
                    maker_order_id: maker.id,
                    taker_order_id: taker.id,
                    maker_trader_id: maker.trader_id,
                    taker_trader_id: taker.trader_id,
                    maker_side: Side::Sell,
                    price: match_price,
                    qty: fill_qty,
                    timestamp_ns: taker.timestamp_ns,
                };
                result.fills.push(fill);

                taker.filled_qty += fill_qty;
                taker.leaves_qty -= fill_qty;

                let maker_id = maker.id;
                let maker_trader_id = maker.trader_id;
                let maker_filled = level.reduce_head(fill_qty);

                if let Some(m) = self.tracker.get_mut(maker_id) {
                    m.leaves_qty = m.leaves_qty.saturating_sub(fill_qty);
                }

                if maker_filled {
                    let completed = level.pop_front().unwrap();
                    self.tracker.remove(completed.id);

                    result.reports.push(ExecutionReport {
                        order_id: maker_id,
                        trader_id: maker_trader_id,
                        symbol_id: self.symbol_id,
                        side: Side::Sell,
                        status: ExecutionStatus::Filled,
                        reason: None,
                        filled_qty: completed.filled_qty,
                        leaves_qty: 0,
                        last_fill_price: Some(match_price),
                        last_fill_qty: Some(fill_qty),
                        avg_price: match_price,
                        timestamp_ns: taker.timestamp_ns,
                    });
                } else {
                    let current_filled = level.front().map_or(0, |o| o.filled_qty);
                    let current_leaves = level.front().map_or(0, |o| o.leaves_qty);
                    result.reports.push(ExecutionReport {
                        order_id: maker_id,
                        trader_id: maker_trader_id,
                        symbol_id: self.symbol_id,
                        side: Side::Sell,
                        status: ExecutionStatus::PartiallyFilled,
                        reason: None,
                        filled_qty: current_filled,
                        leaves_qty: current_leaves,
                        last_fill_price: Some(match_price),
                        last_fill_qty: Some(fill_qty),
                        avg_price: match_price,
                        timestamp_ns: taker.timestamp_ns,
                    });
                }
            }

            if level.is_empty() {
                empty_levels.push(ask_price);
            }

            if taker.leaves_qty == 0 {
                break 'level_loop;
            }
        }

        for price in empty_levels {
            self.asks.remove(&price);
        }
    }

    /// Match an incoming Sell against resting Bids
    fn match_sell(&mut self, taker: &mut Order, result: &mut MatchResult) {
        let is_market = taker.order_type == OrderType::Market;
        let mut empty_levels = Vec::new();

        // Bids traversed descending: highest bid first
        'level_loop: for (&bid_price, level) in self.bids.iter_mut().rev() {
            if !is_market && taker.price > bid_price {
                break 'level_loop;
            }

            while !level.is_empty() && taker.leaves_qty > 0 {
                let maker = level.front_mut().unwrap();

                // Self-Trade Prevention (STP) check
                if maker.trader_id == taker.trader_id {
                    let stp = if taker.stp_mode != STPMode::None {
                        taker.stp_mode
                    } else {
                        maker.stp_mode
                    };

                    match stp {
                        STPMode::CancelNewest => {
                            result.reports.push(ExecutionReport {
                                order_id: taker.id,
                                trader_id: taker.trader_id,
                                symbol_id: self.symbol_id,
                                side: taker.side,
                                status: ExecutionStatus::Cancelled,
                                reason: Some(RejectOrCancelReason::SelfTradePrevention),
                                filled_qty: taker.filled_qty,
                                leaves_qty: 0,
                                last_fill_price: None,
                                last_fill_qty: None,
                                avg_price: self.calculate_avg_price(&result.fills, taker.id),
                                timestamp_ns: taker.timestamp_ns,
                            });
                            taker.leaves_qty = 0;
                            return;
                        }
                        STPMode::CancelOldest => {
                            let old_maker = level.pop_front().unwrap();
                            self.tracker.remove(old_maker.id);
                            result.cancelled_maker_ids.push(old_maker.id);
                            result.reports.push(ExecutionReport {
                                order_id: old_maker.id,
                                trader_id: old_maker.trader_id,
                                symbol_id: self.symbol_id,
                                side: old_maker.side,
                                status: ExecutionStatus::Cancelled,
                                reason: Some(RejectOrCancelReason::SelfTradePrevention),
                                filled_qty: old_maker.filled_qty,
                                leaves_qty: 0,
                                last_fill_price: None,
                                last_fill_qty: None,
                                avg_price: 0,
                                timestamp_ns: taker.timestamp_ns,
                            });
                            continue;
                        }
                        STPMode::DecrementAndCancel => {
                            let decrement_qty = {
                                let maker = level.front_mut().unwrap();
                                let dec = maker.leaves_qty.min(taker.leaves_qty);
                                maker.leaves_qty -= dec;
                                dec
                            };
                            taker.leaves_qty -= decrement_qty;
                            level.total_qty = level.total_qty.saturating_sub(decrement_qty);

                            let maker_depleted = level.front().is_some_and(|m| m.leaves_qty == 0);
                            if maker_depleted {
                                let old_maker = level.pop_front().unwrap();
                                self.tracker.remove(old_maker.id);
                                result.cancelled_maker_ids.push(old_maker.id);
                            }
                            if taker.leaves_qty == 0 {
                                return;
                            }
                            continue;
                        }
                        STPMode::None => {}
                    }
                }

                // Execute trade fill
                let fill_qty = maker.leaves_qty.min(taker.leaves_qty);
                let match_price = bid_price; // Maker establishes match price
                let match_id = self.next_match_id;
                self.next_match_id += 1;

                let fill = Fill {
                    match_id,
                    maker_order_id: maker.id,
                    taker_order_id: taker.id,
                    maker_trader_id: maker.trader_id,
                    taker_trader_id: taker.trader_id,
                    maker_side: Side::Buy,
                    price: match_price,
                    qty: fill_qty,
                    timestamp_ns: taker.timestamp_ns,
                };
                result.fills.push(fill);

                taker.filled_qty += fill_qty;
                taker.leaves_qty -= fill_qty;

                let maker_id = maker.id;
                let maker_trader_id = maker.trader_id;
                let maker_filled = level.reduce_head(fill_qty);

                if let Some(m) = self.tracker.get_mut(maker_id) {
                    m.leaves_qty = m.leaves_qty.saturating_sub(fill_qty);
                }

                if maker_filled {
                    let completed = level.pop_front().unwrap();
                    self.tracker.remove(completed.id);

                    result.reports.push(ExecutionReport {
                        order_id: maker_id,
                        trader_id: maker_trader_id,
                        symbol_id: self.symbol_id,
                        side: Side::Buy,
                        status: ExecutionStatus::Filled,
                        reason: None,
                        filled_qty: completed.filled_qty,
                        leaves_qty: 0,
                        last_fill_price: Some(match_price),
                        last_fill_qty: Some(fill_qty),
                        avg_price: match_price,
                        timestamp_ns: taker.timestamp_ns,
                    });
                } else {
                    let current_filled = level.front().map_or(0, |o| o.filled_qty);
                    let current_leaves = level.front().map_or(0, |o| o.leaves_qty);
                    result.reports.push(ExecutionReport {
                        order_id: maker_id,
                        trader_id: maker_trader_id,
                        symbol_id: self.symbol_id,
                        side: Side::Buy,
                        status: ExecutionStatus::PartiallyFilled,
                        reason: None,
                        filled_qty: current_filled,
                        leaves_qty: current_leaves,
                        last_fill_price: Some(match_price),
                        last_fill_qty: Some(fill_qty),
                        avg_price: match_price,
                        timestamp_ns: taker.timestamp_ns,
                    });
                }
            }

            if level.is_empty() {
                empty_levels.push(bid_price);
            }

            if taker.leaves_qty == 0 {
                break 'level_loop;
            }
        }

        for price in empty_levels {
            self.bids.remove(&price);
        }
    }

    /// Cancel a resting order by ID
    pub fn cancel_order(
        &mut self,
        order_id: OrderId,
        timestamp_ns: u64,
    ) -> Result<ExecutionReport, OrderBookError> {
        self.sequence += 1;
        let meta = self
            .tracker
            .remove(order_id)
            .ok_or(OrderBookError::OrderNotFound(order_id))?;

        let (removed, leaves_qty, filled_qty) = match meta.side {
            Side::Buy => {
                if let Some(level) = self.bids.get_mut(&meta.price) {
                    let order = level.cancel(order_id);
                    let leaves = order.as_ref().map_or(0, |o| o.leaves_qty);
                    let filled = order.as_ref().map_or(0, |o| o.filled_qty);
                    let empty = level.is_empty();
                    if empty {
                        self.bids.remove(&meta.price);
                    }
                    (order.is_some(), leaves, filled)
                } else {
                    (false, 0, 0)
                }
            }
            Side::Sell => {
                if let Some(level) = self.asks.get_mut(&meta.price) {
                    let order = level.cancel(order_id);
                    let leaves = order.as_ref().map_or(0, |o| o.leaves_qty);
                    let filled = order.as_ref().map_or(0, |o| o.filled_qty);
                    let empty = level.is_empty();
                    if empty {
                        self.asks.remove(&meta.price);
                    }
                    (order.is_some(), leaves, filled)
                } else {
                    (false, 0, 0)
                }
            }
        };

        if !removed {
            return Err(OrderBookError::OrderNotFound(order_id));
        }

        Ok(ExecutionReport {
            order_id,
            trader_id: meta.trader_id,
            symbol_id: self.symbol_id,
            side: meta.side,
            status: ExecutionStatus::Cancelled,
            reason: Some(RejectOrCancelReason::UserRequested),
            filled_qty,
            leaves_qty,
            last_fill_price: None,
            last_fill_qty: None,
            avg_price: 0,
            timestamp_ns,
        })
    }

    /// Pre-check if an order can be filled in its entirety (for FOK)
    fn can_fully_fill(&self, order: &Order) -> bool {
        let mut accum_qty = 0;
        match order.side {
            Side::Buy => {
                let is_market = order.order_type == OrderType::Market;
                for (&ask_px, level) in self.asks.iter() {
                    if !is_market && order.price < ask_px {
                        break;
                    }
                    accum_qty += level.total_qty;
                    if accum_qty >= order.leaves_qty {
                        return true;
                    }
                }
            }
            Side::Sell => {
                let is_market = order.order_type == OrderType::Market;
                for (&bid_px, level) in self.bids.iter().rev() {
                    if !is_market && order.price > bid_px {
                        break;
                    }
                    accum_qty += level.total_qty;
                    if accum_qty >= order.leaves_qty {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn calculate_avg_price(&self, fills: &[Fill], order_id: OrderId) -> Price {
        let mut total_notional = 0u128;
        let mut total_qty = 0u64;
        for fill in fills {
            if fill.taker_order_id == order_id || fill.maker_order_id == order_id {
                total_notional += (fill.price as u128) * (fill.qty as u128);
                total_qty += fill.qty;
            }
        }
        if total_qty > 0 {
            (total_notional / (total_qty as u128)) as Price
        } else {
            0
        }
    }

    /// Generate an L2 market depth snapshot
    pub fn get_l2_snapshot(&self, max_depth: usize, timestamp_ns: u64) -> L2Snapshot {
        let bids = self
            .bids
            .iter()
            .rev()
            .take(max_depth)
            .map(|(&price, level)| L2Level {
                price,
                qty: level.total_qty,
                order_count: level.order_count() as u32,
            })
            .collect();

        let asks = self
            .asks
            .iter()
            .take(max_depth)
            .map(|(&price, level)| L2Level {
                price,
                qty: level.total_qty,
                order_count: level.order_count() as u32,
            })
            .collect();

        L2Snapshot {
            symbol_id: self.symbol_id,
            sequence: self.sequence,
            timestamp_ns,
            bids,
            asks,
        }
    }

    /// Generate an L3 full individual order snapshot
    pub fn get_l3_snapshot(&self, timestamp_ns: u64) -> L3Snapshot {
        let mut bids = Vec::new();
        for (&price, level) in self.bids.iter().rev() {
            for (pos, order) in level.orders.iter().enumerate() {
                bids.push(L3Entry {
                    order_id: order.id,
                    trader_id: order.trader_id,
                    side: Side::Buy,
                    price,
                    qty: order.leaves_qty,
                    queue_position: pos,
                    timestamp_ns: order.timestamp_ns,
                });
            }
        }

        let mut asks = Vec::new();
        for (&price, level) in self.asks.iter() {
            for (pos, order) in level.orders.iter().enumerate() {
                asks.push(L3Entry {
                    order_id: order.id,
                    trader_id: order.trader_id,
                    side: Side::Sell,
                    price,
                    qty: order.leaves_qty,
                    queue_position: pos,
                    timestamp_ns: order.timestamp_ns,
                });
            }
        }

        L3Snapshot {
            symbol_id: self.symbol_id,
            sequence: self.sequence,
            timestamp_ns,
            bids,
            asks,
        }
    }
}
