use apex_match::orderbook::OrderBook;
use apex_match::types::{Order, OrderType, STPMode, Side, TimeInForce};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_order_insertion(c: &mut Criterion) {
    c.bench_function("orderbook_limit_insert", |b| {
        let mut book = OrderBook::new(1);
        let mut id = 1u64;
        b.iter(|| {
            let order = Order::new(
                black_box(id),
                100,
                1,
                Side::Buy,
                OrderType::Limit,
                TimeInForce::GTC,
                STPMode::None,
                100_00,
                10,
                1000,
            );
            id += 1;
            let _ = book.match_order(order);
        });
    });
}

fn bench_order_crossing_match(c: &mut Criterion) {
    c.bench_function("orderbook_match_fill", |b| {
        let mut book = OrderBook::new(1);
        // Pre-fill asks
        for i in 1..=10_000 {
            let ask = Order::new(
                i,
                100,
                1,
                Side::Sell,
                OrderType::Limit,
                TimeInForce::GTC,
                STPMode::None,
                100_00,
                10,
                1000,
            );
            let _ = book.match_order(ask);
        }
        let mut id = 20_000u64;
        b.iter(|| {
            let buy = Order::new(
                black_box(id),
                200,
                1,
                Side::Buy,
                OrderType::Limit,
                TimeInForce::IOC,
                STPMode::None,
                100_00,
                10,
                1000,
            );
            id += 1;
            let _ = book.match_order(buy);
        });
    });
}

fn bench_order_cancellation(c: &mut Criterion) {
    c.bench_function("orderbook_cancel_order", |b| {
        let mut book = OrderBook::new(1);
        let mut id = 1u64;
        b.iter(|| {
            let order = Order::new(
                id,
                100,
                1,
                Side::Buy,
                OrderType::Limit,
                TimeInForce::GTC,
                STPMode::None,
                100_00,
                10,
                1000,
            );
            let _ = book.match_order(order);
            let _ = book.cancel_order(black_box(id), 2000);
            id += 1;
        });
    });
}

criterion_group!(
    benches,
    bench_order_insertion,
    bench_order_crossing_match,
    bench_order_cancellation
);
criterion_main!(benches);
