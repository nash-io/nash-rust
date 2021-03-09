use super::types::PlaceOrdersResponse;
use crate::graphql::place_limit_order;
use crate::graphql::place_market_order;
use crate::types::OrderType;
use chrono::{DateTime, Utc};
use std::str::FromStr;

impl From<place_limit_order::ResponseData> for PlaceOrdersResponse {
    fn from(response: place_limit_order::ResponseData) -> Self {
        let response = response.place_limit_order;
        Self {
            status: response.status.into(),
            order_id: response.id,
            remaining_orders: response.orders_till_sign_state as u64,
            placed_at: DateTime::<Utc>::from_str(&response.placed_at)
                .expect("ME returned invalid placed_at DateTime for PlaceOrderResponse"),
            order_type: response.type_.into(),
            buy_or_sell: response.buy_or_sell.into(),
            market_name: response.market.name.clone(),
        }
    }
}

impl From<place_market_order::ResponseData> for PlaceOrdersResponse {
    fn from(response: place_market_order::ResponseData) -> Self {
        let response = response.place_market_order;
        Self {
            status: response.status.into(),
            order_id: response.id,
            remaining_orders: response.orders_till_sign_state as u64,
            placed_at: DateTime::<Utc>::from_str(&response.placed_at)
                .expect("ME returned invalid placed_at DateTime for PlaceOrderResponse"),
            order_type: OrderType::Market,
            buy_or_sell: response.buy_or_sell.into(),
            market_name: response.market.name.clone(),
        }
    }
}

