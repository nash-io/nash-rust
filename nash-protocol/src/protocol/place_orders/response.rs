use super::types::PlaceOrdersResponse;
use crate::graphql::place_limit_order;
use crate::graphql::place_market_order;
use serde::{Deserialize, Deserializer};
use std::fmt;
use serde::de::{self, MapAccess, Visitor};
use serde_json::{Value, Map};
use std::marker::PhantomData;

pub struct ResponseData<T> {
    pub responses: Vec<T>
}

impl<T> From<Vec<T>> for ResponseData<T> {
    fn from(responses: Vec<T>) -> Self {
        Self { responses }
    }
}

pub type LimitResponseData = ResponseData<place_limit_order::ResponseData>;
pub type MarketResponseData = ResponseData<place_market_order::ResponseData>;

struct ResponseVisitor<T, U> {
    pub field: String,
    response_data: PhantomData<T>,
    graphql_response_data: PhantomData<U>
}
impl<T, U> ResponseVisitor<T, U> {
    pub fn new(field: String) -> Self {
        Self {
            field,
            response_data: PhantomData,
            graphql_response_data: PhantomData
        }
    }
}

impl<'de, U: for <'a> Deserialize<'a>, T: From<Vec<U>>> Visitor<'de> for ResponseVisitor<T, U> {
    type Value = T;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Expected multiple OrderResponses")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where A: MapAccess<'de> {
        let mut responses = Vec::new();
        while let Some((_, value)) = map.next_entry::<String, Value>()? {
            let mut object = Map::new();
            object.insert(self.field.clone(), value);
            let response = serde_json::from_value::<U>(Value::Object(object)).map_err(|e| {
                de::Error::custom(format!("Couldn't parse: {:#?}", e))
            })?;
            responses.push(response);
        }
        Ok(T::from(responses))
    }
}

impl<'de> Deserialize<'de> for LimitResponseData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        deserializer.deserialize_map(
            ResponseVisitor::<LimitResponseData, place_limit_order::ResponseData>::new("placeLimitOrder".to_string())
        )
    }
}

impl<'de> Deserialize<'de> for MarketResponseData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de> {
        deserializer.deserialize_map(
            ResponseVisitor::<MarketResponseData, place_market_order::ResponseData>::new("placeMarketOrder".to_string())
        )
    }
}

impl From<LimitResponseData> for PlaceOrdersResponse {
    fn from(response: LimitResponseData) -> Self {
        let responses = response.responses.into_iter().map(|x| x.into()).collect();
        Self { responses }
    }
}

impl From<MarketResponseData> for PlaceOrdersResponse {
    fn from(response: MarketResponseData) -> Self {
        let responses = response.responses.into_iter().map(|x| x.into()).collect();
        Self { responses }
    }
}

