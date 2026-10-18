//! FIFO queue for a single price level in price-time priority order matching.

use crate::types::{Order, OrderId, Price, Qty};
use std::collections::VecDeque;

/// A single price level in the orderbook maintaining FIFO priority
#[derive(Debug, Clone)]
pub struct PriceLevel {
    pub price: Price,
    pub total_qty: Qty,
    pub orders: VecDeque<Order>,
}

impl PriceLevel {
    pub fn new(price: Price) -> Self {
        Self {
            price,
            total_qty: 0,
            orders: VecDeque::with_capacity(16),
        }
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    #[inline(always)]
    pub fn order_count(&self) -> usize {
        self.orders.len()
    }

    #[inline(always)]
    pub fn append(&mut self, order: Order) {
        self.total_qty = self.total_qty.saturating_add(order.leaves_qty);
        self.orders.push_back(order);
    }

    #[inline(always)]
    pub fn front(&self) -> Option<&Order> {
        self.orders.front()
    }

    #[inline(always)]
    pub fn front_mut(&mut self) -> Option<&mut Order> {
        self.orders.front_mut()
    }

    #[inline(always)]
    pub fn pop_front(&mut self) -> Option<Order> {
        if let Some(order) = self.orders.pop_front() {
            self.total_qty = self.total_qty.saturating_sub(order.leaves_qty);
            Some(order)
        } else {
            None
        }
    }

    /// Reduce the quantity of the head order during a match fill
    #[inline(always)]
    pub fn reduce_head(&mut self, fill_qty: Qty) -> bool {
        if let Some(front) = self.orders.front_mut() {
            front.leaves_qty = front.leaves_qty.saturating_sub(fill_qty);
            front.filled_qty = front.filled_qty.saturating_add(fill_qty);
            self.total_qty = self.total_qty.saturating_sub(fill_qty);
            front.leaves_qty == 0
        } else {
            false
        }
    }

    /// Cancel an order by ID within this level
    pub fn cancel(&mut self, order_id: OrderId) -> Option<Order> {
        if let Some(pos) = self.orders.iter().position(|o| o.id == order_id) {
            let order = self.orders.remove(pos).unwrap();
            self.total_qty = self.total_qty.saturating_sub(order.leaves_qty);
            Some(order)
        } else {
            None
        }
    }

    /// Modify order quantity in-place (if reduced, retains time priority)
    pub fn modify_qty(&mut self, order_id: OrderId, new_qty: Qty) -> Option<(Qty, Qty)> {
        if let Some(order) = self.orders.iter_mut().find(|o| o.id == order_id) {
            let old_leaves = order.leaves_qty;
            if new_qty <= old_leaves {
                let diff = old_leaves - new_qty;
                order.leaves_qty = new_qty;
                self.total_qty = self.total_qty.saturating_sub(diff);
                Some((old_leaves, new_qty))
            } else {
                // If increasing quantity, priority loss handled by caller
                let diff = new_qty - old_leaves;
                order.leaves_qty = new_qty;
                self.total_qty = self.total_qty.saturating_add(diff);
                Some((old_leaves, new_qty))
            }
        } else {
            None
        }
    }

    /// Get position in queue for an order ID
    pub fn queue_position(&self, order_id: OrderId) -> Option<usize> {
        self.orders.iter().position(|o| o.id == order_id)
    }
}
