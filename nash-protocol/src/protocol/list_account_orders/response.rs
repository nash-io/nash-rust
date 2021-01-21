use super::types::ListAccountOrdersResponse;
use crate::errors::{ProtocolError, Result};
use crate::graphql::list_account_orders;
use crate::types::{
    AccountTradeSide, BuyOrSell, Order, OrderCancellationPolicy, OrderCancellationReason,
    OrderStatus, OrderType, Trade,
};
use chrono::{DateTime, Utc};
use bigdecimal::BigDecimal;
use std::str::FromStr;
use crate::protocol::traits::TryFromState;
use crate::protocol::state::State;
use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;

#[async_trait]
impl TryFromState<list_account_orders::ResponseData> for ListAccountOrdersResponse {
    async fn from(response: list_account_orders::ResponseData, _state: Arc<RwLock<State>>) -> Result<ListAccountOrdersResponse> {
        // FIXME (if possible): there is significant duplication of code between here and the response handles for list_account_trades
        // and get_account_order. unfortunately, given the way the graphql_client library works, it is not obvious how to make this
        // response processing generic
        let mut orders = Vec::new();
        for order_data in response.list_account_orders.orders {
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
            orders.push(Order {
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
            })
        }
        Ok(ListAccountOrdersResponse {
            orders: orders,
            next_page: response.list_account_orders.next.clone(),
        })
    }
}

impl From<list_account_orders::OrderBuyOrSell> for BuyOrSell {
    fn from(response: list_account_orders::OrderBuyOrSell) -> Self {
        match response {
            list_account_orders::OrderBuyOrSell::BUY => Self::Buy,
            list_account_orders::OrderBuyOrSell::SELL => Self::Sell,
            _ => panic!("Unexpected value in BuyOrSell enum"),
        }
    }
}

impl From<list_account_orders::OrderType> for OrderType {
    fn from(response: list_account_orders::OrderType) -> Self {
        match response {
            list_account_orders::OrderType::MARKET => Self::Market,
            list_account_orders::OrderType::LIMIT => Self::Limit,
            list_account_orders::OrderType::STOP_MARKET => Self::StopMarket,
            list_account_orders::OrderType::STOP_LIMIT => Self::StopLimit,
            _ => panic!("Unexpected value in OrderType enum"),
        }
    }
}

impl From<list_account_orders::OrderStatus> for OrderStatus {
    fn from(response: list_account_orders::OrderStatus) -> Self {
        match response {
            list_account_orders::OrderStatus::PENDING => OrderStatus::Pending,
            list_account_orders::OrderStatus::OPEN => OrderStatus::Open,
            list_account_orders::OrderStatus::CANCELLED => OrderStatus::Canceled,
            list_account_orders::OrderStatus::FILLED => OrderStatus::Filled,
            _ => panic!("Unexpected value in OrderStatus enum"),
        }
    }
}

impl From<&list_account_orders::OrderCancellationReason> for OrderCancellationReason {
    fn from(cancellation_reason: &list_account_orders::OrderCancellationReason) -> Self {
        match cancellation_reason {
            list_account_orders::OrderCancellationReason::ADMIN_CANCELED => {
                OrderCancellationReason::AdminCancelled
            }
            list_account_orders::OrderCancellationReason::EXPIRATION => {
                OrderCancellationReason::Expiration
            }
            list_account_orders::OrderCancellationReason::USER => OrderCancellationReason::User,
            list_account_orders::OrderCancellationReason::NO_FILL => {
                OrderCancellationReason::NoFill
            }
            list_account_orders::OrderCancellationReason::INVALID_FOR_ORDERBOOK_STATE => {
                OrderCancellationReason::InvalidForOrderbookState
            }
            _ => panic!("Unsupported OrderCancellationReason"),
        }
    }
}

impl From<&list_account_orders::ListAccountOrdersListAccountOrdersOrders>
    for Option<OrderCancellationPolicy>
{
    fn from(order: &list_account_orders::ListAccountOrdersListAccountOrdersOrders) -> Self {
        if let Some(cancellation_policy) = &order.cancellation_policy {
            Some(match cancellation_policy {
                list_account_orders::OrderCancellationPolicy::FILL_OR_KILL => {
                    OrderCancellationPolicy::FillOrKill
                }
                list_account_orders::OrderCancellationPolicy::GOOD_TIL_CANCELLED => {
                    OrderCancellationPolicy::GoodTilCancelled
                }
                list_account_orders::OrderCancellationPolicy::GOOD_TIL_TIME => {
                    // This unwrap is safe
                    let cancel_at =
                        DateTime::<Utc>::from_str(&order.cancel_at.as_ref().unwrap()).unwrap();
                    OrderCancellationPolicy::GoodTilTime(cancel_at)
                }
                list_account_orders::OrderCancellationPolicy::IMMEDIATE_OR_CANCEL => {
                    OrderCancellationPolicy::ImmediateOrCancel
                }
                _ => panic!("Unsupported OrderCancellationPolicy"),
            })
        } else {
            None
        }
    }
}

impl From<&list_account_orders::AccountTradeSide> for AccountTradeSide {
    fn from(trade_side: &list_account_orders::AccountTradeSide) -> Self {
        match trade_side {
            list_account_orders::AccountTradeSide::MAKER => AccountTradeSide::Maker,
            list_account_orders::AccountTradeSide::TAKER => AccountTradeSide::Taker,
            list_account_orders::AccountTradeSide::NONE => AccountTradeSide::None,
            _ => panic!("Unsupported value in AccountTradeSide"),
        }
    }
}

impl From<&list_account_orders::Direction> for BuyOrSell {
    fn from(response: &list_account_orders::Direction) -> Self {
        match response {
            list_account_orders::Direction::BUY => Self::Buy,
            list_account_orders::Direction::SELL => Self::Sell,
            _ => panic!("Unexpected value in BuyOrSell enum"),
        }
    }
}
