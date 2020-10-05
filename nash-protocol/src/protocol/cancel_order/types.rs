use super::super::{
    serializable_to_json, try_response_from_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::cancel_order;
use crate::types::Market;

use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

/// Request to cancel all orders in a given market. To prove orders have been canceled,
/// the client must sign and sync nonces on the assets in question.
#[derive(Clone, Debug)]
pub struct CancelOrderRequest {
    pub order_id: String,
    pub market: Market,
}

#[derive(Clone, Debug)]
pub struct CancelOrderResponse {
    pub order_id: String,
}

/// Implement protocol bindings for CancelOrder
#[async_trait]
impl NashProtocol for CancelOrderRequest {
    type Response = CancelOrderResponse;

    async fn graphql(&self, state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let mut state = state.lock().await;
        let signer = state.signer()?;
        let query = self.make_query(signer);
        serializable_to_json(&query)
    }

    fn response_from_json(
        &self,
        response: serde_json::Value,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<CancelOrderResponse, cancel_order::ResponseData>(response)
    }
}
