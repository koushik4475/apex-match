//! Credit limits, turnover caps, and order size thresholds.

use crate::types::{Order, Price, Qty, RejectOrCancelReason, TraderId};
use std::collections::HashMap;

/// Pre-trade credit and risk parameters per trader
#[derive(Debug, Clone)]
pub struct TraderRiskLimits {
    pub trader_id: TraderId,
    pub max_order_qty: Qty,
    pub max_order_notional: u64,
    pub max_daily_turnover: u64,
    pub current_daily_turnover: u64,
    pub max_open_orders: usize,
    pub current_open_orders: usize,
}

impl TraderRiskLimits {
    pub fn new(
        trader_id: TraderId,
        max_qty: Qty,
        max_notional: u64,
        max_turnover: u64,
        max_open: usize,
    ) -> Self {
        Self {
            trader_id,
            max_order_qty: max_qty,
            max_order_notional: max_notional,
            max_daily_turnover: max_turnover,
            current_daily_turnover: 0,
            max_open_orders: max_open,
            current_open_orders: 0,
        }
    }

    /// Validate order against trader specific limits
    pub fn validate_order(&self, order: &Order) -> Result<(), RejectOrCancelReason> {
        if order.original_qty > self.max_order_qty {
            return Err(RejectOrCancelReason::RiskLimitExceeded);
        }

        let notional = order.price.saturating_mul(order.original_qty);
        if notional > self.max_order_notional {
            return Err(RejectOrCancelReason::RiskLimitExceeded);
        }

        if self.current_daily_turnover.saturating_add(notional) > self.max_daily_turnover {
            return Err(RejectOrCancelReason::RiskLimitExceeded);
        }

        if self.current_open_orders >= self.max_open_orders {
            return Err(RejectOrCancelReason::PositionLimitExceeded);
        }

        Ok(())
    }

    pub fn record_fill(&mut self, price: Price, qty: Qty) {
        let notional = price.saturating_mul(qty);
        self.current_daily_turnover = self.current_daily_turnover.saturating_add(notional);
    }
}

/// Global registry of trader risk parameters
#[derive(Debug, Default, Clone)]
pub struct CreditManager {
    limits: HashMap<TraderId, TraderRiskLimits>,
}

impl CreditManager {
    pub fn new() -> Self {
        Self {
            limits: HashMap::new(),
        }
    }

    pub fn register(&mut self, limits: TraderRiskLimits) {
        self.limits.insert(limits.trader_id, limits);
    }

    pub fn get_or_create_default(&mut self, trader_id: TraderId) -> &mut TraderRiskLimits {
        self.limits.entry(trader_id).or_insert_with(|| {
            TraderRiskLimits::new(
                trader_id,
                1_000_000,         // 1M lots
                100_000_000_000,   // $1B notional
                1_000_000_000_000, // $10B daily turnover
                10_000,            // 10,000 open orders
            )
        })
    }

    pub fn validate(&mut self, order: &Order) -> Result<(), RejectOrCancelReason> {
        let limits = self.get_or_create_default(order.trader_id);
        limits.validate_order(order)
    }

    pub fn on_order_placed(&mut self, trader_id: TraderId) {
        if let Some(l) = self.limits.get_mut(&trader_id) {
            l.current_open_orders = l.current_open_orders.saturating_add(1);
        }
    }

    pub fn on_order_closed(&mut self, trader_id: TraderId) {
        if let Some(l) = self.limits.get_mut(&trader_id) {
            l.current_open_orders = l.current_open_orders.saturating_sub(1);
        }
    }
}
