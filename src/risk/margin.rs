//! Pre-trade margin calculations and portfolio equity validation.

use crate::types::{Order, Price, Qty, RejectOrCancelReason, Side, SymbolId, TraderId};
use std::collections::HashMap;

/// Margin requirements and position tracking per trading account
#[derive(Debug, Clone)]
pub struct TraderAccount {
    pub trader_id: TraderId,
    pub cash_balance: i64,                 // In cents / smallest currency unit
    pub positions: HashMap<SymbolId, i64>, // Symbol -> Net position (units)
    pub avg_entry_prices: HashMap<SymbolId, Price>,
    pub initial_margin_rate: f64,     // E.g. 0.10 for 10x leverage
    pub maintenance_margin_rate: f64, // E.g. 0.05
}

impl TraderAccount {
    pub fn new(trader_id: TraderId, initial_cash: i64, im_rate: f64, mm_rate: f64) -> Self {
        Self {
            trader_id,
            cash_balance: initial_cash,
            positions: HashMap::new(),
            avg_entry_prices: HashMap::new(),
            initial_margin_rate: im_rate,
            maintenance_margin_rate: mm_rate,
        }
    }

    pub fn net_position(&self, symbol_id: SymbolId) -> i64 {
        self.positions.get(&symbol_id).copied().unwrap_or(0)
    }

    /// Calculate required initial margin for an incoming order
    pub fn calculate_order_margin(&self, price: Price, qty: Qty) -> i64 {
        let notional = (price as f64) * (qty as f64);
        (notional * self.initial_margin_rate) as i64
    }

    /// Validate whether account has sufficient free margin for incoming order
    pub fn validate_order_margin(&self, order: &Order) -> Result<(), RejectOrCancelReason> {
        let required_margin = self.calculate_order_margin(order.price, order.leaves_qty);

        // Calculate current equity and used margin
        let used_margin = self.calculate_total_used_margin();
        let free_margin = self.cash_balance - used_margin;

        if free_margin < required_margin {
            return Err(RejectOrCancelReason::InsufficientMargin);
        }

        Ok(())
    }

    /// Total margin locked in open positions
    pub fn calculate_total_used_margin(&self) -> i64 {
        let mut total = 0i64;
        for (&symbol, &pos) in &self.positions {
            if pos != 0 {
                let entry_px = self.avg_entry_prices.get(&symbol).copied().unwrap_or(0);
                let notional = (pos.unsigned_abs() as f64) * (entry_px as f64);
                total += (notional * self.initial_margin_rate) as i64;
            }
        }
        total
    }

    /// Update position and cash when a trade fill occurs
    pub fn apply_fill(
        &mut self,
        symbol_id: SymbolId,
        side: Side,
        fill_price: Price,
        fill_qty: Qty,
    ) {
        let current_pos = self.net_position(symbol_id);
        let signed_fill = match side {
            Side::Buy => fill_qty as i64,
            Side::Sell => -(fill_qty as i64),
        };

        let new_pos = current_pos + signed_fill;
        let notional = (fill_price as i64) * (fill_qty as i64);

        if side.is_buy() {
            self.cash_balance -= notional;
        } else {
            self.cash_balance += notional;
        }

        if new_pos == 0 {
            self.positions.remove(&symbol_id);
            self.avg_entry_prices.remove(&symbol_id);
        } else {
            self.positions.insert(symbol_id, new_pos);
            self.avg_entry_prices.insert(symbol_id, fill_price);
        }
    }
}
