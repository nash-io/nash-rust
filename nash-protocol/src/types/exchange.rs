//! Types used for protocol request inputs (arguments) and outputs. Types
//! specific to a single protocol request will live within the respetitive
//! module. For example `protocol::place_order`.

use crate::errors::{ProtocolError, Result};
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use super::blockchain::bigdecimal_to_nash_prec;

/// Representation of blockchains to help navigate encoding issues

#[derive(Clone, Debug, Copy, PartialEq, Hash, Eq)]
pub enum Blockchain {
    NEO,
    Ethereum,
    Bitcoin,
}

impl Blockchain {
    pub fn all() -> Vec<Blockchain> {
        vec![Self::Bitcoin, Self::Ethereum, Self::NEO]
    }
}

/// Assets are the units of value that can be traded in a market
/// We also use this type as a root for encodings for smart contract
/// operations defined in super::sc_payloads
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Asset {
    ETH,
    BAT,
    OMG,
    USDC,
    USDT,
    ZRX,
    LINK,
    QNT,
    RLC,
    ANT,
    BTC,
    NEO,
    GAS,
    TRAC,
    GUNTHY,
    NNN,
}

impl Asset {
    /// Get blockchain associated with a specific asset
    pub fn blockchain(&self) -> Blockchain {
        match self {
            Self::ETH => Blockchain::Ethereum,
            Self::USDC => Blockchain::Ethereum,
            Self::USDT => Blockchain::Ethereum,
            Self::BAT => Blockchain::Ethereum,
            Self::OMG => Blockchain::Ethereum,
            Self::ZRX => Blockchain::Ethereum,
            Self::LINK => Blockchain::Ethereum,
            Self::QNT => Blockchain::Ethereum,
            Self::RLC => Blockchain::Ethereum,
            Self::ANT => Blockchain::Ethereum,
            Self::TRAC => Blockchain::Ethereum,
            Self::GUNTHY => Blockchain::Ethereum,
            Self::BTC => Blockchain::Bitcoin,
            Self::NEO => Blockchain::NEO,
            Self::GAS => Blockchain::NEO,
            Self::NNN => Blockchain::NEO,
        }
    }

    /// Get asset name as an `&str`. This name will be compatible with the
    /// what Nash's GraphQL protocol wants
    // FIXME: This can probably be more cleverly automated
    pub fn name(&self) -> &'static str {
        match self {
            Self::ETH => "eth",
            Self::USDC => "usdc",
            Self::USDT => "usdt",
            Self::BAT => "bat",
            Self::OMG => "omg",
            Self::ZRX => "zrx",
            Self::LINK => "link",
            Self::QNT => "qnt",
            Self::RLC => "rlc",
            Self::ANT => "ant",
            Self::BTC => "btc",
            Self::NEO => "neo",
            Self::GAS => "gas",
            Self::TRAC => "trac",
            Self::GUNTHY => "gunthy",
            Self::NNN => "nnn",
        }
    }

    pub fn from_str(asset_str: &str) -> Result<Self> {
        match asset_str {
            "eth" => Ok(Self::ETH),
            "usdc" => Ok(Self::USDC),
            "usdt" => Ok(Self::USDT),
            "bat" => Ok(Self::BAT),
            "omg" => Ok(Self::OMG),
            "zrx" => Ok(Self::ZRX),
            "link" => Ok(Self::LINK),
            "qnt" => Ok(Self::QNT),
            "rlc" => Ok(Self::RLC),
            "ant" => Ok(Self::ANT),
            "btc" => Ok(Self::BTC),
            "neo" => Ok(Self::NEO),
            "gas" => Ok(Self::GAS),
            "trac" => Ok(Self::TRAC),
            "gunthy" => Ok(Self::GUNTHY),
            "nnn" => Ok(Self::NNN),
            _ => Err(ProtocolError("Asset not known")),
        }
    }

    // FIXME: can this be more cleverly automated?
    /// Return list of all supported `Asset` types
    pub fn assets() -> Vec<Self> {
        vec![
            Self::ETH,
            Self::USDC,
            Self::USDT,
            Self::BAT,
            Self::OMG,
            Self::ZRX,
            Self::LINK,
            Self::QNT,
            Self::ANT,
            Self::BTC,
            Self::NEO,
            Self::GAS,
            Self::TRAC,
            Self::GUNTHY,
            Self::NNN,
        ]
    }
}

/// Assets can potentially have different precisions across markets.
/// This keeps track of what precision we are dealing with
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssetofPrecision {
    pub asset: Asset,
    pub precision: u32,
}

/// Convert AssetOfPrecision back into asset, e.g., for encoding
impl Into<Asset> for AssetofPrecision {
    fn into(self) -> Asset {
        self.asset
    }
}

impl AssetofPrecision {
    /// Starting with an asset of some precision, create a new asset that holds
    /// a specific amount of value
    pub fn with_amount(&self, amount_str: &str) -> Result<AssetAmount> {
        let amount = Amount::new(amount_str, self.precision)?;
        Ok(AssetAmount {
            asset: *self,
            amount,
        })
    }
}

// Extending Asset here with helper to convert with precision
impl Asset {
    /// Starting with an asset, create a new asset of desired precision.
    pub fn with_precision(&self, precision: u32) -> AssetofPrecision {
        AssetofPrecision {
            asset: *self,
            precision,
        }
    }
}

/// A specific amount of an asset being traded
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetAmount {
    pub asset: AssetofPrecision,
    pub amount: Amount,
}

impl AssetAmount {
    /// Create a new amount based on an exchange of `self` into `into_asset` at `rate`
    pub fn exchange_at(&self, rate: &Rate, into_asset: AssetofPrecision) -> Result<AssetAmount> {
        let new_amount = self.amount.to_bigdecimal() * rate.to_bigdecimal()?;
        Ok(AssetAmount {
            asset: into_asset,
            amount: Amount::from_bigdecimal(new_amount, into_asset.precision),
        })
    }
}

/// This type encodes all the information necessary for a client operating
/// over the protocol to understand a market.
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Market {
    // FIXME: probably more things here?
    pub asset_a: AssetofPrecision,
    pub asset_b: AssetofPrecision,
}

impl Market {
    /// Create a new market from assets with precision
    /// ```
    /// use nash_protocol::types::{Market, Asset};
    /// Market::new(Asset::ETH.with_precision(4), Asset::USDC.with_precision(2));
    /// ```
    pub fn new(asset_a: AssetofPrecision, asset_b: AssetofPrecision) -> Self {
        Self { asset_a, asset_b }
    }

    /// Return name of A/B market as "A_B"
    pub fn market_name(&self) -> String {
        format!(
            "{}_{}",
            self.asset_a.asset.name(),
            self.asset_b.asset.name()
        )
    }

    /// Get list of blockchains associated with this market. There will always be
    /// either one or two blockchains associated with a market.
    pub fn blockchains(&self) -> Vec<Blockchain> {
        let chain_a = self.asset_a.asset.blockchain();
        let chain_b = self.asset_b.asset.blockchain();
        if chain_a == chain_b {
            vec![chain_a]
        } else {
            vec![chain_a, chain_b]
        }
    }

    /// Get market asset by string name
    pub fn get_asset(&self, asset_name: &str) -> Result<AssetofPrecision> {
        if asset_name == self.asset_a.asset.name() {
            Ok(self.asset_a.clone())
        } else if asset_name == self.asset_b.asset.name() {
            Ok(self.asset_b.clone())
        } else {
            Err(ProtocolError("Asset not associated with market"))
        }
    }

    /// Return BTC/USDC `Market`
    pub fn btc_usdc() -> Self {
        Market::new(Asset::BTC.with_precision(8), Asset::USDC.with_precision(1))
    }

    /// Return ETH/USDC `Market`
    pub fn eth_usdc() -> Self {
        Market::new(Asset::ETH.with_precision(4), Asset::USDC.with_precision(2))
    }

    /// Return NEO/USDC `Market`
    pub fn neo_usdc() -> Self {
        Market::new(Asset::NEO.with_precision(3), Asset::USDC.with_precision(2))
    }

    /// Return ETH/BTC `Market`
    pub fn eth_btc() -> Self {
        Market::new(Asset::ETH.with_precision(6), Asset::BTC.with_precision(5))
    }

    /// Create a market object from an string. Todo: add the rest of the markets
    pub fn from_str(market_str: &str) -> Result<Self> {
        match market_str {
            "btc_usdc" => Ok(Self::btc_usdc()),
            "eth_usdc" => Ok(Self::eth_usdc()),
            "neo_usdc" => Ok(Self::neo_usdc()),
            "eth_btc" => Ok(Self::eth_btc()),
            _ => Err(ProtocolError("Market not supported")),
        }
    }
}

/// Buy or sell type for Nash protocol. We don't use the one generated automatically
/// from the GraphQL schema as it does not implement necessary traits like Clone
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuyOrSell {
    Buy,
    Sell,
}

/// Type of order execution in Nash ME
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    StopMarket,
    StopLimit,
}

impl std::fmt::Display for OrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// The Rate enum describes behavior common to rates/prices.

#[derive(Clone, Debug, PartialEq)]
pub enum Rate {
    OrderRate(OrderRate),
    MaxOrderRate,
    MinOrderRate,
    FeeRate(FeeRate),
    MaxFeeRate,
    MinFeeRate,
}

/// Enum wrappers are kind of annoying. This allows us to use .into()
/// on a value to generate a wrapped value. For example:
/// `OrderRate::new("1").unwrap().into() => Rate::OrderRate(...)`

impl From<OrderRate> for Rate {
    fn from(rate: OrderRate) -> Self {
        Self::OrderRate(rate)
    }
}

impl Rate {
    /// Return new bigdecimal inner value based on Rate
    pub fn to_bigdecimal(&self) -> Result<BigDecimal> {
        let num = match self {
            Self::FeeRate(rate) | Self::OrderRate(rate) => rate.inner.clone(),
            // FIXME: this could be wrong
            Self::MaxOrderRate | Self::MaxFeeRate => {
                // FIXME: be bytes could be wrong for NEO
                BigDecimal::from_str("0.0025").unwrap()
            }
            Self::MinOrderRate | Self::MinFeeRate => 0.into(),
        };
        Ok(num)
    }

    pub fn invert_rate(&self, precision: Option<u32>) -> Result<Self> {
        match self {
            Self::OrderRate(rate) => Ok(Self::OrderRate(rate.invert_rate(precision))),
            _ => Err(ProtocolError(
                "Cannot invert a Rate that is not an OrderRate",
            )),
        }
    }

    /// Subtract fee from user by adjusting the order rate downwards
    pub fn subtract_fee(&self, fee: BigDecimal) -> Result<OrderRate> {
        let as_order_rate = OrderRate {
            inner: self.to_bigdecimal()?,
        };
        Ok(as_order_rate.subtract_fee(fee))
    }
}

/// Order rates impose limits on what trades the smart contract is allowed to
/// execute. For example, the smart contract will reject a payload that requests
/// a less favorable amount of asset_to than is imposed by the minimum order rate.
/// For Sell orders, an order rate represented by currency B an in A/B market. For
/// Buy orders, an order rate is represented in the protocol by currency A. Note
/// that OrderRates are always created w.r.t. currency B, then potentially inverted
/// during payload creation. This is because, unlike the smart contract, the ME
/// always wants order prices expressed in currency B. In either case, these rates
/// are encoded as 64 bit integers which take the initial price ratio, multiply
/// by 10^8, and then drop all precision after the decimal.

#[derive(Clone, Debug, PartialEq)]
pub struct OrderRate {
    inner: BigDecimal,
}

impl OrderRate {
    /// Construct a new OrderRate from a numerical string
    pub fn new(str_num: &str) -> Result<Self> {
        BigDecimal::from_str(str_num)
            .map_err(|_| ProtocolError("String to BigDecimal failed in creating OrderRate"))
            .map(|inner| Self { inner })
    }

    /// Create new OrderRate from bigdecimal
    pub fn from_bigdecimal(decimal: BigDecimal) -> Self {
        Self { inner: decimal }
    }

    /// Invert the price to units of the other market pair. For example, if price is
    /// in terms of ETH in an ETH/USDC market, this will convert it to terms of USDC
    pub fn invert_rate(&self, precision: Option<u32>) -> Self {
        let mut inverse = self.inner.inverse();
        if let Some(precision) = precision {
            let scale_num = BigDecimal::from(u64::pow(10, precision));
            inverse = (&self.inner * &scale_num).with_scale(0) / scale_num;
        }
        Self { inner: inverse }
    }

    /// Return a new `BigDecimal` based on `OrderRate`
    pub fn to_bigdecimal(&self) -> BigDecimal {
        self.inner.clone()
    }

    /// Subtract fee from user by adjusting the order rate downwards. This will keep track of as
    /// much precision as BigDecimal is capable of. However, this method is exclusively used by
    /// the smart contract and will be reduced to an integer in scale of 10^8 before encoding
    pub fn subtract_fee(&self, fee: BigDecimal) -> Self {
        let fee_multiplier = BigDecimal::from(1) - fee;
        let inner = &self.inner * &fee_multiplier;
        OrderRate { inner }
    }
}

type FeeRate = OrderRate;

/// Amount encodes the amount of asset being bought or sold in an order
/// It is encoded with a precision that depends on the market and asset
/// being traded. For example, in the ETH/USD market, ETH has a precision
/// of 4. In an A/B market, amount is always in units of A.

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Amount {
    pub precision: u32,
    pub value: BigDecimal,
}

impl Amount {
    /// Construct a new Amount from string and precision
    pub fn new(str_num: &str, precision: u32) -> Result<Self> {
        let value = BigDecimal::from_str(str_num)
            .map_err(|_| ProtocolError("String to BigDecimal failed in creating Amount"))?;
        let adjust_precision = bigdecimal_to_nash_prec(&value, precision);
        Ok(Self { value: adjust_precision, precision })
    }

    pub fn from_bigdecimal(value: BigDecimal, precision: u32) -> Self {
        Self { value, precision }
    }

    // FIXME: this is a helper for rate conversion. Can be improved
    pub fn to_bigdecimal(&self) -> BigDecimal {
        self.value.clone()
    }
}

/// Nonces are 32 bit integers. They increment over time such that data
/// with lower nonces that has already been observed are rejected by the
/// matching engine and smart contract.

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
pub enum Nonce {
    Value(u32),
    Crosschain,
}

impl Nonce {
    pub fn crosschain() -> u32 {
        0xffff_ffff
    }
}

// GraphQL wants this as i64
impl Into<i64> for Nonce {
    fn into(self) -> i64 {
        match self {
            Self::Value(value) => value as i64,
            Self::Crosschain => Nonce::crosschain() as i64,
        }
    }
}

impl Into<u32> for Nonce {
    fn into(self) -> u32 {
        match self {
            Self::Value(value) => value as u32,
            Self::Crosschain => Nonce::crosschain() as u32,
        }
    }
}

impl From<u32> for Nonce {
    fn from(val: u32) -> Self {
        if val == Nonce::crosschain() {
            Self::Crosschain
        } else {
            Self::Value(val)
        }
    }
}

impl From<&u32> for Nonce {
    fn from(val: &u32) -> Self {
        if val == &Nonce::crosschain() {
            Self::Crosschain
        } else {
            Self::Value(*val)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CandleInterval {
    FifteenMinute,
    FiveMinute,
    FourHour,
    OneDay,
    OneHour,
    OneMinute,
    OneMonth,
    OneWeek,
    SixHour,
    ThirtyMinute,
    ThreeHour,
    TwelveHour,
}

#[derive(Debug)]
pub struct Candle {
    pub a_volume: AssetAmount,
    pub b_volume: AssetAmount,
    pub close_price: AssetAmount,
    pub high_price: AssetAmount,
    pub low_price: AssetAmount,
    pub open_price: AssetAmount,
    pub interval: CandleInterval,
    pub interval_start: DateTime<Utc>,
}
#[derive(Clone, Copy, Debug)]
pub struct DateTimeRange {
    pub start: DateTime<Utc>,
    pub stop: DateTime<Utc>,
}

/// Status of an order on Nash
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    Open,
    Filled,
    Canceled,
}

/// Relation of an account to a trade, whether maker, taker, or not related (none)
#[derive(Clone, Debug, PartialEq)]
pub enum AccountTradeSide {
    Maker,
    Taker,
    // account played no role in this trade
    None,
}

#[derive(Clone, Debug)]
pub struct Trade {
    pub id: String,
    pub taker_order_id: String,
    pub maker_order_id: String,
    pub amount: AssetAmount,
    pub executed_at: DateTime<Utc>,
    pub account_side: AccountTradeSide,
    pub maker_fee: AssetAmount,
    pub taker_fee: AssetAmount,
    pub maker_recieved: AssetAmount,
    pub taker_recieved: AssetAmount,
    pub market: Market,
    pub direction: BuyOrSell,
    pub limit_price: AssetAmount,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrderCancellationPolicy {
    FillOrKill,
    GoodTilCancelled,
    GoodTilTime(DateTime<Utc>),
    ImmediateOrCancel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrderCancellationReason {
    AdminCancelled,
    Expiration,
    InvalidForOrderbookState,
    NoFill,
    User,
}

#[derive(Clone, Debug)]
pub struct Order {
    pub id: String,
    // Amount the order was placed for
    pub amount_placed: AssetAmount,
    // Amount remaining in order
    pub amount_remaining: AssetAmount,
    // Amount executed in order
    pub amount_executed: AssetAmount,
    pub limit_price: Option<AssetAmount>,
    pub stop_price: Option<AssetAmount>,
    pub placed_at: DateTime<Utc>,
    pub buy_or_sell: BuyOrSell,
    pub cancellation_policy: Option<OrderCancellationPolicy>,
    pub cancellation_reason: Option<OrderCancellationReason>,
    pub market: Market,
    pub order_type: OrderType,
    pub status: OrderStatus,
    pub trades: Vec<Trade>,
}

/// Compressed representation for Order as returned by Orderbook queries and subscriptions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderbookOrder {
    pub price: String,
    pub amount: AssetAmount,
}

#[cfg(test)]
mod tests {
    use super::{BigDecimal, FromStr, OrderRate};
    #[test]
    fn fee_rate_conversion_precision() {
        let rate = OrderRate::new("150").unwrap();
        let inverted_rate = rate.invert_rate(None);
        let minus_fee = inverted_rate.subtract_fee(BigDecimal::from_str("0.0025").unwrap());
        let payload = minus_fee.to_be_bytes().unwrap();
        assert_eq!(665000, u64::from_be_bytes(payload));
    }
}
