//! Types shared across the client and protocol requests

pub mod blockchain;
pub mod exchange;
pub mod keys;

pub use blockchain::{eth, neo, AssetOrCrosschain, Prefix, PublicKey};
pub use exchange::{
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
    OrderbookOrder,
    OrderCancellationPolicy,
    OrderCancellationReason,
    OrderRate,
    OrderStatus,
    OrderType,
    Rate,
    SubscriptionTrade, //TODO: remove subscription trade
    Trade,
};
pub use keys::ApiKeys;
