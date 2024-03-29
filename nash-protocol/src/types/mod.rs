//! Types shared across the client and protocol requests

pub mod blockchain;
pub mod exchange;
pub mod keys;
pub mod market_pair;

pub use blockchain::{eth, neo, AssetOrCrosschain, Prefix, PublicKey};
pub use self::exchange::{
    AccountTradeSide,
    Amount,
    Asset,
    AssetAmount,
    AssetofPrecision,
    Blockchain,
    BuyOrSell,
    Candle,
    CandleInterval,
    DateTimeRange,
    Market,
    Nonce,
    Order,
    OrderCancellationPolicy,
    OrderCancellationReason,
    OrderRate,
    OrderStatus,
    OrderType,
    OrderbookOrder,
    Rate,
    Trade,
};
pub use keys::ApiKeys;
