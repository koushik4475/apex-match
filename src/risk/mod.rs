//! Risk management module: Pre-trade margin checks, credit limits, and fat-finger validation.

pub mod credit;
pub mod margin;
pub mod validator;

pub use credit::{CreditManager, TraderRiskLimits};
pub use margin::TraderAccount;
pub use validator::RiskValidator;
