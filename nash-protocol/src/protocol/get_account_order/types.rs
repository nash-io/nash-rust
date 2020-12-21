use super::super::{
    serializable_to_json, try_response_with_state_from_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::get_account_order;
use crate::types::Order;
use super::super::list_markets::ListMarketsRequest;
use super::super::hooks::{ProtocolHook, NashProtocolRequest};
use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

/// Lookup order information via id
#[derive(Clone, Debug)]
pub struct GetAccountOrderRequest {
    pub order_id: String,
}

/// Response contains an `Order` with associated information
#[derive(Clone, Debug)]
pub struct GetAccountOrderResponse {
    pub order: Order,
}

/// Implement protocol bindings for GetAccountOrderRequest
#[async_trait]
impl NashProtocol for GetAccountOrderRequest {
    type Response = GetAccountOrderResponse;

    async fn graphql(&self, _state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        serializable_to_json(&query)
    }

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        state: Arc<Mutex<State>>
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_with_state_from_json::<GetAccountOrderResponse, get_account_order::ResponseData>(response, state).await
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
