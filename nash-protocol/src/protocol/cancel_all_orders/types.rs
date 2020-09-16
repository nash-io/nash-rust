use super::super::{
    try_response_from_json, serializable_to_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::cancel_all_orders;
use crate::types::Market;

use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

/// Request to cancel all orders in a given market. To prove orders have been canceled,
/// the client must sign and sync nonces on the assets in question.
#[derive(Clone, Copy, Debug)]
pub struct CancelAllOrders {
    pub market: Market,
}

#[derive(Clone, Copy, Debug)]
pub struct CancelAllOrdersResponse {
    pub accepted: bool,
}

/// Implement protocol bindings for CancelOrder
#[async_trait]
impl NashProtocol for CancelAllOrders {
    type Response = CancelAllOrdersResponse;

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
        try_response_from_json::<CancelAllOrdersResponse, cancel_all_orders::ResponseData>(response)
    }
}
