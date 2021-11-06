pub use crate as nash_protocol;
use openlimits_exchange::model::{OrderBookRequest, OrderBookResponse, AskBid, CancelOrderRequest, OrderCanceled, CancelAllOrdersRequest, OrderType, Order, TradeHistoryRequest, Trade, Side, Liquidity, GetHistoricRatesRequest, GetHistoricTradesRequest, Interval, Candle, GetOrderHistoryRequest, OrderStatus, GetPriceTickerRequest, Ticker, GetOrderRequest, TimeInForce, Paginator};
use openlimits_exchange::{OpenLimitsError, MissingImplementationContent};
use openlimits_exchange::model::websocket::{AccountOrders, Subscription};
use openlimits_exchange::shared::{Result, timestamp_to_utc_datetime};
use rust_decimal::Decimal;
use std::convert::{TryFrom, TryInto};
use crate::types::{BuyOrSell, DateTimeRange};
use crate::protocol::subscriptions::updated_account_orders::SubscribeAccountOrders;
use chrono::Utc;
use std::str::FromStr;
use crate::types::market_pair::MarketPair;

pub fn try_split_paginator(
    paginator: Option<Paginator>,
) -> Result<(
    Option<String>,
    Option<i64>,
    Option<nash_protocol::types::DateTimeRange>,
)> {
    Ok(match paginator {
        Some(paginator) => (
            paginator.before,
            match paginator.limit {
                Some(v) => Some(i64::try_from(v).map_err(|_| {
                    OpenLimitsError::InvalidParameter(
                        "Couldn't convert paginator limit to i64".to_string(),
                    )
                })?),
                None => None,
            },
            if paginator.start_time.is_some() && paginator.end_time.is_some() {
                Some(nash_protocol::types::DateTimeRange {
                    start: paginator.start_time.map(timestamp_to_utc_datetime).unwrap(),
                    stop: paginator.end_time.map(timestamp_to_utc_datetime).unwrap(),
                })
            } else {
                None
            },
        ),
        None => (None, None, None),
    })
}

impl From<&OrderBookRequest> for nash_protocol::protocol::orderbook::OrderbookRequest {
    fn from(req: &OrderBookRequest) -> Self {
        let market = req.market_pair.clone();
        let market = MarketPair::from(market).0;
        Self { market }
    }
}

impl From<nash_protocol::protocol::orderbook::OrderbookResponse> for OrderBookResponse {
    fn from(book: nash_protocol::protocol::orderbook::OrderbookResponse) -> Self {
        Self {
            update_id: Some(book.update_id as u64),
            last_update_id: Some(book.last_update_id as u64),
            bids: book.bids.into_iter().map(Into::into).collect(),
            asks: book.asks.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<nash_protocol::types::OrderbookOrder> for AskBid {
    fn from(resp: nash_protocol::types::OrderbookOrder) -> Self {
        let price = Decimal::from_str(&resp.price).expect("Couldn't parse Decimal from string.");
        let qty = Decimal::from_str(&resp.amount.to_string())
            .expect("Couldn't parse Decimal from string.");
        Self { price, qty }
    }
}

impl From<&CancelOrderRequest> for nash_protocol::protocol::cancel_order::CancelOrderRequest {
    fn from(req: &CancelOrderRequest) -> Self {
        // TODO: why this param?
        let market = req.market_pair.clone().expect("Couldn't get market_pair.");

        Self {
            market,
            order_id: req.id.clone(),
        }
    }
}

impl From<nash_protocol::protocol::cancel_order::CancelOrderResponse> for OrderCanceled {
    fn from(resp: nash_protocol::protocol::cancel_order::CancelOrderResponse) -> Self {
        Self { id: resp.order_id }
    }
}

impl From<&CancelAllOrdersRequest> for nash_protocol::protocol::cancel_all_orders::CancelAllOrders {
    fn from(req: &CancelAllOrdersRequest) -> Self {
        // TODO: why is this required param for Nash?
        let market = req
            .market_pair
            .clone()
            .expect("Market pair is a required param for Nash");
        let market = MarketPair::from(market).0;
        Self { market }
    }
}

impl From<nash_protocol::types::OrderType> for OrderType {
    fn from(order_type: nash_protocol::types::OrderType) -> Self {
        match order_type {
            nash_protocol::types::OrderType::Limit => OrderType::Limit,
            nash_protocol::types::OrderType::Market => OrderType::Market,
            nash_protocol::types::OrderType::StopLimit => OrderType::StopLimit,
            nash_protocol::types::OrderType::StopMarket => OrderType::StopMarket,
        }
    }
}

impl From<nash_protocol::protocol::place_order::PlaceOrderResponse> for Order {
    fn from(resp: nash_protocol::protocol::place_order::PlaceOrderResponse) -> Self {
        Self {
            id: resp.order_id,
            market_pair: resp.market.name,
            client_order_id: None,
            created_at: Some(resp.placed_at.timestamp_millis() as u64),
            order_type: resp.order_type.into(),
            side: resp.buy_or_sell.into(),
            status: resp.status.into(),
            size: Decimal::from(0),
            price: None,
            remaining: None,
            trades: Vec::new(),
        }
    }
}

impl TryFrom<&TradeHistoryRequest>
for nash_protocol::protocol::list_account_trades::ListAccountTradesRequest
{
    type Error = OpenLimitsError;
    fn try_from(req: &TradeHistoryRequest) -> openlimits_exchange::shared::Result<Self> {
        let (before, limit, range) = try_split_paginator(req.paginator.clone())?;
        let market = req.market_pair.clone();
        let market = market.map(|market| MarketPair::from(market).0);
        Ok(Self { market, before, limit, range })
    }
}

impl From<nash_protocol::types::Trade> for Trade {
    fn from(resp: nash_protocol::types::Trade) -> Self {
        let qty = Decimal::from_str(&resp.amount.to_string())
            .expect("Couldn't parse Decimal from string.");
        let price = Decimal::from_str(&resp.limit_price.to_string())
            .expect("Couldn't parse Decimal from string.");

        let fees = match resp.account_side {
            nash_protocol::types::AccountTradeSide::Taker => {
                Decimal::from_str(&resp.taker_fee.to_string())
                    .expect("Couldn't parse Decimal from string.")
            }
            _ => Decimal::from(0),
        };

        let (buyer_order_id, seller_order_id) = match resp.direction {
            nash_protocol::types::BuyOrSell::Buy => (resp.taker_order_id, resp.maker_order_id),
            nash_protocol::types::BuyOrSell::Sell => (resp.maker_order_id, resp.taker_order_id),
        };

        Self {
            id: resp.id,
            created_at: (resp.executed_at.timestamp_millis() as u64).to_string(),
            fees: Some(fees),
            liquidity: Some(resp.account_side.into()),
            market_pair: resp.market.clone(),
            buyer_order_id: Some(buyer_order_id),
            seller_order_id: Some(seller_order_id),
            price,
            qty,
            side: resp.direction.into(),
        }
    }
}

impl From<nash_protocol::types::BuyOrSell> for Side {
    fn from(side: nash_protocol::types::BuyOrSell) -> Self {
        match side {
            nash_protocol::types::BuyOrSell::Buy => Side::Buy,
            nash_protocol::types::BuyOrSell::Sell => Side::Sell,
        }
    }
}

impl From<nash_protocol::types::AccountTradeSide> for Liquidity {
    fn from(side: nash_protocol::types::AccountTradeSide) -> Self {
        match side {
            nash_protocol::types::AccountTradeSide::Taker => Liquidity::Taker,
            _ => Liquidity::Maker,
        }
    }
}

impl TryFrom<&GetHistoricRatesRequest>
for nash_protocol::protocol::list_candles::ListCandlesRequest
{
    type Error = OpenLimitsError;
    fn try_from(req: &GetHistoricRatesRequest) -> openlimits_exchange::shared::Result<Self> {
        let (before, limit, range) = try_split_paginator(req.paginator.clone())?;
        let market = req.market_pair.clone();
        let market = MarketPair::from(market).0;

        Ok(Self {
            market,
            chronological: None,
            before,
            interval: Some(
                req.interval
                    .try_into()
                    .expect("Couldn't convert Interval to CandleInterval."),
            ),
            limit,
            range,
        })
    }
}

impl TryFrom<&GetHistoricTradesRequest>
for nash_protocol::protocol::list_trades::ListTradesRequest
{
    type Error = OpenLimitsError;
    fn try_from(req: &GetHistoricTradesRequest) -> openlimits_exchange::shared::Result<Self> {
        let market = req.market_pair.clone();
        let (before, limit, _) = try_split_paginator(req.paginator.clone())?;
        //FIXME: Some issues with the graphql protocol for the market to be non nil
        Ok(Self {
            market,
            before,
            limit,
        })
    }
}

impl TryFrom<Interval> for nash_protocol::types::CandleInterval {
    type Error = OpenLimitsError;
    fn try_from(interval: Interval) -> openlimits_exchange::shared::Result<Self> {
        match interval {
            Interval::OneMinute => Ok(nash_protocol::types::CandleInterval::OneMinute),
            Interval::FiveMinutes => Ok(nash_protocol::types::CandleInterval::FiveMinute),
            Interval::FifteenMinutes => Ok(nash_protocol::types::CandleInterval::FifteenMinute),
            Interval::ThirtyMinutes => Ok(nash_protocol::types::CandleInterval::ThirtyMinute),
            Interval::OneHour => Ok(nash_protocol::types::CandleInterval::OneHour),
            Interval::SixHours => Ok(nash_protocol::types::CandleInterval::SixHour),
            Interval::TwelveHours => Ok(nash_protocol::types::CandleInterval::TwelveHour),
            Interval::OneDay => Ok(nash_protocol::types::CandleInterval::OneDay),
            _ => {
                let err = MissingImplementationContent {
                    message: String::from("Not supported interval"),
                };
                Err(OpenLimitsError::MissingImplementation(err))
            }
        }
    }
}

impl From<nash_protocol::types::Candle> for Candle {
    fn from(candle: nash_protocol::types::Candle) -> Self {
        let close = Decimal::from_str(&candle.close_price.to_string())
            .expect("Couldn't parse Decimal from string.");
        let high = Decimal::from_str(&candle.high_price.to_string())
            .expect("Couldn't parse Decimal from string.");
        let low = Decimal::from_str(&candle.low_price.to_string())
            .expect("Couldn't parse Decimal from string.");
        let open = Decimal::from_str(&candle.open_price.to_string())
            .expect("Couldn't parse Decimal from string.");
        let volume = Decimal::from_str(&candle.a_volume.to_string())
            .expect("Couldn't parse Decimal from string.");

        Self {
            close,
            high,
            low,
            open,
            time: candle.interval_start.timestamp_millis() as u64,
            volume,
        }
    }
}

impl TryFrom<&GetOrderHistoryRequest>
for nash_protocol::protocol::list_account_orders::ListAccountOrdersRequest
{
    type Error = OpenLimitsError;
    fn try_from(req: &GetOrderHistoryRequest) -> openlimits_exchange::shared::Result<Self> {
        let (before, limit, range) = try_split_paginator(req.paginator.clone())?;
        let market = req.market_pair.clone();
        let market = market.map(|market| MarketPair::from(market).0);

        Ok(Self {
            market,
            before,
            limit,
            range,
            buy_or_sell: None,
            order_type: None,
            status: match req.order_status.clone() {
                Some(v) => Some(
                    v.into_iter()
                        .map(TryInto::try_into)
                        .collect::<Result<Vec<nash_protocol::types::OrderStatus>>>()?,
                ),
                None => None,
            },
        })
    }
}

impl From<nash_protocol::types::Order> for Order {
    fn from(order: nash_protocol::types::Order) -> Self {
        let size = Decimal::from_str(&order.amount_placed.to_string())
            .expect("Couldn't parse Decimal from string.");
        let price = order
            .limit_price
            .map(|p| Decimal::from_str(&p.to_string()).unwrap());
        let remaining = Some(
            Decimal::from_str(&order.amount_remaining.to_string())
                .expect("Couldn't parse Decimal from string."),
        );

        Self {
            id: order.id,
            market_pair: order.market.clone(),
            client_order_id: None,
            created_at: Some(order.placed_at.timestamp_millis() as u64),
            order_type: order.order_type.into(),
            side: order.buy_or_sell.into(),
            status: order.status.into(),
            size,
            price,
            remaining,
            trades: order.trades.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<nash_protocol::types::OrderStatus> for OrderStatus {
    fn from(status: nash_protocol::types::OrderStatus) -> Self {
        match status {
            nash_protocol::types::OrderStatus::Filled => OrderStatus::Filled,
            nash_protocol::types::OrderStatus::Open => OrderStatus::Open,
            nash_protocol::types::OrderStatus::Canceled => OrderStatus::Canceled,
            nash_protocol::types::OrderStatus::Pending => OrderStatus::Pending,
        }
    }
}

impl TryFrom<OrderStatus> for nash_protocol::types::OrderStatus {
    type Error = OpenLimitsError;
    fn try_from(status: OrderStatus) -> openlimits_exchange::shared::Result<Self> {
        Ok(match status {
            OrderStatus::Filled => nash_protocol::types::OrderStatus::Filled,
            OrderStatus::Open => nash_protocol::types::OrderStatus::Open,
            OrderStatus::Canceled => nash_protocol::types::OrderStatus::Canceled,
            OrderStatus::Pending => nash_protocol::types::OrderStatus::Pending,
            _ => {
                return Err(OpenLimitsError::InvalidParameter(
                    "Had invalid order status for Nash".to_string(),
                ))
            }
        })
    }
}

impl From<&GetPriceTickerRequest> for nash_protocol::protocol::get_ticker::TickerRequest {
    fn from(req: &GetPriceTickerRequest) -> Self {
        let market = req.market_pair.clone();
        let market = MarketPair::from(market).0;

        Self { market }
    }
}

impl From<nash_protocol::protocol::get_ticker::TickerResponse> for Ticker {
    fn from(resp: nash_protocol::protocol::get_ticker::TickerResponse) -> Self {
        let mut price = None;
        if resp.best_ask_price.is_some() && resp.best_bid_price.is_some() {
            let ask = Decimal::from_str(&resp.best_ask_price.unwrap().to_string())
                .expect("Couldn't parse Decimal from string.");
            let bid = Decimal::from_str(&resp.best_bid_price.unwrap().to_string())
                .expect("Couldn't parse Decimal from string.");
            price = Some((ask + bid) / Decimal::from(2));
        }
        let mut price_24h = None;
        if resp.high_price_24h.is_some() && resp.low_price_24h.is_some() {
            let day_high = Decimal::from_str(
                &resp
                    .high_price_24h
                    .expect("Couldn't get high price 24h.")
                    .to_string(),
            )
                .expect("Couldn't parse Decimal from string.");
            let day_low = Decimal::from_str(
                &resp
                    .low_price_24h
                    .expect("Couldn't get low price 24h.")
                    .to_string(),
            )
                .expect("Couldn't parse Decimal from string.");
            price_24h = Some((day_high + day_low) / Decimal::from(2));
        }
        Self { price, price_24h }
    }
}

impl From<&GetOrderRequest> for nash_protocol::protocol::get_account_order::GetAccountOrderRequest {
    fn from(req: &GetOrderRequest) -> Self {
        Self {
            order_id: req.id.clone(),
        }
    }
}

impl From<Side> for BuyOrSell {
    fn from(side: Side) -> Self {
        match side {
            Side::Buy => BuyOrSell::Buy,
            Side::Sell => BuyOrSell::Sell,
        }
    }
}

impl TryFrom<OrderType> for nash_protocol::types::OrderType {
    type Error = OpenLimitsError;
    fn try_from(order_type: OrderType) -> Result<Self> {
        match order_type {
            OrderType::Limit => Ok(Self::Limit),
            OrderType::Market => Ok(Self::Market),
            OrderType::StopLimit => Ok(Self::StopLimit),
            OrderType::StopMarket => Ok(Self::StopMarket),
            OrderType::Unknown => Err(OpenLimitsError::InvalidParameter(
                "Had invalid order type for Nash".to_string(),
            )),
        }
    }
}

impl From<AccountOrders> for SubscribeAccountOrders {
    fn from(account_orders: AccountOrders) -> Self {
        let market = account_orders.market.map(|market| MarketPair::from(market.clone()).0);
        Self {
            market,
            order_type: account_orders.order_type.map(|x| {
                x.iter()
                    .cloned()
                    .map(|x| x.try_into().ok())
                    .filter(|x| x.is_some())
                    .map(|x| x.unwrap())
                    .collect()
            }),
            range: account_orders.range.map(|range| DateTimeRange {
                start: timestamp_to_utc_datetime(range.start),
                stop: timestamp_to_utc_datetime(range.end),
            }),
            buy_or_sell: account_orders.buy_or_sell.map(|x| x.into()),
            status: account_orders.status.map(|x| {
                x.iter()
                    .cloned()
                    .map(|x| x.try_into().ok())
                    .filter(|x| x.is_some())
                    .map(|x| x.unwrap())
                    .collect()
            }),
        }
    }
}

impl From<Subscription> for nash_protocol::protocol::subscriptions::SubscriptionRequest {
    fn from(sub: Subscription) -> Self {
        match sub {
            Subscription::OrderBookUpdates(market) => {
                let market = MarketPair::from(market).0;
                Self::Orderbook(
                    nash_protocol::protocol::subscriptions::updated_orderbook::SubscribeOrderbook {
                        market,
                    },
                )
            },
            Subscription::Trades(market) => {
                let market = MarketPair::from(market).0;
                Self::Trades(
                    nash_protocol::protocol::subscriptions::trades::SubscribeTrades { market },
                )
            },
            // Subscription::AccountOrders(account_orders) => Self::AccountOrders(
            //     account_orders.into()
            // ),
            // Subscription::AccountTrades(market_name) => {
            //     let market = MarketPair::from(market).0;
            //     Self::AccountTrades(
            //         nash_protocol::protocol::subscriptions::new_account_trades::SubscribeAccountTrades {
            //             market_name
            //         }
            //     )
            // },
            // Subscription::AccountBalance(symbol) => Self::AccountBalances(
            //     nash_protocol::protocol::subscriptions::updated_account_balances::SubscribeAccountBalances {
            //         symbol
            //     }
            // ),
        }
    }
}

impl From<TimeInForce> for nash_protocol::types::OrderCancellationPolicy {
    fn from(tif: TimeInForce) -> Self {
        match tif {
            TimeInForce::GoodTillCancelled => {
                nash_protocol::types::OrderCancellationPolicy::GoodTilCancelled
            }
            TimeInForce::FillOrKill => nash_protocol::types::OrderCancellationPolicy::FillOrKill,
            TimeInForce::ImmediateOrCancelled => {
                nash_protocol::types::OrderCancellationPolicy::ImmediateOrCancel
            }
            TimeInForce::GoodTillTime(duration) => {
                let expire_time = Utc::now() + duration;
                nash_protocol::types::OrderCancellationPolicy::GoodTilTime(expire_time)
            }
        }
    }
}

impl From<nash_protocol::errors::ProtocolError> for openlimits_exchange::OpenLimitsError {
    fn from(error: nash_protocol::errors::ProtocolError) -> Self {
        Self::Generic(Box::new(error))
    }
}