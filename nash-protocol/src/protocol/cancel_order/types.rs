use super::super::{
    serializable_to_json, try_response_from_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::cancel_order;

use async_trait::async_trait;
use tokio::sync::RwLock;
use std::sync::Arc;

/// Request to cancel a single order in a given market. Note: to prove orders have been canceled,
/// the client must sign and sync nonces on the assets in question.
#[derive(Clone, Debug)]
pub struct CancelOrderRequest {
    pub order_id: String, // TODO: ME bug, this should not be required field
    pub market: String,
}

/// Response contains the order id of the canceled order if successful
#[derive(Clone, Debug)]
pub struct CancelOrderResponse {
    pub order_id: String,
}

/// Implement protocol bindings for CancelOrder
#[async_trait]
impl NashProtocol for CancelOrderRequest {
    type Response = CancelOrderResponse;

    async fn graphql(&self, state: Arc<RwLock<State>>) -> Result<serde_json::Value> {
        let state = state.read().await;
        let signer = state.signer()?;
        let query = self.make_query(signer);
        serializable_to_json(&query)
    }

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        _state: Arc<RwLock<State>>
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<CancelOrderResponse, cancel_order::ResponseData>(response)
    }
}
