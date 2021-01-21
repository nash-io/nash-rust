use super::super::{
    serializable_to_json, try_response_from_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::cancel_all_orders;

use async_trait::async_trait;
use tokio::sync::RwLock;
use std::sync::Arc;

/// Request to cancel all orders in a given market. To prove orders have been canceled,
/// the client must sign and sync nonces on the assets in question.
#[derive(Clone, Debug)]
pub struct CancelAllOrders {
    pub market: String,
}

/// Response indicates whether the ME accepted the request to cancel all orders
#[derive(Clone, Copy, Debug)]
pub struct CancelAllOrdersResponse {
    pub accepted: bool,
}

/// Implement protocol bindings for CancelOrder
#[async_trait]
impl NashProtocol for CancelAllOrders {
    type Response = CancelAllOrdersResponse;

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
        try_response_from_json::<CancelAllOrdersResponse, cancel_all_orders::ResponseData>(response)
    }
}
