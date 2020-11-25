use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

use crate::errors::Result;
use crate::types::AssetAmount;

use super::super::{
    json_to_type_or_error, serializable_to_json, NashProtocol, ResponseOrError, State,
};

/// Get ticker associated with market
#[derive(Clone, Debug)]
pub struct TickerRequest {
    pub market: String,
}

/// Ticker response information
#[derive(Clone, Debug)]
pub struct TickerResponse {
    pub id: String,
    pub a_volume_24h: AssetAmount,
    pub b_volume_24h: AssetAmount,
    pub best_ask_price: Option<AssetAmount>,
    pub best_bid_price: Option<AssetAmount>,
    pub best_ask_amount: Option<AssetAmount>,
    pub best_bid_amount: Option<AssetAmount>,
    pub high_price_24h: Option<AssetAmount>,
    pub last_price: Option<AssetAmount>,
    pub low_price_24h: Option<AssetAmount>,
    pub price_change_24h: Option<AssetAmount>,
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

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        state: Arc<Mutex<State>>
    ) -> Result<ResponseOrError<Self::Response>> {
        let as_graphql = json_to_type_or_error(response)?;
        self.response_from_graphql(as_graphql, state).await
    }
}
