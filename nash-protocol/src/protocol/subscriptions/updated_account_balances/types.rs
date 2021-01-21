use super::super::super::{
    json_to_type_or_error, serializable_to_json, NashProtocolSubscription, ResponseOrError, State,
};
use super::super::SubscriptionResponse;
use super::request::SubscribeAccountBalances;
use super::response::AccountBalancesResponse;
use crate::errors::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait]
impl NashProtocolSubscription for SubscribeAccountBalances {
    type SubscriptionResponse = AccountBalancesResponse;
    async fn graphql(&self, _state: Arc<RwLock<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        serializable_to_json(&query)
    }
    async fn subscription_response_from_json(
        &self,
        response: serde_json::Value,
        state: Arc<RwLock<State>>
    ) -> Result<ResponseOrError<Self::SubscriptionResponse>> {
        let as_graphql = json_to_type_or_error(response)?;
        self.response_from_graphql(as_graphql, state).await
    }

    async fn wrap_response_as_any_subscription(
        &self,
        response: serde_json::Value,
        state: Arc<RwLock<State>>
    ) -> Result<ResponseOrError<SubscriptionResponse>> {
        let response = self.subscription_response_from_json(response, state).await?;
        let wrapped_response = response.map(Box::new(|res| 
            SubscriptionResponse::AccountBalances(res)
        ));
        Ok(wrapped_response)
    }
}
