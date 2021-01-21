use super::super::super::{DataResponse, ResponseOrError};
use super::request::SubscribeAccountOrders;
use crate::errors::{ProtocolError, Result};
use crate::types::{
    BuyOrSell, Order, OrderCancellationPolicy, OrderCancellationReason,
    OrderStatus, OrderType,
};
use crate::graphql::updated_account_orders;
use crate::protocol::state::State;
use std::sync::Arc;
use tokio::sync::RwLock;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use std::str::FromStr;

/// Order book updates pushed over a subscription consist of a list of bid orders and
/// a list of ask orders.
#[derive(Clone, Debug)]
pub struct AccountOrdersResponse {
    pub orders: Vec<Order>,
}

// FIXME: if possible, remove duplication with orderbook query
impl SubscribeAccountOrders {
    pub async fn response_from_graphql(
        &self,
        response: ResponseOrError<updated_account_orders::ResponseData>,
        _state: Arc<RwLock<State>>
    ) -> Result<ResponseOrError<AccountOrdersResponse>> {
        match response {
            ResponseOrError::Response(data) => {
                let response = data.data;

                let mut orders = Vec::new();
                for order_data in response.updated_account_orders {
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
                        trades: Vec::new(),
                        cancellation_policy: (&order_data).into(),
                        cancellation_reason: order_data.cancellation_reason.as_ref().map(|x| x.into()),
                        buy_or_sell: order_data.buy_or_sell.into(),
                        order_type: order_data.type_.into(),
                        status: order_data.status.into(),
                    })
                }
                Ok(ResponseOrError::Response(DataResponse {
                    data: AccountOrdersResponse {
                        orders,
                    },
                }))
            }
            ResponseOrError::Error(error) => Ok(ResponseOrError::Error(error)),
        }
    }
}

impl From<updated_account_orders::OrderBuyOrSell> for BuyOrSell {
    fn from(response: updated_account_orders::OrderBuyOrSell) -> Self {
        match response {
            updated_account_orders::OrderBuyOrSell::BUY => Self::Buy,
            updated_account_orders::OrderBuyOrSell::SELL => Self::Sell,
            _ => panic!("Unexpected value in BuyOrSell enum"),
        }
    }
}

impl From<updated_account_orders::OrderType> for OrderType {
    fn from(response: updated_account_orders::OrderType) -> Self {
        match response {
            updated_account_orders::OrderType::MARKET => Self::Market,
            updated_account_orders::OrderType::LIMIT => Self::Limit,
            updated_account_orders::OrderType::STOP_MARKET => Self::StopMarket,
            updated_account_orders::OrderType::STOP_LIMIT => Self::StopLimit,
            _ => panic!("Unexpected value in OrderType enum"),
        }
    }
}

impl From<updated_account_orders::OrderStatus> for OrderStatus {
    fn from(response: updated_account_orders::OrderStatus) -> Self {
        match response {
            updated_account_orders::OrderStatus::PENDING => OrderStatus::Pending,
            updated_account_orders::OrderStatus::OPEN => OrderStatus::Open,
            updated_account_orders::OrderStatus::CANCELLED => OrderStatus::Canceled,
            updated_account_orders::OrderStatus::FILLED => OrderStatus::Filled,
            _ => panic!("Unexpected value in OrderStatus enum"),
        }
    }
}

impl From<&updated_account_orders::OrderCancellationReason> for OrderCancellationReason {
    fn from(cancellation_reason: &updated_account_orders::OrderCancellationReason) -> Self {
        match cancellation_reason {
            updated_account_orders::OrderCancellationReason::ADMIN_CANCELED => {
                OrderCancellationReason::AdminCancelled
            }
            updated_account_orders::OrderCancellationReason::EXPIRATION => {
                OrderCancellationReason::Expiration
            }
            updated_account_orders::OrderCancellationReason::USER => OrderCancellationReason::User,
            updated_account_orders::OrderCancellationReason::NO_FILL => {
                OrderCancellationReason::NoFill
            }
            updated_account_orders::OrderCancellationReason::INVALID_FOR_ORDERBOOK_STATE => {
                OrderCancellationReason::InvalidForOrderbookState
            }
            _ => panic!("Unsupported OrderCancellationReason"),
        }
    }
}

impl From<&updated_account_orders::UpdatedAccountOrdersUpdatedAccountOrders>
    for Option<OrderCancellationPolicy>
{
    fn from(order: &updated_account_orders::UpdatedAccountOrdersUpdatedAccountOrders) -> Self {
        if let Some(cancellation_policy) = &order.cancellation_policy {
            Some(match cancellation_policy {
                updated_account_orders::OrderCancellationPolicy::FILL_OR_KILL => {
                    OrderCancellationPolicy::FillOrKill
                }
                updated_account_orders::OrderCancellationPolicy::GOOD_TIL_CANCELLED => {
                    OrderCancellationPolicy::GoodTilCancelled
                }
                updated_account_orders::OrderCancellationPolicy::GOOD_TIL_TIME => {
                    // This unwrap is safe
                    let cancel_at =
                        DateTime::<Utc>::from_str(&order.cancel_at.as_ref().unwrap()).unwrap();
                    OrderCancellationPolicy::GoodTilTime(cancel_at)
                }
                updated_account_orders::OrderCancellationPolicy::IMMEDIATE_OR_CANCEL => {
                    OrderCancellationPolicy::ImmediateOrCancel
                }
                _ => panic!("Unsupported OrderCancellationPolicy"),
            })
        } else {
            None
        }
    }
}
