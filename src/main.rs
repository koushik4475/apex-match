//! Binary entrypoint for apex-match: Runs a real-time 100,000 order benchmark and displays engine metrics.

use apex_match::engine::MatchingEngine;
use apex_match::types::{Order, OrderType, STPMode, Side, TimeInForce};
use hdrhistogram::Histogram;
use std::time::Instant;

fn main() {
    println!(
        r#"
  █████╗ ██████╗ ███████╗██╗  ██╗       ███╗   ███╗ █████╗ ████████╗ ██████╗██╗  ██╗
 ██╔══██╗██╔══██╗██╔════╝╚██╗██╔╝       ████╗ ████║██╔══██╗╚══██╔══╝██╔════╝██║  ██║
 ███████║██████╔╝█████╗   ╚███╔╝  █████╗██╔████╔██║███████║   ██║   ██║     ███████║
 ██╔══██║██╔═══╝ ██╔══╝   ██╔██╗  ╚════╝██║╚██╔╝██║██╔══██║   ██║   ██║     ██╔══██║
 ██║  ██║██║     ███████╗██╔╝ ██╗       ██║ ╚═╝ ██║██║  ██║   ██║   ╚██████╗██║  ██║
 ╚═╝  ╚═╝╚═╝     ╚══════╝╚═╝  ╚═╝       ╚═╝     ╚═╝╚═╝  ╚═╝   ╚═╝    ╚═════╝╚═╝  ╚═╝
    "#
    );
    println!("=== Apex-Match Ultra-Low-Latency HFT Matching Engine Benchmark ===");
    println!("Target: Sub-300ns Execution Latency | Price-Time Priority FIFO");
    println!("Lock-Free Ring Buffer | L2/L3 Orderbook | Real-Time Risk Checks\n");

    let mut engine = MatchingEngine::new(262_144);
    let symbol_id = 1; // E.g., BTC/USD or AAPL
    engine.register_symbol(symbol_id);
    engine.risk_validator.set_reference_price(symbol_id, 10_000); // Reference price $100.00

    let num_orders = 100_000usize;
    println!(
        "Warming up caches & running benchmark on {} orders...",
        num_orders
    );

    let mut hist =
        Histogram::<u64>::new_with_bounds(1, 100_000_000, 3).expect("Histogram creation");
    let start_all = Instant::now();

    // 1. Submit 50,000 resting maker orders (Bids from 95.00 to 99.99, Asks from 100.01 to 105.00)
    for i in 1..=50_000 {
        let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
        let price = if side == Side::Buy {
            9500 + ((i as u64) % 500) // 95.00 to 99.99
        } else {
            10_001 + ((i as u64) % 500) // 100.01 to 105.00
        };

        let order = Order::new(
            i as u64,
            (i % 10 + 1) as u32,
            symbol_id,
            side,
            OrderType::Limit,
            TimeInForce::GTC,
            STPMode::CancelOldest,
            price,
            10,
            i as u64 * 100,
        );

        let t0 = Instant::now();
        let _ = engine.submit_order_direct(order);
        let elapsed = t0.elapsed().as_nanos() as u64;
        let _ = hist.record(elapsed.max(1));
    }

    // 2. Submit 50,000 aggressive taker orders that cross the book and execute trades
    for i in 50_001..=100_000 {
        let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
        // Aggressive crossing prices: Buy @ 103.00, Sell @ 97.00
        let price = if side == Side::Buy { 10_300 } else { 9700 };

        let order = Order::new(
            i as u64,
            (i % 10 + 100) as u32,
            symbol_id,
            side,
            OrderType::Limit,
            TimeInForce::GTC,
            STPMode::CancelOldest,
            price,
            10,
            i as u64 * 100,
        );

        let t0 = Instant::now();
        let _ = engine.submit_order_direct(order);
        let elapsed = t0.elapsed().as_nanos() as u64;
        let _ = hist.record(elapsed.max(1));
    }

    let total_elapsed = start_all.elapsed();
    let throughput = (num_orders as f64) / total_elapsed.as_secs_f64();

    println!("\n====================== RESULTS ======================");
    println!("Total Orders Processed: {}", num_orders);
    println!("Elapsed Time:           {:?}", total_elapsed);
    println!("Throughput:             {:.2} orders/sec", throughput);
    println!("Trades Executed:        {}", engine.metrics.trades_executed);
    println!(
        "Volume Traded:          {}",
        engine.metrics.total_volume_traded
    );
    println!("-----------------------------------------------------");
    println!("LATENCY DISTRIBUTION (Pre-Trade Risk + Order Match):");
    println!("  Min Latency:   {:>6} ns", hist.min());
    println!("  p50 (Median):  {:>6} ns", hist.value_at_quantile(0.50));
    println!("  p90:           {:>6} ns", hist.value_at_quantile(0.90));
    println!("  p99:           {:>6} ns", hist.value_at_quantile(0.99));
    println!("  p99.9:         {:>6} ns", hist.value_at_quantile(0.999));
    println!("  Max Latency:   {:>6} ns", hist.max());
    println!("=====================================================");

    // Print L2 Market Depth
    if let Some(l2) = engine.get_l2_snapshot(symbol_id, 5, 0) {
        println!("\nL2 TOP 5 MARKET DEPTH (Symbol ID: {}):", symbol_id);
        println!("+-------------------------+-------------------------+");
        println!("|           BIDS          |           ASKS          |");
        println!("| Price      Qty    Orders| Price      Qty    Orders|");
        println!("+-------------------------+-------------------------+");
        let depth = l2.bids.len().max(l2.asks.len()).min(5);
        for i in 0..depth {
            let bid_str = if let Some(b) = l2.bids.get(i) {
                format!(
                    "{:>7.2} {:>8} {:>6}",
                    (b.price as f64) / 100.0,
                    b.qty,
                    b.order_count
                )
            } else {
                "                       ".to_string()
            };
            let ask_str = if let Some(a) = l2.asks.get(i) {
                format!(
                    "{:>7.2} {:>8} {:>6}",
                    (a.price as f64) / 100.0,
                    a.qty,
                    a.order_count
                )
            } else {
                "                       ".to_string()
            };
            println!("| {} | {} |", bid_str, ask_str);
        }
        println!("+-------------------------+-------------------------+\n");
    }
}
