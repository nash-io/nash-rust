use crate::graphql::place_limit_order;
use crate::graphql::place_market_order;
use serde::{Deserialize, Deserializer};
use crate::protocol::multi_request::{ResponseData, ResponseVisitor};

pub type LimitResponseData = ResponseData<place_limit_order::ResponseData>;
pub type MarketResponseData = ResponseData<place_market_order::ResponseData>;

impl<'de> Deserialize<'de> for LimitResponseData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        deserializer.deserialize_map(
            ResponseVisitor::<Self, place_limit_order::ResponseData>::new("placeLimitOrder".to_string())
        )
    }
}

impl<'de> Deserialize<'de> for MarketResponseData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de> {
        deserializer.deserialize_map(
            ResponseVisitor::<Self, place_market_order::ResponseData>::new("placeMarketOrder".to_string())
        )
    }
}
