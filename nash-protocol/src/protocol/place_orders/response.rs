use super::types::PlaceOrdersResponse;
use crate::graphql::place_limit_order;
use crate::graphql::place_market_order;
use crate::types::{OrderType, OrderStatus, BuyOrSell};
use chrono::{DateTime, Utc};
use std::str::FromStr;
use std::collections::HashMap;
use crate::protocol::place_order::PlaceOrderResponse;
use serde::{Deserialize, Deserializer};
use std::fmt;
use serde::de::{self, MapAccess, Visitor};
use serde_json::{Value, Map};

#[derive(Debug)]
pub struct ResponseData {
    pub responses: Vec<place_limit_order::ResponseData>
}

struct LimitResponseVisitor;

impl<'de> Visitor<'de> for LimitResponseVisitor {
    type Value = ResponseData;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an integer between -2^31 and 2^31")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where A: MapAccess<'de> {
        let mut responses = Vec::new();
        while let Some((_, value)) = map.next_entry::<String, Value>()? {
            let mut object = Map::new();
            object.insert("placeLimitOrder".to_string(), value);
            let response = serde_json::from_value::<place_limit_order::ResponseData>(Value::Object(object)).map_err(|e| {
                de::Error::custom(format!("Couldn't parse: {:#?}", e))
            })?;
            responses.push(response);
        }
        Ok(ResponseData { responses })
    }
}

impl<'de> Deserialize<'de> for ResponseData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        deserializer.deserialize_map(LimitResponseVisitor)
    }
}

impl From<ResponseData> for PlaceOrdersResponse {
    fn from(response: ResponseData) -> Self {
        let mut responses = Vec::new();
        for response in response.responses.into_iter() {
            let response = response.place_limit_order;
            responses.push(PlaceOrderResponse {
                status: response.status.into(),
                order_id: response.id,
                remaining_orders: response.orders_till_sign_state as u64,
                placed_at: DateTime::<Utc>::from_str(&response.placed_at)
                    .expect("ME returned invalid placed_at DateTime for PlaceOrderResponse"),
                order_type: response.type_.into(),
                buy_or_sell: response.buy_or_sell.into(),
                market_name: response.market.name.clone(),
            });
        }
        Self { responses }
    }
}

impl From<place_market_order::ResponseData> for PlaceOrdersResponse {
    fn from(response: place_market_order::ResponseData) -> Self {
        todo!()
        // let response = response.place_market_order;
        // Self {
        //     status: response.status.into(),
        //     order_id: response.id,
        //     remaining_orders: response.orders_till_sign_state as u64,
        //     placed_at: DateTime::<Utc>::from_str(&response.placed_at)
        //         .expect("ME returned invalid placed_at DateTime for PlaceOrderResponse"),
        //     order_type: OrderType::Market,
        //     buy_or_sell: response.buy_or_sell.into(),
        //     market_name: response.market.name.clone(),
        // }
    }
}

