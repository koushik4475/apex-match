//! Comprehensive pre-trade risk validation engine.
//! Enforces price collars (fat-finger protection), rate limits, margin, and credit bounds.

use crate::risk::credit::CreditManager;
use crate::risk::margin::TraderAccount;
use crate::types::{Order, Price, RejectOrCancelReason, Side, SymbolId, TraderId};
use std::collections::HashMap;

/// Rate limiter state per trader using token bucket algorithm
#[derive(Debug, Clone)]
struct RateBucket {
    tokens: f64,
    capacity: f64,
    refill_rate_per_ns: f64,
    last_update_ns: u64,
}

impl RateBucket {
    fn new(capacity: f64, rate_per_second: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_rate_per_ns: rate_per_second / 1_000_000_000.0,
            last_update_ns: 0,
        }
    }

    fn try_consume(&mut self, timestamp_ns: u64) -> bool {
        if self.last_update_ns > 0 {
            let elapsed_ns = timestamp_ns.saturating_sub(self.last_update_ns);
            let added_tokens = (elapsed_ns as f64) * self.refill_rate_per_ns;
            self.tokens = (self.tokens + added_tokens).min(self.capacity);
        }
        self.last_update_ns = timestamp_ns;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Pre-trade risk validator
pub struct RiskValidator {
    pub accounts: HashMap<TraderId, TraderAccount>,
    pub credit_manager: CreditManager,
    pub price_collar_pct: f64, // E.g. 0.05 for +/- 5%
    pub reference_prices: HashMap<SymbolId, Price>,
    rate_limiters: HashMap<TraderId, RateBucket>,
    rate_limit_capacity: f64,
    rate_limit_per_second: f64,
}

impl Default for RiskValidator {
    fn default() -> Self {
        Self::new(0.05, 50_000.0) // 5% collar, 50k orders/sec
    }
}

impl RiskValidator {
    pub fn new(price_collar_pct: f64, max_orders_per_sec: f64) -> Self {
        Self {
            accounts: HashMap::new(),
            credit_manager: CreditManager::new(),
            price_collar_pct,
            reference_prices: HashMap::new(),
            rate_limiters: HashMap::new(),
            rate_limit_capacity: max_orders_per_sec,
            rate_limit_per_second: max_orders_per_sec,
        }
    }

    pub fn set_reference_price(&mut self, symbol_id: SymbolId, price: Price) {
        self.reference_prices.insert(symbol_id, price);
    }

    pub fn add_account(&mut self, account: TraderAccount) {
        self.accounts.insert(account.trader_id, account);
    }

    /// Primary pre-trade validation gate
    pub fn validate_order(&mut self, order: &Order) -> Result<(), RejectOrCancelReason> {
        // 1. Basic sanity checks
        if order.original_qty == 0 {
            return Err(RejectOrCancelReason::InvalidQuantity);
        }
        if order.price == 0 && order.order_type != crate::types::OrderType::Market {
            return Err(RejectOrCancelReason::InvalidPrice);
        }

        // 2. High-frequency rate limiting (token bucket)
        let rate_limiter = self
            .rate_limiters
            .entry(order.trader_id)
            .or_insert_with(|| {
                RateBucket::new(self.rate_limit_capacity, self.rate_limit_per_second)
            });

        if !rate_limiter.try_consume(order.timestamp_ns) {
            return Err(RejectOrCancelReason::RateLimitThrottled);
        }

        // 3. Price Collar (Fat-Finger Protection)
        if let Some(&ref_price) = self.reference_prices.get(&order.symbol_id) {
            if ref_price > 0 && order.order_type != crate::types::OrderType::Market {
                let max_price = ((ref_price as f64) * (1.0 + self.price_collar_pct)) as Price;
                let min_price = ((ref_price as f64) * (1.0 - self.price_collar_pct)) as Price;

                match order.side {
                    Side::Buy => {
                        if order.price > max_price {
                            return Err(RejectOrCancelReason::PriceCollarViolation);
                        }
                    }
                    Side::Sell => {
                        if order.price < min_price {
                            return Err(RejectOrCancelReason::PriceCollarViolation);
                        }
                    }
                }
            }
        }

        // 4. Credit & Turnover validation
        self.credit_manager.validate(order)?;

        // 5. Margin validation (if account registered)
        if let Some(account) = self.accounts.get(&order.trader_id) {
            account.validate_order_margin(order)?;
        }

        Ok(())
    }

    pub fn record_order_placed(&mut self, trader_id: TraderId) {
        self.credit_manager.on_order_placed(trader_id);
    }

    pub fn record_order_closed(&mut self, trader_id: TraderId) {
        self.credit_manager.on_order_closed(trader_id);
    }
}
