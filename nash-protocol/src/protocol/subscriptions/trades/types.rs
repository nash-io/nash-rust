use super::super::super::{
    json_to_type_or_error, serializable_to_json, NashProtocolSubscription, ResponseOrError, State,
};
use super::super::SubscriptionResponse;
use super::request::SubscribeTrades;
use super::response::TradesResponse;
use crate::errors::Result;
use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

#[async_trait]
impl NashProtocolSubscription for SubscribeTrades {
    type SubscriptionResponse = TradesResponse;
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

    fn wrap_response_as_any_subscription(
        &self,
        response: serde_json::Value,
    ) -> Result<SubscriptionResponse> {
        let response = self.subscription_response_from_json(response)?;
        Ok(SubscriptionResponse::NewTrade(response))
    }
}
