use apex_match::engine::MatchingEngine;
use apex_match::orderbook::OrderBook;
use apex_match::types::{
    ExecutionStatus, Order, OrderType, RejectOrCancelReason, STPMode, Side, TimeInForce,
};

#[test]
fn test_orderbook_price_time_fifo_priority() {
    let mut book = OrderBook::new(1);

    // Place 3 Buy orders at price 100: Order 1 (qty 10), Order 2 (qty 20), Order 3 (qty 30)
    let o1 = Order::new(
        1,
        101,
        1,
        Side::Buy,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        100,
        10,
        1000,
    );
    let o2 = Order::new(
        2,
        102,
        1,
        Side::Buy,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        100,
        20,
        2000,
    );
    let o3 = Order::new(
        3,
        103,
        1,
        Side::Buy,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        100,
        30,
        3000,
    );

    assert!(book.match_order(o1).unwrap().order_placed);
    assert!(book.match_order(o2).unwrap().order_placed);
    assert!(book.match_order(o3).unwrap().order_placed);

    assert_eq!(book.len(), 3);
    let bbo = book.bbo();
    assert_eq!(bbo.0, Some((100, 60))); // Total 60 qty at 100

    // Incoming Sell order for qty 25 at price 100
    // Should fully fill Order 1 (10 qty) and partially fill Order 2 (15 qty leaves 5)
    let taker = Order::new(
        4,
        201,
        1,
        Side::Sell,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        100,
        25,
        4000,
    );
    let res = book.match_order(taker).unwrap();

    assert_eq!(res.fills.len(), 2);
    assert_eq!(res.fills[0].maker_order_id, 1);
    assert_eq!(res.fills[0].qty, 10);
    assert_eq!(res.fills[1].maker_order_id, 2);
    assert_eq!(res.fills[1].qty, 15);

    // Remaining book should have Order 2 (5 qty) and Order 3 (30 qty) => 35 qty total
    let bbo_after = book.bbo();
    assert_eq!(bbo_after.0, Some((100, 35)));
    assert_eq!(book.len(), 2);
}

#[test]
fn test_order_cancellation_and_partial_fill() {
    let mut book = OrderBook::new(1);

    // Place an Ask at 200, qty 50
    let maker = Order::new(
        10,
        101,
        1,
        Side::Sell,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        200,
        50,
        1000,
    );
    book.match_order(maker).unwrap();

    // Partial fill with Buy at 200, qty 20
    let taker = Order::new(
        11,
        201,
        1,
        Side::Buy,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        200,
        20,
        2000,
    );
    let res = book.match_order(taker).unwrap();
    assert_eq!(res.fills.len(), 1);
    assert_eq!(res.fills[0].qty, 20);

    // Resting order has 30 remaining. Now cancel it
    let cancel_rep = book.cancel_order(10, 3000).unwrap();
    assert_eq!(cancel_rep.status, ExecutionStatus::Cancelled);
    assert_eq!(cancel_rep.leaves_qty, 30);
    assert_eq!(cancel_rep.filled_qty, 20);

    // Book must now be completely empty
    assert!(book.is_empty());
    assert_eq!(book.bbo().1, None);
}

#[test]
fn test_self_trade_prevention_modes() {
    // Mode 1: CancelOldest
    {
        let mut book = OrderBook::new(1);
        let maker = Order::new(
            1,
            42,
            1,
            Side::Buy,
            OrderType::Limit,
            TimeInForce::GTC,
            STPMode::CancelOldest,
            100,
            10,
            100,
        );
        book.match_order(maker).unwrap();

        let taker = Order::new(
            2,
            42,
            1,
            Side::Sell,
            OrderType::Limit,
            TimeInForce::GTC,
            STPMode::CancelOldest,
            100,
            10,
            200,
        );
        let res = book.match_order(taker).unwrap();

        assert_eq!(res.fills.len(), 0); // No wash trade allowed
        assert_eq!(res.cancelled_maker_ids, vec![1]); // Maker cancelled
        assert!(res.order_placed); // Taker rests
        assert_eq!(book.bbo().1, Some((100, 10)));
    }

    // Mode 2: CancelNewest
    {
        let mut book = OrderBook::new(1);
        let maker = Order::new(
            3,
            42,
            1,
            Side::Buy,
            OrderType::Limit,
            TimeInForce::GTC,
            STPMode::CancelNewest,
            100,
            10,
            100,
        );
        book.match_order(maker).unwrap();

        let taker = Order::new(
            4,
            42,
            1,
            Side::Sell,
            OrderType::Limit,
            TimeInForce::GTC,
            STPMode::CancelNewest,
            100,
            10,
            200,
        );
        let res = book.match_order(taker).unwrap();

        assert_eq!(res.fills.len(), 0);
        assert!(!res.order_placed); // Taker cancelled
        assert_eq!(book.bbo().0, Some((100, 10))); // Maker remains
    }
}

#[test]
fn test_ioc_and_fok_semantics() {
    let mut book = OrderBook::new(1);

    // Resting Ask: 100 qty @ 50
    let maker = Order::new(
        1,
        10,
        1,
        Side::Sell,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        50,
        100,
        1000,
    );
    book.match_order(maker).unwrap();

    // 1. IOC for 150 qty @ 50 -> Should fill 100, and cancel remaining 50 without resting
    let ioc = Order::new(
        2,
        20,
        1,
        Side::Buy,
        OrderType::ImmediateOrCancel,
        TimeInForce::IOC,
        STPMode::None,
        50,
        150,
        2000,
    );
    let res = book.match_order(ioc).unwrap();
    assert_eq!(res.fills.len(), 1);
    assert_eq!(res.fills[0].qty, 100);
    assert!(!res.order_placed);
    assert!(book.is_empty());

    // 2. FOK for 50 qty when book only has 30 qty -> Should reject completely with FOKCannotFill
    let maker2 = Order::new(
        3,
        10,
        1,
        Side::Sell,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        60,
        30,
        3000,
    );
    book.match_order(maker2).unwrap();

    let fok = Order::new(
        4,
        20,
        1,
        Side::Buy,
        OrderType::FillOrKill,
        TimeInForce::FOK,
        STPMode::None,
        60,
        50,
        4000,
    );
    let res2 = book.match_order(fok).unwrap();
    assert_eq!(res2.fills.len(), 0);
    assert_eq!(
        res2.reports[0].reason,
        Some(RejectOrCancelReason::FOKCannotFill)
    );
    assert_eq!(book.len(), 1); // Resting maker still exists
}

#[test]
fn test_l2_and_l3_consistency() {
    let mut book = OrderBook::new(1);

    for i in 1..=10 {
        let buy = Order::new(
            i,
            100,
            1,
            Side::Buy,
            OrderType::Limit,
            TimeInForce::GTC,
            STPMode::None,
            100 + i,
            10,
            i * 100,
        );
        let sell = Order::new(
            i + 100,
            101,
            1,
            Side::Sell,
            OrderType::Limit,
            TimeInForce::GTC,
            STPMode::None,
            120 + i,
            10,
            i * 100,
        );
        book.match_order(buy).unwrap();
        book.match_order(sell).unwrap();
    }

    let l2 = book.get_l2_snapshot(10, 5000);
    let l3 = book.get_l3_snapshot(5000);

    assert_eq!(l2.bids.len(), 10);
    assert_eq!(l2.asks.len(), 10);
    assert_eq!(l3.bids.len(), 10);
    assert_eq!(l3.asks.len(), 10);

    // Best bid must be highest price (110)
    assert_eq!(l2.bids[0].price, 110);
    // Best ask must be lowest price (121)
    assert_eq!(l2.asks[0].price, 121);
}

#[test]
fn test_100k_orders_matching_benchmark() {
    let mut engine = MatchingEngine::new(131_072);
    let symbol_id = 1;
    engine.register_symbol(symbol_id);

    let start = std::time::Instant::now();
    let num_orders = 100_000u64;

    for i in 1..=num_orders {
        let is_buy = i % 2 == 0;
        let side = if is_buy { Side::Buy } else { Side::Sell };
        let price = if is_buy {
            1000 + (i % 20)
        } else {
            1010 - (i % 20)
        };

        let order = Order::new(
            i,
            (i % 50 + 1) as u32,
            symbol_id,
            side,
            OrderType::Limit,
            TimeInForce::GTC,
            STPMode::CancelOldest,
            price,
            5,
            i * 50,
        );

        let _ = engine.submit_order_direct(order);
    }

    let duration = start.elapsed();
    assert_eq!(engine.metrics.orders_processed, num_orders);
    assert!(engine.metrics.trades_executed > 10_000);

    println!(
        "Processed {} orders in {:?} ({:.2} orders/sec)",
        num_orders,
        duration,
        (num_orders as f64) / duration.as_secs_f64()
    );
}
