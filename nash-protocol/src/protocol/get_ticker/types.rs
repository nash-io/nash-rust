use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

use super::super::{
    json_to_type_or_error, serializable_to_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use bigdecimal::BigDecimal;

/// Get ticker associated with market
#[derive(Clone, Debug)]
pub struct TickerRequest {
    pub market: String,
}

/// Ticker response information
#[derive(Clone, Debug)]
pub struct TickerResponse {
    pub id: String,
    pub a_volume_24h: BigDecimal,
    pub b_volume_24h: BigDecimal,
    pub best_ask_price: Option<BigDecimal>,
    pub best_bid_price: Option<BigDecimal>,
    pub best_ask_amount: Option<BigDecimal>,
    pub best_bid_amount: Option<BigDecimal>,
    pub high_price_24h: Option<BigDecimal>,
    pub last_price: Option<BigDecimal>,
    pub low_price_24h: Option<BigDecimal>,
    pub price_change_24h: Option<BigDecimal>,
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
        state: Arc<Mutex<State>>,
    ) -> Result<ResponseOrError<Self::Response>> {
        let as_graphql = json_to_type_or_error(response)?;
        self.response_from_graphql(as_graphql, state).await
    }
}
