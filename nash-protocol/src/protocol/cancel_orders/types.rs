use super::super::{
    serializable_to_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;

use async_trait::async_trait;
use tokio::sync::RwLock;
use std::sync::Arc;
use std::convert::TryInto;
use crate::protocol::multi_request::{MultiRequest, MultiResponse};
use crate::protocol::cancel_order::{CancelOrderRequest, CancelOrderResponse};

pub type CancelOrdersRequest = MultiRequest<CancelOrderRequest>;
pub type CancelOrdersResponse = MultiResponse<CancelOrderResponse>;

/// Implement protocol bindings for CancelOrders
#[async_trait]
impl NashProtocol for CancelOrdersRequest {
    type Response = CancelOrdersResponse;

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
        Ok(ResponseOrError::from_data(response.try_into()?))
    }
}
