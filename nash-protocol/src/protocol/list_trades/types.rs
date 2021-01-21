use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;
use crate::errors::Result;
use crate::graphql::list_trades;
use crate::types::Trade;
use super::super::list_markets::ListMarketsRequest;
use super::super::hooks::{ProtocolHook, NashProtocolRequest};
use super::super::{
    NashProtocol, ResponseOrError, serializable_to_json, State, try_response_with_state_from_json,
};

/// Get trades associated with market, filtering on several optional fields.
#[derive(Clone, Debug)]
pub struct ListTradesRequest {
    pub market: String,
    /// max trades to return
    pub limit: Option<i64>,
    /// page before if using pagination
    pub before: Option<String>,
}

/// List of trades as well as an optional link to the next page of data.
#[derive(Debug)]
pub struct ListTradesResponse {
    pub trades: Vec<Trade>,
    pub next_page: Option<String>,
}

#[async_trait]
impl NashProtocol for ListTradesRequest {
    type Response = ListTradesResponse;

    async fn graphql(&self, _state: Arc<RwLock<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        serializable_to_json(&query)
    }

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        state: Arc<RwLock<State>>
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_with_state_from_json::<ListTradesResponse, list_trades::ResponseData>(response, state).await
    }

    async fn run_before(&self, state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        let state = state.read().await;
        let mut hooks = Vec::new();
        if let None = state.markets {
            hooks.push(ProtocolHook::Protocol(NashProtocolRequest::ListMarkets(
                ListMarketsRequest,
            )))
        }
        Ok(Some(hooks))
    }
}
