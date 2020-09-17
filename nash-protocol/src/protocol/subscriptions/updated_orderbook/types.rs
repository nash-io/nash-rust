use super::super::super::{
    json_to_type_or_error, serializable_to_json, NashProtocolSubscription, ResponseOrError, State,
};
use crate::errors::Result;
use crate::types::{Market, OrderbookOrder};
use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct SubscribeOrderbook {
    pub market: Market,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SubscribeOrderbookResponse {
    pub bids: Vec<OrderbookOrder>,
    pub asks: Vec<OrderbookOrder>
}

#[async_trait]
impl NashProtocolSubscription for SubscribeOrderbook {
    type SubscriptionResponse = SubscribeOrderbookResponse;
    async fn graphql(&self, _state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        serializable_to_json(&query)
    }
    fn subscription_response_from_json(
        &self,
        response: serde_json::Value,
    ) -> Result<ResponseOrError<Self::SubscriptionResponse>> {
        let as_graphql = json_to_type_or_error(response)?;
        self.response_from_graphql(as_graphql)
    }
}
