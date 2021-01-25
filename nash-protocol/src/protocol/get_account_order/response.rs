use super::types::GetAccountOrderResponse;
use crate::errors::{ProtocolError, Result};
use crate::graphql::get_account_order;
use crate::types::{
    AccountTradeSide, BuyOrSell, Order, OrderCancellationPolicy, OrderCancellationReason,
    OrderStatus, OrderType, Trade,
};
use crate::protocol::{State, traits::TryFromState};
use chrono::{DateTime, Utc};
use std::str::FromStr;
use bigdecimal::BigDecimal;
use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;

#[async_trait]
impl TryFromState<get_account_order::ResponseData> for GetAccountOrderResponse {
    async fn from(response: get_account_order::ResponseData, _state: Arc<RwLock<State>>) -> Result<GetAccountOrderResponse> {
        let order_data = response.get_account_order;
        let market = order_data.market.name.clone();
        let amount_placed = BigDecimal::from_str(&order_data.amount.amount)?;
        let amount_remaining = BigDecimal::from_str(&order_data.amount_remaining.amount)?;
        let amount_executed = BigDecimal::from_str(&order_data.amount_executed.amount)?;
        let limit_price = match &order_data.limit_price {
            Some(price) => Some(BigDecimal::from_str(&price.amount)?),
            None => None,
        };
        let stop_price = match &order_data.stop_price {
            Some(price) => Some(BigDecimal::from_str(&price.amount)?),
            None => None,
        };
        let placed_at = DateTime::<Utc>::from_str(&order_data.placed_at)
            .map_err(|_| ProtocolError("Could not convert value to DateTime"))?;
        let mut trades = Vec::new();
        // These wraps are safe. ME_FIXME
        for trade_data in order_data.trades.as_ref().unwrap() {
            let trade_data = trade_data.as_ref().unwrap();
            let market = trade_data.market.name.clone();
            let taker_fee = BigDecimal::from_str(&trade_data.taker_fee.amount)?;
            let maker_fee = BigDecimal::from_str(&trade_data.maker_fee.amount)?;
            let amount = BigDecimal::from_str(&trade_data.amount.amount)?;
            let maker_recieved = BigDecimal::from_str(&trade_data.maker_received.amount)?;
            let taker_recieved = BigDecimal::from_str(&trade_data.taker_received.amount)?;
            let limit_price = BigDecimal::from_str(&trade_data.limit_price.amount)?;
            trades.push(Trade {
                market,
                amount,
                taker_fee,
                maker_fee,
                maker_recieved,
                taker_recieved,
                taker_order_id: trade_data.taker_order_id.clone(),
                maker_order_id: trade_data.maker_order_id.clone(),
                account_side: (&trade_data.account_side).into(),
                id: trade_data.id.clone(),
                executed_at: DateTime::<Utc>::from_str(&trade_data.executed_at)
                    .map_err(|_| ProtocolError("Could not convert value to DateTime"))?,
                limit_price,
                direction: (&trade_data.direction).into(),
            })
        }
        Ok(GetAccountOrderResponse {
            order: Order {
                id: order_data.id.clone(),
                client_order_id: order_data.client_order_id.clone(),
                market,
                amount_placed,
                amount_remaining,
                amount_executed,
                limit_price,
                stop_price,
                placed_at,
                trades,
                cancellation_policy: (&order_data).into(),
                cancellation_reason: order_data.cancellation_reason.as_ref().map(|x| x.into()),
                buy_or_sell: order_data.buy_or_sell.into(),
                order_type: order_data.type_.into(),
                status: order_data.status.into(),
            },
        })
    }
}

impl From<get_account_order::OrderBuyOrSell> for BuyOrSell {
    fn from(response: get_account_order::OrderBuyOrSell) -> Self {
        match response {
            get_account_order::OrderBuyOrSell::BUY => Self::Buy,
            get_account_order::OrderBuyOrSell::SELL => Self::Sell,
            _ => panic!("Unexpected value in BuyOrSell enum"),
        }
    }
}

impl From<get_account_order::OrderType> for OrderType {
    fn from(response: get_account_order::OrderType) -> Self {
        match response {
            get_account_order::OrderType::MARKET => Self::Market,
            get_account_order::OrderType::LIMIT => Self::Limit,
            get_account_order::OrderType::STOP_MARKET => Self::StopMarket,
            get_account_order::OrderType::STOP_LIMIT => Self::StopLimit,
            _ => panic!("Unexpected value in OrderType enum"),
        }
    }
}

impl From<get_account_order::OrderStatus> for OrderStatus {
    fn from(response: get_account_order::OrderStatus) -> Self {
        match response {
            get_account_order::OrderStatus::PENDING => OrderStatus::Pending,
            get_account_order::OrderStatus::OPEN => OrderStatus::Open,
            get_account_order::OrderStatus::CANCELLED => OrderStatus::Canceled,
            get_account_order::OrderStatus::FILLED => OrderStatus::Filled,
            _ => panic!("Unexpected value in OrderStatus enum"),
        }
    }
}

impl From<&get_account_order::OrderCancellationReason> for OrderCancellationReason {
    fn from(cancellation_reason: &get_account_order::OrderCancellationReason) -> Self {
        match cancellation_reason {
            get_account_order::OrderCancellationReason::ADMIN_CANCELED => {
                OrderCancellationReason::AdminCancelled
            }
            get_account_order::OrderCancellationReason::EXPIRATION => {
                OrderCancellationReason::Expiration
            }
            get_account_order::OrderCancellationReason::USER => OrderCancellationReason::User,
            get_account_order::OrderCancellationReason::NO_FILL => OrderCancellationReason::NoFill,
            get_account_order::OrderCancellationReason::INVALID_FOR_ORDERBOOK_STATE => {
                OrderCancellationReason::InvalidForOrderbookState
            }
            _ => panic!("Unsupported OrderCancellationReason"),
        }
    }
}

impl From<&get_account_order::GetAccountOrderGetAccountOrder> for Option<OrderCancellationPolicy> {
    fn from(order: &get_account_order::GetAccountOrderGetAccountOrder) -> Self {
        if let Some(cancellation_policy) = &order.cancellation_policy {
            Some(match cancellation_policy {
                get_account_order::OrderCancellationPolicy::FILL_OR_KILL => {
                    OrderCancellationPolicy::FillOrKill
                }
                get_account_order::OrderCancellationPolicy::GOOD_TIL_CANCELLED => {
                    OrderCancellationPolicy::GoodTilCancelled
                }
                get_account_order::OrderCancellationPolicy::GOOD_TIL_TIME => {
                    // This unwrap is safe here. GoodTilTime must have an associated time
                    let cancel_at =
                        DateTime::<Utc>::from_str(order.cancel_at.as_ref().unwrap()).unwrap();
                    OrderCancellationPolicy::GoodTilTime(cancel_at)
                }
                get_account_order::OrderCancellationPolicy::IMMEDIATE_OR_CANCEL => {
                    OrderCancellationPolicy::ImmediateOrCancel
                }
                _ => panic!("Unsupported OrderCancellationPolicy"),
            })
        } else {
            None
        }
    }
}

impl From<&get_account_order::AccountTradeSide> for AccountTradeSide {
    fn from(trade_side: &get_account_order::AccountTradeSide) -> Self {
        match trade_side {
            get_account_order::AccountTradeSide::MAKER => AccountTradeSide::Maker,
            get_account_order::AccountTradeSide::TAKER => AccountTradeSide::Taker,
            get_account_order::AccountTradeSide::NONE => AccountTradeSide::None,
            _ => panic!("Unsupported value in AccountTradeSide"),
        }
    }
}

impl From<&get_account_order::Direction> for BuyOrSell {
    fn from(response: &get_account_order::Direction) -> Self {
        match response {
            get_account_order::Direction::BUY => Self::Buy,
            get_account_order::Direction::SELL => Self::Sell,
            _ => panic!("Unexpected value in BuyOrSell enum"),
        }
    }
}
