//! L3 Order tracking providing individual order-level visibility and queue position.

use crate::types::{OrderId, Price, Qty, Side, SymbolId, TraderId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metadata for locating and inspecting an order in the L3 book
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct L3OrderMeta {
    pub order_id: OrderId,
    pub trader_id: TraderId,
    pub symbol_id: SymbolId,
    pub side: Side,
    pub price: Price,
    pub original_qty: Qty,
    pub leaves_qty: Qty,
    pub timestamp_ns: u64,
}

/// L3 Order Index for O(1) order location and fast queue tracking
#[derive(Debug, Default, Clone)]
pub struct L3Tracker {
    orders: HashMap<OrderId, L3OrderMeta>,
}

impl L3Tracker {
    pub fn new() -> Self {
        Self {
            orders: HashMap::with_capacity(100_000),
        }
    }

    #[inline(always)]
    pub fn insert(&mut self, meta: L3OrderMeta) {
        self.orders.insert(meta.order_id, meta);
    }

    #[inline(always)]
    pub fn get(&self, order_id: OrderId) -> Option<&L3OrderMeta> {
        self.orders.get(&order_id)
    }

    #[inline(always)]
    pub fn get_mut(&mut self, order_id: OrderId) -> Option<&mut L3OrderMeta> {
        self.orders.get_mut(&order_id)
    }

    #[inline(always)]
    pub fn remove(&mut self, order_id: OrderId) -> Option<L3OrderMeta> {
        self.orders.remove(&order_id)
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.orders.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    #[inline(always)]
    pub fn contains(&self, order_id: OrderId) -> bool {
        self.orders.contains_key(&order_id)
    }
}
