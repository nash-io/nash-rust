use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

use crate::errors::Result;
use crate::types::{AssetAmount, Market};

use super::super::{
    json_to_type_or_error, serializable_to_json, NashProtocol, ResponseOrError, State,
};

#[derive(Clone, Debug)]
pub struct TickerRequest {
    pub market: Market,
}

#[derive(Clone, Debug)]
pub struct TickerResponse {
    pub id: String,
    pub a_volume_24h: AssetAmount,
    pub b_volume_24h: AssetAmount,
    pub best_ask_price: AssetAmount,
    pub best_bid_price: AssetAmount,
    pub best_ask_amount: AssetAmount,
    pub best_bid_amount: AssetAmount,
    pub high_price_24h: AssetAmount,
    pub last_price: AssetAmount,
    pub low_price_24h: AssetAmount,
    pub price_change_24h: AssetAmount,
    pub market_name: String,
}

/// Implement protocol bindings for TickerRequest
#[async_trait]
impl NashProtocol for TickerRequest {
    type Response = TickerResponse;

    async fn graphql(&self, _state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        serializable_to_json(&query)
    }

    fn response_from_json(
        &self,
        response: serde_json::Value,
    ) -> Result<ResponseOrError<Self::Response>> {
        let as_graphql = json_to_type_or_error(response)?;
        self.response_from_graphql(as_graphql)
    }
}
