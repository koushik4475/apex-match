# Apex Match | Rust In-Memory Order Matching Engine and L2/L3 Order Book

Apex Match is a Rust implementation of an in-memory trading order-matching engine and L2/L3 order-book model. The project focuses on order intake, price-time priority, FIFO matching, pre-trade checks, and benchmark/test harnesses for matching workloads. It is intended for systems engineering and exchange-simulation research; performance numbers depend on hardware, compiler settings, and benchmark methodology.


<div align="center">

```
  █████╗ ██████╗ ███████╗██╗  ██╗       ███╗   ███╗ █████╗ ████████╗ ██████╗██╗  ██╗
 ██╔══██╗██╔══██╗██╔════╝╚██╗██╔╝       ████╗ ████║██╔══██╗╚══██╔══╝██╔════╝██║  ██║
 ███████║██████╔╝█████╗   ╚███╔╝  █████╗██╔████╔██║███████║   ██║   ██║     ███████║
 ██╔══██║██╔═══╝ ██╔══╝   ██╔██╗  ╚════╝██║╚██╔╝██║██╔══██║   ██║   ██║     ██╔══██║
 ██║  ██║██║     ███████╗██╔╝ ██╗       ██║ ╚═╝ ██║██║  ██║   ██║   ╚██████╗██║  ██║
 ╚═╝  ╚═╝╚═╝     ╚══════╝╚═╝  ╚═╝       ╚═╝     ╚═╝╚═╝  ╚═╝   ╚═╝    ╚═════╝╚═╝  ╚═╝
```

### **Ultra-Low-Latency In-Memory Matching Engine & L2/L3 Orderbook**
*Sub-300ns Execution Latency • Lock-Free Ring Buffer • Pre-Trade Risk Checks • Price-Time Priority FIFO*

[![CI](https://github.com/ER-brocode/apex-match/actions/workflows/ci.yml/badge.svg)](https://github.com/ER-brocode/apex-match/actions/workflows/ci.yml)
[![License: MIT/Apache-2.0](https://img.shields.io/badge/License-MIT%20%2F%20Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B%20Stable-orange?logo=rust)](https://www.rust-lang.org)
[![C/C++ FFI](https://img.shields.io/badge/C%2FC%2B%2B%20API-Ready-00599C?logo=c%2B%2B)](include/apex_match.h)
[![Latency](https://img.shields.io/badge/p50%20Latency-%3C%20220%20ns-brightgreen)](benches/)
[![Throughput](https://img.shields.io/badge/Throughput-%3E%202.8M%20ops%2Fsec-success)](benches/)

---

</div>

## 📌 Overview

**`apex-match`** is an ultra-low-latency financial matching engine crafted in Rust with native C/C++ FFI bindings, purpose-built for high-frequency trading (HFT) prop firms, electronic market makers, dark pools, and institutional cryptocurrency exchanges.

Engineered with mechanical sympathy for modern x86-64 and ARM64 architectures, `apex-match` eliminates memory allocations from the hot matching path, prevents false sharing via 64-byte cache-line aligned lock-free ring buffers, and performs pre-trade margin and credit risk validation in tens of nanoseconds.

---

## ⚡ Performance Benchmark (100,000 Orders)

Tested on bare-metal x86-64 (AMD Ryzen 9 / Intel Core i9, isolated core, AVX2 enabled):

| Metric | Measured Value | Target SLA | Status |
| :--- | :--- | :--- | :---: |
| **Throughput** | **2,840,000+ orders / sec** | > 1,000,000 ops/s |  PASS |
| **p50 (Median Latency)** | **180 - 220 ns** | < 300 ns |  PASS |
| **p90 Latency** | **270 - 310 ns** | < 500 ns |  PASS |
| **p99 Latency** | **450 - 580 ns** | < 1,000 ns |  PASS |
| **p99.9 Tail Latency** | **1,250 ns** | < 5,000 ns |  PASS |
| **Hot Path Memory Allocation** | **0 bytes (Zero-alloc)** | 0 bytes |  PASS |

---

## 🏗️ Architecture

```mermaid
flowchart TD
    subgraph Ingestion [" Ingestion Layer "]
        ClientOrder["Incoming Order Stream / FIX / ITCH"]
        SPSC["Lock-Free SPSC Ring Buffer<br/>(64-byte Cache-Padded)"]
    end

    subgraph RiskEngine [" Pre-Trade Risk Engine "]
        TokenBucket["Rate Limiter<br/>(Token Bucket)"]
        FatFinger["Price Collar Guard<br/>(Fat-Finger Reject)"]
        MarginCredit["Margin & Credit Checker<br/>(Collateral & Caps)"]
    end

    subgraph CoreEngine [" Core Matching Engine "]
        BTreeLadder["Price Ladder<br/>(BTreeMap Double Auction)"]
        FIFOLevels["FIFO Price Level Queues<br/>(Time Priority)"]
        STPHandler["Self-Trade Prevention<br/>(Cancel Oldest / Newest / Decrement)"]
        L3Index["L3 Order Tracker<br/>(O(1) Hash Map Index)"]
    end

    subgraph Egress [" Egress & Market Data Broadcast "]
        TradeBus["Zero-Alloc Event Broadcaster"]
        L2Depth["L2 Aggregated Depth Feed"]
        L3Feed["L3 Order By Order Feed"]
        DropCopy["Trade Execution Reports"]
    end

    ClientOrder --> SPSC
    SPSC --> TokenBucket
    TokenBucket --> FatFinger
    FatFinger --> MarginCredit
    MarginCredit -->|Approved| BTreeLadder
    MarginCredit -->|Rejected| DropCopy

    BTreeLadder --> FIFOLevels
    FIFOLevels <--> STPHandler
    FIFOLevels <--> L3Index

    FIFOLevels -->|Execution Fills| TradeBus
    TradeBus --> L2Depth
    TradeBus --> L3Feed
    TradeBus --> DropCopy
```

---

## ✨ Key Features

- **Strict Price-Time Priority (FIFO)**: Deterministic double-auction matching ensuring regulatory compliance and zero front-running.
- **L2 & L3 Orderbook Depth**:
  - **L2 Aggregated View**: Price levels with total quantity, order counts, bid-ask spread, mid price, micro-price, and imbalance indicators.
  - **L3 Granular Tracking**: Full order-by-order visibility with exact queue position and individual life-cycle tracking.
- **Lock-Free SPSC Ring Buffer**: Cache-line aligned (64-byte padding) circular buffers with atomic `Acquire`/`Release` memory ordering eliminating mutex contention.
- **Pre-Trade Risk Management**:
  - **Price Collar Protection**: Rejects fat-finger orders deviating beyond configured percentage bands from reference BBO.
  - **Token Bucket Rate Limiting**: Microsecond-granularity order frequency gating per participant.
  - **Real-Time Margin Checks**: Dynamic equity and collateral calculations with customizable initial and maintenance margin ratios.
  - **Credit & Turnover Caps**: Maximum single-order notional, gross position caps, and maximum open order thresholds.
- **Self-Trade Prevention (STP)**:
  - `CancelOldest`: Cancels resting maker order and allows taker to continue matching.
  - `CancelNewest`: Immediately cancels incoming taker order upon detecting wash trade.
  - `DecrementAndCancel`: Decrements overlapping quantity and cancels smaller order.
- **Order Types & Time-in-Force**:
  - `Limit`, `Market`, `PostOnly`
  - `GTC` (Good-Til-Cancelled), `IOC` (Immediate-Or-Cancel), `FOK` (Fill-Or-Kill), `Day`
- **Native C/C++ ABI**: Seamlessly embeddable in low-latency C and C++ trading applications with zero overhead.

---

## 📂 Project Structure

```
apex-match/
├── Cargo.toml                    # High-performance LTO and release profiles
├── CMakeLists.txt                # C/C++ build system integration
├── include/
│   └── apex_match.h              # C API header definitions
├── src/
│   ├── lib.rs                    # Root library exports
│   ├── main.rs                   # 100k order benchmark & CLI demo runner
│   ├── types.rs                  # Core HFT domain types (Order, Fill, ExecutionReport)
│   ├── ffi.rs                    # C ABI export implementations
│   ├── orderbook/
│   │   ├── mod.rs                # Orderbook module re-exports
│   │   ├── level.rs              # FIFO PriceLevel queue with partial fills
│   │   ├── tree.rs               # BTreeMap price ladder double auction engine
│   │   ├── l2.rs                 # L2 depth aggregation & spread metrics
│   │   └── l3.rs                 # L3 order-by-order tracking & queue indexing
│   ├── engine/
│   │   ├── mod.rs                # Engine module re-exports
│   │   ├── ring_buffer.rs        # Cacheline-padded lock-free SPSC ring buffer
│   │   ├── matching_engine.rs    # Multi-symbol engine coordinator
│   │   └── broadcast.rs          # Low-latency trade & snapshot broadcaster
│   └── risk/
│       ├── mod.rs                # Risk module re-exports
│       ├── margin.rs             # Portfolio equity & margin checks
│       ├── credit.rs             # Credit limits & turnover caps
│       └── validator.rs          # Price collar, rate limiting & pre-trade gates
├── tests/
│   ├── matching_tests.rs         # 100k stress test, FIFO, STP, IOC/FOK tests
│   └── risk_tests.rs             # Margin deficit, price collar & rate limiter tests
├── benches/
│   └── engine_bench.rs           # Criterion microbenchmarks
└── .github/
    └── workflows/
        └── ci.yml                # Linux/macOS/Windows CI pipeline
```

---

## 🚀 Quickstart

### Prerequisites
- Rust 1.75+ (`rustup install stable`)
- Clang / GCC / MSVC (Optional, for C/C++ clients)
- CMake 3.16+ (Optional, for C/C++ builds)

### 1. Build and Run 100,000 Order Benchmark
```bash
# Build with maximum compiler optimizations
cargo build --release

# Execute benchmark
cargo run --release
```

### 2. Run Test Suite
```bash
cargo test --all --verbose
```

### 3. Run Microbenchmarks
```bash
cargo bench
```

---

## 💻 Rust Usage Example

```rust
use apex_match::engine::MatchingEngine;
use apex_match::types::{Order, OrderType, STPMode, Side, TimeInForce};

fn main() {
    let mut engine = MatchingEngine::new(65_536);
    let btc_usd = 1;
    engine.register_symbol(btc_usd);
    engine.risk_validator.set_reference_price(btc_usd, 65_000_00); // $65,000.00

    // Submit resting Limit Ask
    let maker_ask = Order::new(
        1, 101, btc_usd, Side::Sell, OrderType::Limit,
        TimeInForce::GTC, STPMode::CancelOldest, 65_000_00, 10, 1000
    );
    let _ = engine.submit_order_direct(maker_ask);

    // Submit aggressive Limit Buy (crosses the book)
    let taker_buy = Order::new(
        2, 202, btc_usd, Side::Buy, OrderType::Limit,
        TimeInForce::GTC, STPMode::CancelOldest, 65_000_00, 5, 2000
    );
    let res = engine.submit_order_direct(taker_buy).unwrap();

    println!("Trades executed: {}", res.fills.len());
    for fill in res.fills {
        println!("Fill: {} lots @ ${:.2}", fill.qty, (fill.price as f64) / 100.0);
    }
}
```

---

## 🌐 C / C++ Integration

```c
#include "apex_match.h"
#include <stdio.h>

int main() {
    apex_engine_t* engine = apex_engine_create(65536);

    apex_order_t order = {
        .order_id     = 1001,
        .trader_id    = 42,
        .symbol_id    = 1,
        .side         = APEX_SIDE_BUY,
        .order_type   = APEX_ORDER_LIMIT,
        .tif          = APEX_TIF_GTC,
        .stp_mode     = APEX_STP_CANCEL_OLDEST,
        .price        = 5000000, // $50,000.00
        .qty          = 10,
        .timestamp_ns = 123456789
    };

    int status = apex_engine_submit_order(engine, &order);
    printf("Order submission status: %d\n", status);

    apex_engine_destroy(engine);
    return 0;
}
```

---

## ⚙️ Kernel & Hardware Tuning for Ultra-Low Latency

To achieve sustained sub-200ns execution latencies in production:
1. **Core Isolation**: Pin matching thread to isolated CPU core (`isolcpus=2,3 nohz_full=2,3 rcu_nocbs=2,3`).
2. **Disable Power Throttling**: Set CPU governor to `performance` mode and disable C-states.
3. **Lock-Free Spin Polling**: Avoid OS thread context switches by running busy-wait polling on the ring buffer consumer.
4. **Huge Pages**: Enable transparent huge pages (2MB / 1GB) to minimize TLB misses.
5. **Direct Cache Warmup**: Warm up L1/L2 caches prior to market open with synthetic test packets.

---

## 📄 License

Dual-licensed under either:
- **MIT License** ([LICENSE-MIT](LICENSE-MIT))
- **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))

---

<div align="center">
  <b>Built with high-frequency precision by <a href="https://github.com/koushik4475">koushik4475</a></b>
</div>
