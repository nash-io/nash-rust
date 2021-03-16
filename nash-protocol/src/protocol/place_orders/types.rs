use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{Mutex, RwLock};
use tracing::error;

use crate::errors::{Result, ProtocolError};
use crate::protocol::ErrorResponse;
use crate::protocol::{
    asset_nonces::AssetNoncesRequest, list_markets::ListMarketsRequest, serializable_to_json,
    sign_all_states::SignAllStates, NashProtocol, NashProtocolRequest,
    ProtocolHook, ResponseOrError, State,
};
use crate::types::{AssetAmount, AssetofPrecision, Market};
use crate::utils::current_time_as_i64;
use crate::protocol::place_order::{LimitOrderRequest, PlaceOrderResponse};

/// Request to place limit orders on Nash exchange. On an A/B market
/// price amount will always be in terms of A and price in terms of B.
#[derive(Clone, Debug)]
pub struct LimitOrdersRequest {
    pub requests: Vec<LimitOrderRequest>
}

#[derive(Clone, Debug)]
pub struct MarketOrdersRequest {
    pub client_order_id: Option<String>,
    pub market: String,
    pub amount: String,
}

impl LimitOrdersRequest {
    pub fn new(requests: Vec<LimitOrderRequest>) -> Result<Self> {
        Ok(Self { requests })
    }
}

impl MarketOrdersRequest {
    pub fn new(market: String, amount_a: &str, client_order_id: Option<String>) -> Result<Self> {
        Ok(Self {
            market,
            amount: amount_a.to_string(),
            client_order_id,
        })
    }
}

use crate::protocol::place_order::types::LimitOrderConstructor;
use crate::protocol::place_orders::response::{LimitResponseData, MarketResponseData};

/// A helper type for constructing blockchain payloads and GraphQL requests
pub struct LimitOrdersConstructor {
    pub constructors: Vec<LimitOrderConstructor>
}

pub struct MarketOrderConstructor {
    // These fields are for GraphQL
    pub market: Market,
    pub client_order_id: Option<String>,
    pub me_amount: AssetAmount,
    // These fields are for the smart contracts
    pub source: AssetAmount,
    pub destination: AssetofPrecision,
}

/// Response from server once we have placed a limit order
#[derive(Clone, Debug)]
pub struct PlaceOrdersResponse {
    pub responses: Vec<PlaceOrderResponse>
}

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
        let data = response.get("data")
            .ok_or_else(|| ProtocolError("data field not found."))?
            .clone();
        let response: LimitResponseData = serde_json::from_value(data).map_err(|x| {
            ProtocolError::coerce_static_from_str(&format!("{:#?}", x))
        })?;
        Ok(ResponseOrError::from_data(response.into()))
    }

    /// Update the number of orders remaining before state sync
    async fn process_response(
        &self,
        response: &Self::Response,
        state: Arc<RwLock<State>>,
    ) -> Result<()> {
        if let Some(response) = response.responses.last() {
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
        let nonces = builder.make_payload_nonces(state.clone(), time).await?;
        let state = state.read().await;
        let affiliate = state.affiliate_code.clone();
        let query = builder.signed_graphql_request(nonces, time, affiliate, state.signer()?)?;
        serializable_to_json(&query)
    }

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        _state: Arc<RwLock<State>>,
    ) -> Result<ResponseOrError<Self::Response>> {
        let data = response.get("data")
            .ok_or_else(|| ProtocolError("data field not found."))?
            .clone();
        let response: MarketResponseData = serde_json::from_value(data).map_err(|x| {
            ProtocolError::coerce_static_from_str(&format!("{:#?}", x))
        })?;
        Ok(ResponseOrError::from_data(response.into()))    }

    /// Update the number of orders remaining before state sync
    async fn process_response(
        &self,
        response: &Self::Response,
        state: Arc<RwLock<State>>,
    ) -> Result<()> {
        if let Some(response) = response.responses.last() {
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
        get_required_hooks(state, &self.market).await.map(Some)
    }
}
