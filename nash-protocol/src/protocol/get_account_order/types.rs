use super::super::{
    serializable_to_json, try_response_from_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::get_account_order;
use crate::types::Order;

use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct GetAccountOrderRequest {
    pub order_id: String,
}

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

    fn response_from_json(
        &self,
        response: serde_json::Value,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<GetAccountOrderResponse, get_account_order::ResponseData>(response)
    }
}
