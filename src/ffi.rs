//! Foreign Function Interface (FFI) exposing C ABI bindings for apex-match.

use crate::engine::matching_engine::MatchingEngine;
use crate::types::{Order, OrderType, STPMode, Side, TimeInForce};
use std::ffi::c_void;

#[repr(C)]
pub struct ApexOrderC {
    pub order_id: u64,
    pub trader_id: u32,
    pub symbol_id: u32,
    pub side: u8,
    pub order_type: u8,
    pub tif: u8,
    pub stp_mode: u8,
    pub price: u64,
    pub qty: u64,
    pub timestamp_ns: u64,
}

#[repr(C)]
pub struct ApexL2LevelC {
    pub price: u64,
    pub qty: u64,
    pub order_count: u32,
}

#[repr(C)]
pub struct ApexL2SnapshotC {
    pub symbol_id: u32,
    pub sequence: u64,
    pub timestamp_ns: u64,
    pub bid_count: u32,
    pub ask_count: u32,
    pub bids: [ApexL2LevelC; 50],
    pub asks: [ApexL2LevelC; 50],
}

#[no_mangle]
pub extern "C" fn apex_engine_create(ring_buffer_capacity: usize) -> *mut c_void {
    let cap = if ring_buffer_capacity == 0 {
        65536
    } else {
        ring_buffer_capacity
    };
    let engine = Box::new(MatchingEngine::new(cap));
    Box::into_raw(engine) as *mut c_void
}

/// # Safety
/// Caller must ensure `engine_ptr` is either null or a valid pointer returned by `apex_engine_create`.
#[no_mangle]
pub unsafe extern "C" fn apex_engine_destroy(engine_ptr: *mut c_void) {
    if !engine_ptr.is_null() {
        let _ = Box::from_raw(engine_ptr as *mut MatchingEngine);
    }
}

/// # Safety
/// Caller must ensure `engine_ptr` points to a valid `MatchingEngine` instance and `order` is a valid pointer.
#[no_mangle]
pub unsafe extern "C" fn apex_engine_submit_order(
    engine_ptr: *mut c_void,
    order: *const ApexOrderC,
) -> i32 {
    if engine_ptr.is_null() || order.is_null() {
        return -1;
    }
    let engine = &mut *(engine_ptr as *mut MatchingEngine);
    let o = &*order;

    // C callers can pass arbitrary integer discriminants. Reject unknown values
    // instead of silently translating malformed input into a valid order.
    if o.side > 1 || o.order_type > 4 || o.tif > 3 || o.stp_mode > 3 {
        return -1;
    }

    let side = match o.side {
        0 => Side::Buy,
        _ => Side::Sell,
    };
    let order_type = match o.order_type {
        0 => OrderType::Limit,
        1 => OrderType::Market,
        2 => OrderType::PostOnly,
        3 => OrderType::ImmediateOrCancel,
        4 => OrderType::FillOrKill,
        _ => OrderType::Limit,
    };
    let tif = match o.tif {
        0 => TimeInForce::GTC,
        1 => TimeInForce::IOC,
        2 => TimeInForce::FOK,
        _ => TimeInForce::Day,
    };
    let stp = match o.stp_mode {
        1 => STPMode::CancelOldest,
        2 => STPMode::CancelNewest,
        3 => STPMode::DecrementAndCancel,
        _ => STPMode::None,
    };

    let rust_order = Order::new(
        o.order_id,
        o.trader_id,
        o.symbol_id,
        side,
        order_type,
        tif,
        stp,
        o.price,
        o.qty,
        o.timestamp_ns,
    );

    match engine.submit_order_direct(rust_order) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

/// # Safety
/// Caller must ensure `engine_ptr` is a valid pointer to a `MatchingEngine` instance.
#[no_mangle]
pub unsafe extern "C" fn apex_engine_cancel_order(
    engine_ptr: *mut c_void,
    symbol_id: u32,
    order_id: u64,
) -> i32 {
    if engine_ptr.is_null() {
        return -1;
    }
    let engine = &mut *(engine_ptr as *mut MatchingEngine);
    match engine.cancel_order_direct(symbol_id, order_id, 0) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

/// # Safety
/// Caller must ensure `engine_ptr` is a valid pointer to a `MatchingEngine` instance.
#[no_mangle]
pub unsafe extern "C" fn apex_engine_poll(engine_ptr: *mut c_void, max_events: usize) -> i32 {
    if engine_ptr.is_null() {
        return -1;
    }
    let engine = &mut *(engine_ptr as *mut MatchingEngine);
    engine.poll_ring_buffer(max_events) as i32
}

/// # Safety
/// Caller must ensure `engine_ptr` is a valid pointer and `out_snapshot` points to writable allocated memory.
#[no_mangle]
pub unsafe extern "C" fn apex_engine_get_l2_snapshot(
    engine_ptr: *mut c_void,
    symbol_id: u32,
    out_snapshot: *mut ApexL2SnapshotC,
) -> i32 {
    if engine_ptr.is_null() || out_snapshot.is_null() {
        return -1;
    }
    let engine = &*(engine_ptr as *mut MatchingEngine);
    if let Some(snap) = engine.get_l2_snapshot(symbol_id, 50, 0) {
        let out = &mut *out_snapshot;
        out.symbol_id = snap.symbol_id;
        out.sequence = snap.sequence;
        out.timestamp_ns = snap.timestamp_ns;
        out.bid_count = snap.bids.len().min(50) as u32;
        out.ask_count = snap.asks.len().min(50) as u32;

        for (i, b) in snap.bids.iter().take(50).enumerate() {
            out.bids[i] = ApexL2LevelC {
                price: b.price,
                qty: b.qty,
                order_count: b.order_count,
            };
        }
        for (i, a) in snap.asks.iter().take(50).enumerate() {
            out.asks[i] = ApexL2LevelC {
                price: a.price,
                qty: a.qty,
                order_count: a.order_count,
            };
        }
        0
    } else {
        -2
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn valid_order() -> ApexOrderC {
        ApexOrderC {
            order_id: 1,
            trader_id: 7,
            symbol_id: 1,
            side: 0,
            order_type: 0,
            tif: 0,
            stp_mode: 0,
            price: 10_000,
            qty: 1,
            timestamp_ns: 1,
        }
    }

    #[test]
    fn rejects_unknown_c_enum_values_without_processing_order() {
        let engine_ptr = apex_engine_create(16);
        let mut order = valid_order();

        for (field, invalid_value) in [
            (0, 2), // side
            (1, 5), // order_type
            (2, 4), // time in force
            (3, 4), // self-trade prevention
        ] {
            match field {
                0 => order.side = invalid_value,
                1 => order.order_type = invalid_value,
                2 => order.tif = invalid_value,
                3 => order.stp_mode = invalid_value,
                _ => unreachable!(),
            }

            let result = unsafe { apex_engine_submit_order(engine_ptr, &order) };
            assert_eq!(result, -1, "field {field} should be rejected");

            match field {
                0 => order.side = 0,
                1 => order.order_type = 0,
                2 => order.tif = 0,
                3 => order.stp_mode = 0,
                _ => unreachable!(),
            }
        }

        unsafe {
            let engine = &*(engine_ptr as *mut MatchingEngine);
            assert_eq!(engine.metrics.orders_processed, 0);
            apex_engine_destroy(engine_ptr);
        }
    }
}
