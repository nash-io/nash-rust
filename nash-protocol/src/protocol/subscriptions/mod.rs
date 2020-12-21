pub mod trades;
pub mod updated_orderbook;
pub mod updated_ticker;
use super::graphql::ResponseOrError;
use super::{NashProtocolSubscription, State};
use crate::errors::Result;
use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

/// Wrapper for all subscription request types supported on Nash. Required only for current version
/// of openlimits subscription logic
#[derive(Clone, Debug)]
pub enum SubscriptionRequest {
    Trades(trades::SubscribeTrades),
    Ticker(updated_ticker::SubscribeTicker),
    Orderbook(updated_orderbook::SubscribeOrderbook),
}

/// Wrapper for incoming subscription data supported on Nash. Required only for current version
/// of openlimits subscription logic
#[derive(Debug)]
pub enum SubscriptionResponse {
    Orderbook(updated_orderbook::SubscribeOrderbookResponse),
    Ticker(Box<updated_ticker::SubscribeTickerResponse>),
    Trades(trades::TradesResponse),
}

#[async_trait]
impl NashProtocolSubscription for SubscriptionRequest {
    type SubscriptionResponse = SubscriptionResponse;
    async fn graphql(&self, state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        match self {
            Self::Trades(trades_req) => trades_req.graphql(state).await,
            Self::Ticker(ticker_req) => ticker_req.graphql(state).await,
            Self::Orderbook(orderbook_req) => orderbook_req.graphql(state).await,
        }
    }
    async fn subscription_response_from_json(
        &self,
        response: serde_json::Value,
        state: Arc<Mutex<State>>,
    ) -> Result<ResponseOrError<Self::SubscriptionResponse>> {
        match self {
            Self::Trades(trades_req) => Ok(trades_req
                .subscription_response_from_json(response, state)
                .await?
                .map(Box::new(SubscriptionResponse::Trades))),
            Self::Ticker(ticker_req) => Ok(ticker_req
                .subscription_response_from_json(response, state)
                .await?
                .map(Box::new(|res| SubscriptionResponse::Ticker(Box::new(res))))),
            Self::Orderbook(orderbook_req) => Ok(orderbook_req
                .subscription_response_from_json(response, state)
                .await?
                .map(Box::new(SubscriptionResponse::Orderbook))),
        }
    }
    async fn wrap_response_as_any_subscription(
        &self,
        response: serde_json::Value,
        state: Arc<Mutex<State>>,
    ) -> Result<ResponseOrError<SubscriptionResponse>> {
        let response = self
            .subscription_response_from_json(response, state)
            .await?;
        Ok(response)
    }
}
