use crate::errors::Result;
use crate::types::OrderbookOrder;
use super::super::list_markets::ListMarketsRequest;
use super::super::hooks::{ProtocolHook, NashProtocolRequest};
use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

use super::super::{
    json_to_type_or_error, serializable_to_json, NashProtocol, ResponseOrError, State,
};

/// Get order book data for the provided market
#[derive(Clone, Debug)]
pub struct OrderbookRequest {
    pub market: String,
}

/// An order book is a list of bid and ask orders
#[derive(Debug)]
pub struct OrderbookResponse {
    pub last_update_id: i64,
    pub update_id: i64,
    pub asks: Vec<OrderbookOrder>,
    pub bids: Vec<OrderbookOrder>,
}

#[async_trait]
impl NashProtocol for OrderbookRequest {
    type Response = OrderbookResponse;

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

    async fn run_before(&self, state: Arc<Mutex<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        let state = state.lock().await;
        let mut hooks = Vec::new();
        if state.markets.is_none() {
            hooks.push(ProtocolHook::Protocol(NashProtocolRequest::ListMarkets(
                ListMarketsRequest,
            )))
        }
        Ok(Some(hooks))
    }
}
