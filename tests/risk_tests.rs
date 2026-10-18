use apex_match::risk::credit::TraderRiskLimits;
use apex_match::risk::margin::TraderAccount;
use apex_match::risk::validator::RiskValidator;
use apex_match::types::{Order, OrderType, RejectOrCancelReason, STPMode, Side, TimeInForce};

#[test]
fn test_pre_trade_margin_rejection() {
    let mut validator = RiskValidator::default();

    // Account with $1,000 cash balance, 10% initial margin requirement
    let account = TraderAccount::new(101, 100_000, 0.10, 0.05);
    validator.add_account(account);

    // Order requires $50,000 notional * 0.10 = $5,000 margin -> Exceeds $1,000 cash balance!
    let order = Order::new(
        1,
        101,
        1,
        Side::Buy,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        50_000,
        100,
        1000,
    );
    let res = validator.validate_order(&order);

    assert_eq!(res, Err(RejectOrCancelReason::InsufficientMargin));

    // Order requires $5,000 notional * 0.10 = $500 margin -> Fits within $1,000 cash balance!
    let valid_order = Order::new(
        2,
        101,
        1,
        Side::Buy,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        5000,
        100,
        2000,
    );
    assert!(validator.validate_order(&valid_order).is_ok());
}

#[test]
fn test_price_collar_fat_finger_protection() {
    let mut validator = RiskValidator::new(0.05, 100_000.0); // 5% price collar
    let symbol_id = 1;
    validator.set_reference_price(symbol_id, 10_000); // Ref price = $100.00
                                                      // Max allowable Buy = $105.00, Min allowable Sell = $95.00

    // Buy @ $106.00 should be rejected by collar
    let bad_buy = Order::new(
        1,
        1,
        symbol_id,
        Side::Buy,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        10_600,
        10,
        1000,
    );
    assert_eq!(
        validator.validate_order(&bad_buy),
        Err(RejectOrCancelReason::PriceCollarViolation)
    );

    // Sell @ $94.00 should be rejected by collar
    let bad_sell = Order::new(
        2,
        1,
        symbol_id,
        Side::Sell,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        9400,
        10,
        2000,
    );
    assert_eq!(
        validator.validate_order(&bad_sell),
        Err(RejectOrCancelReason::PriceCollarViolation)
    );

    // Valid Buy @ $104.00 within band
    let good_buy = Order::new(
        3,
        1,
        symbol_id,
        Side::Buy,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        10_400,
        10,
        3000,
    );
    assert!(validator.validate_order(&good_buy).is_ok());
}

#[test]
fn test_credit_limits_rejection() {
    let mut validator = RiskValidator::default();

    // Trader with max order qty = 50 lots
    let limits = TraderRiskLimits::new(99, 50, 1_000_000, 10_000_000, 100);
    validator.credit_manager.register(limits);

    // Order with 100 lots exceeds max 50 lots
    let bad_order = Order::new(
        1,
        99,
        1,
        Side::Buy,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        1000,
        100,
        1000,
    );
    assert_eq!(
        validator.validate_order(&bad_order),
        Err(RejectOrCancelReason::RiskLimitExceeded)
    );

    // Order with 20 lots passes
    let good_order = Order::new(
        2,
        99,
        1,
        Side::Buy,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        1000,
        20,
        2000,
    );
    assert!(validator.validate_order(&good_order).is_ok());
}

#[test]
fn test_rate_limiter_throttling() {
    // Capacity 3 tokens, rate 1 token per second
    let mut validator = RiskValidator::new(0.50, 1.0);
    let trader_id = 77;

    // Burst 3 orders in rapid succession at same timestamp
    for i in 1..=3 {
        let order = Order::new(
            i,
            trader_id,
            1,
            Side::Buy,
            OrderType::Limit,
            TimeInForce::GTC,
            STPMode::None,
            100,
            1,
            1_000_000,
        );
        assert!(validator.validate_order(&order).is_ok());
    }

    // 4th order at the same timestamp must be throttled
    let throttled_order = Order::new(
        4,
        trader_id,
        1,
        Side::Buy,
        OrderType::Limit,
        TimeInForce::GTC,
        STPMode::None,
        100,
        1,
        1_000_000,
    );
    assert_eq!(
        validator.validate_order(&throttled_order),
        Err(RejectOrCancelReason::RateLimitThrottled)
    );
}
