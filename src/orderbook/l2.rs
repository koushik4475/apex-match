//! L2 Price level aggregation and market depth snapshotting.

use crate::types::{Price, Qty, Side, SymbolId};
use serde::{Deserialize, Serialize};

/// Incremental delta update for L2 feed
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct L2Delta {
    pub symbol_id: SymbolId,
    pub side: Side,
    pub price: Price,
    pub qty: Qty,
    pub order_count: u32,
    pub is_delete: bool,
    pub sequence: u64,
    pub timestamp_ns: u64,
}

/// Helper calculations for L2 market statistics
pub struct L2Metrics;

impl L2Metrics {
    /// Calculate bid-ask spread in price units
    #[inline]
    pub fn spread(best_bid: Option<Price>, best_ask: Option<Price>) -> Option<u64> {
        match (best_bid, best_ask) {
            (Some(bid), Some(ask)) if ask >= bid => Some(ask - bid),
            _ => None,
        }
    }

    /// Calculate arithmetic mid price
    #[inline]
    pub fn mid_price(best_bid: Option<Price>, best_ask: Option<Price>) -> Option<f64> {
        match (best_bid, best_ask) {
            (Some(bid), Some(ask)) => Some((bid as f64 + ask as f64) / 2.0),
            _ => None,
        }
    }

    /// Calculate volume-weighted micro-price (order flow indicator)
    #[inline]
    pub fn micro_price(
        best_bid: Option<(Price, Qty)>,
        best_ask: Option<(Price, Qty)>,
    ) -> Option<f64> {
        match (best_bid, best_ask) {
            (Some((bid_px, bid_qty)), Some((ask_px, ask_qty))) => {
                let total_qty = (bid_qty + ask_qty) as f64;
                if total_qty > 0.0 {
                    Some(
                        (bid_px as f64 * ask_qty as f64 + ask_px as f64 * bid_qty as f64)
                            / total_qty,
                    )
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Calculate top of book order imbalance (-1.0 to +1.0)
    #[inline]
    pub fn order_book_imbalance(bid_qty: Qty, ask_qty: Qty) -> f64 {
        let total = (bid_qty + ask_qty) as f64;
        if total == 0.0 {
            0.0
        } else {
            (bid_qty as f64 - ask_qty as f64) / total
        }
    }
}
