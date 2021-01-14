pub mod trades;
pub mod new_account_trades;
pub mod updated_account_orders;
pub mod updated_account_balances;
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
    AccountTrades(new_account_trades::SubscribeAccountTrades),
    AccountOrders(updated_account_orders::SubscribeAccountOrders),
    AccountBalances(updated_account_balances::SubscribeAccountBalances),
}

/// Wrapper for incoming subscription data supported on Nash. Required only for current version
/// of openlimits subscription logic
#[derive(Debug)]
pub enum SubscriptionResponse {
    Orderbook(updated_orderbook::SubscribeOrderbookResponse),
    Ticker(updated_ticker::SubscribeTickerResponse),
    Trades(trades::TradesResponse),
    AccountTrades(new_account_trades::AccountTradesResponse),
    AccountOrders(updated_account_orders::AccountOrdersResponse),
    AccountBalances(updated_account_balances::AccountBalancesResponse),
}

#[async_trait]
impl NashProtocolSubscription for SubscriptionRequest {
    type SubscriptionResponse = SubscriptionResponse;
    async fn graphql(&self, state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        match self {
            Self::Trades(trades_req) => trades_req.graphql(state).await,
            Self::Ticker(ticker_req) => ticker_req.graphql(state).await,
            Self::Orderbook(orderbook_req) => orderbook_req.graphql(state).await,
            Self::AccountTrades(account_trades_req) => account_trades_req.graphql(state).await,
            Self::AccountOrders(account_orders_req) => account_orders_req.graphql(state).await,
            Self::AccountBalances(account_balance_req) => account_balance_req.graphql(state).await
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
                .map(Box::new(|res| SubscriptionResponse::Trades(res)))),
            Self::AccountTrades(account_trades_req) => Ok(account_trades_req
                .subscription_response_from_json(response, state)
                .await?
                .map(Box::new(|res| SubscriptionResponse::AccountTrades(res)))),
            Self::AccountOrders(account_orders_req) => Ok(account_orders_req
                .subscription_response_from_json(response, state)
                .await?
                .map(Box::new(|res| SubscriptionResponse::AccountOrders(res)))),
            Self::AccountBalances(account_balances_req) => Ok(account_balances_req
                .subscription_response_from_json(response, state)
                .await?
                .map(Box::new(|res| SubscriptionResponse::AccountBalances(res)))),
            Self::Ticker(ticker_req) => Ok(ticker_req
                .subscription_response_from_json(response, state)
                .await?
                .map(Box::new(|res| SubscriptionResponse::Ticker(res)))),
            Self::Orderbook(orderbook_req) => Ok(orderbook_req
                .subscription_response_from_json(response, state)
                .await?
                .map(Box::new(|res| SubscriptionResponse::Orderbook(res)))),
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
