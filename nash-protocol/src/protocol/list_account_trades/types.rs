use super::super::hooks::{NashProtocolRequest, ProtocolHook};
use super::super::list_markets::ListMarketsRequest;
use super::super::{
    serializable_to_json, try_response_with_state_from_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::list_account_trades;
use crate::types::{DateTimeRange, Trade};
use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

/// List trades associated with current account session filtered by several optional fields.
#[derive(Clone, Debug)]
pub struct ListAccountTradesRequest {
    pub market: Option<String>,
    /// page before if using pagination
    pub before: Option<String>,
    /// max trades to return
    pub limit: Option<i64>,
    pub range: Option<DateTimeRange>,
}

/// List of trades and optional link to the next page of data
#[derive(Debug)]
pub struct ListAccountTradesResponse {
    pub trades: Vec<Trade>,
    pub next_page: Option<String>,
}

#[async_trait]
impl NashProtocol for ListAccountTradesRequest {
    type Response = ListAccountTradesResponse;

    async fn graphql(&self, _state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        serializable_to_json(&query)
    }

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        state: Arc<Mutex<State>>,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_with_state_from_json::<
            ListAccountTradesResponse,
            list_account_trades::ResponseData,
        >(response, state)
        .await
    }

    async fn run_before(&self, state: Arc<Mutex<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        let state = state.lock().await;
        let mut hooks = Vec::new();
        if let None = state.markets {
            hooks.push(ProtocolHook::Protocol(NashProtocolRequest::ListMarkets(
                ListMarketsRequest,
            )))
        }
        Ok(Some(hooks))
    }
}
