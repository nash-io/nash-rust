use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{Mutex, RwLock};
use tracing::error;

use crate::errors::Result;
use crate::protocol::ErrorResponse;
use crate::protocol::{
    asset_nonces::AssetNoncesRequest, list_markets::ListMarketsRequest, serializable_to_json,
    sign_all_states::SignAllStates, NashProtocol, NashProtocolRequest,
    ProtocolHook, ResponseOrError, State,
};
use crate::utils::current_time_as_i64;
use crate::protocol::place_order::{LimitOrderRequest, PlaceOrderResponse, MarketOrderRequest};

/// Request to place limit orders on Nash exchange. On an A/B market
/// price amount will always be in terms of A and price in terms of B.
pub type LimitOrdersRequest = MultiRequest<LimitOrderRequest>;
pub type PlaceOrdersResponse = MultiResponse<PlaceOrderResponse>;

/// Request to place market orders on Nash exchange. On an A/B market
/// price amount will always be in terms of A and price in terms of B.
pub type MarketOrdersRequest = MultiRequest<MarketOrderRequest>;

use crate::protocol::place_order::types::{LimitOrderConstructor, MarketOrderConstructor};
use crate::protocol::multi_request::{MultiRequest, MultiRequestConstructor, MultiResponse};
use std::convert::TryInto;

pub type LimitOrdersConstructor = MultiRequestConstructor<LimitOrderConstructor>;
pub type MarketOrdersConstructor = MultiRequestConstructor<MarketOrderConstructor>;

async fn get_required_hooks(state: Arc<RwLock<State>>, market: &str) -> Result<Vec<ProtocolHook>> {
    let state = state.read().await;

    let mut hooks = Vec::new();
    // If we need assets or markets list, pull them
    match (&state.assets, &state.markets) {
        (None, _) | (_, None) => {
            hooks.push(ProtocolHook::Protocol(NashProtocolRequest::ListMarkets(
                ListMarketsRequest,
            )));
        }
        _ => {}
    }
    // If have run out of r values, get more before running this pipeline
    let chains = state.get_market(market)?.blockchains();
    let fill_pool_schedules = state
        .acquire_fill_pool_schedules(Some(&chains), Some(10))
        .await?;
    for (request, permit) in fill_pool_schedules {
        // A bit too complicated but we have to satisfy the compiler
        let permit = Some(Arc::new(Mutex::new(Some(permit))));
        hooks.push(ProtocolHook::Protocol(NashProtocolRequest::DhFill(
            request, permit,
        )));
    }
    // Retrieve asset nonces if we don't have them or an error triggered need to refresh
    match (state.asset_nonces.as_ref(), state.assets_nonces_refresh) {
        (None, _) | (_, true) => {
            hooks.push(ProtocolHook::Protocol(NashProtocolRequest::AssetNonces(
                AssetNoncesRequest::new(),
            )));
        }
        _ => {}
    }
    // If we are about to run out of orders...
    if !state.dont_sign_states && state.get_remaining_orders() < 10 {
        // Need to sign states
        hooks.push(ProtocolHook::SignAllState(SignAllStates::new()));
        // After signing states, need to update nonces again
        hooks.push(ProtocolHook::Protocol(NashProtocolRequest::AssetNonces(
            AssetNoncesRequest::new(),
        )));
    }
    Ok(hooks)
}

#[async_trait]
impl NashProtocol for LimitOrdersRequest {
    type Response = PlaceOrdersResponse;

    async fn acquire_permit(
        &self,
        state: Arc<RwLock<State>>,
    ) -> Option<tokio::sync::OwnedSemaphorePermit> {
        state
            .read()
            .await
            .place_order_semaphore
            .clone()
            .acquire_owned()
            .await
            .ok()
    }

    async fn graphql(&self, state: Arc<RwLock<State>>) -> Result<serde_json::Value> {
        let builder = self.make_constructor(state.clone()).await?;
        let time = current_time_as_i64();
        let affiliate = state.read().await.affiliate_code.clone();
        let query = builder.signed_graphql_request(time, affiliate, state).await?;
        serializable_to_json(&query)
    }

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        _state: Arc<RwLock<State>>,
    ) -> Result<ResponseOrError<Self::Response>> {
        Ok(ResponseOrError::from_data(response.try_into()?))
    }

    /// Update the number of orders remaining before state sync
    async fn process_response(
        &self,
        response: &Self::Response,
        state: Arc<RwLock<State>>,
    ) -> Result<()> {
        if let Some(Ok(response)) = response.responses.iter().rfind(|response| response.is_ok()) {
            let state = state.read().await;
            state.set_remaining_orders(response.remaining_orders);
        }
        Ok(())
    }

    async fn process_error(
        &self,
        response: &ErrorResponse,
        state: Arc<RwLock<State>>,
    ) -> Result<()> {
        // TODO: Do we need to decrement for errors?
        state.read().await.decr_remaining_orders();
        for err in &response.errors {
            if err.message.find("invalid blockchain signature").is_some() {
                error!(err = %err.message, request = ?self, "invalid blockchain signature");
            }
        }
        Ok(())
    }

    /// Potentially get more r values or sign states before placing an order
    async fn run_before(&self, state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        let request = &self.requests[0];
        get_required_hooks(state, &request.market).await.map(Some)
    }
}

#[async_trait]
impl NashProtocol for MarketOrdersRequest {
    type Response = PlaceOrdersResponse;

    async fn acquire_permit(
        &self,
        state: Arc<RwLock<State>>,
    ) -> Option<tokio::sync::OwnedSemaphorePermit> {
        state
            .read()
            .await
            .place_order_semaphore
            .clone()
            .acquire_owned()
            .await
            .ok()
    }

    async fn graphql(&self, state: Arc<RwLock<State>>) -> Result<serde_json::Value> {
        let builder = self.make_constructor(state.clone()).await?;
        let time = current_time_as_i64();
        let affiliate = state.read().await.affiliate_code.clone();
        let query = builder.signed_graphql_request(time, affiliate, state).await?;
        serializable_to_json(&query)
    }

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        _state: Arc<RwLock<State>>,
    ) -> Result<ResponseOrError<Self::Response>> {
        Ok(ResponseOrError::from_data(response.try_into()?))
    }

    /// Update the number of orders remaining before state sync
    async fn process_response(
        &self,
        response: &Self::Response,
        state: Arc<RwLock<State>>,
    ) -> Result<()> {
        if let Some(Ok(response)) = response.responses.iter().rfind(|response| response.is_ok()) {
            let state = state.read().await;
            // TODO: Incorporate error into process response
            state.set_remaining_orders(response.remaining_orders);
        }
        Ok(())
    }

    async fn process_error(
        &self,
        _response: &ErrorResponse,
        state: Arc<RwLock<State>>,
    ) -> Result<()> {
        // TODO: Do we need to decrement for errors?
        state.read().await.decr_remaining_orders();
        Ok(())
    }

    /// Potentially get more r values or sign states before placing an order
    async fn run_before(&self, state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        let request = &self.requests[0];
        get_required_hooks(state, &request.market).await.map(Some)
    }
}
