//! Get market ticker. Contains information about recent trade prices, liquidity at
//! order book tip, and market volume 

mod request;
mod response;
mod types;

pub use types::{TickerRequest, TickerResponse};