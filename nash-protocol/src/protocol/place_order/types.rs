use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::{Mutex, RwLock};
use tracing::error;
use serde::{Serialize, Deserialize};

use crate::errors::Result;
use crate::graphql::place_limit_order;
use crate::graphql::place_market_order;
use crate::protocol::ErrorResponse;
use crate::protocol::{
    asset_nonces::AssetNoncesRequest, list_markets::ListMarketsRequest, serializable_to_json,
    sign_all_states::SignAllStates, try_response_from_json, NashProtocol, NashProtocolRequest,
    ProtocolHook, ResponseOrError, State,
};
use crate::types::{
    AssetAmount, AssetofPrecision, BuyOrSell, Market, Nonce, OrderCancellationPolicy, OrderStatus,
    OrderType, Rate,
};
use crate::utils::current_time_as_i64;

/// Request to place limit orders on Nash exchange. On an A/B market
/// price amount will always be in terms of A and price in terms of B.
#[derive(Clone, Debug)]
pub struct LimitOrderRequest {
    pub market: String,
    pub client_order_id: Option<String>,
    pub buy_or_sell: BuyOrSell,
    pub amount: String,
    pub price: String,
    pub cancellation_policy: OrderCancellationPolicy,
    pub allow_taker: bool,
}

#[derive(Clone, Debug)]
pub struct MarketOrderRequest {
    pub client_order_id: Option<String>,
    pub market: String,
    pub amount: String,
}

impl LimitOrderRequest {
    pub fn new(
        market: String,
        buy_or_sell: BuyOrSell,
        amount_a: &str,
        price_b: &str,
        cancellation_policy: OrderCancellationPolicy,
        allow_taker: bool,
        client_order_id: Option<String>,
    ) -> Result<Self> {
        Ok(Self {
            market,
            buy_or_sell,
            amount: amount_a.to_string(),
            price: price_b.to_string(),
            cancellation_policy,
            allow_taker,
            client_order_id,
        })
    }
}

impl MarketOrderRequest {
    pub fn new(market: String, amount_a: &str, client_order_id: Option<String>) -> Result<Self> {
        Ok(Self {
            market,
            amount: amount_a.to_string(),
            client_order_id,
        })
    }
}

/// A helper type for constructing blockchain payloads and GraphQL requests
pub struct LimitOrderConstructor {
    // These fields are for GraphQL
    pub buy_or_sell: BuyOrSell,
    pub client_order_id: Option<String>,
    pub market: Market,
    pub me_amount: AssetAmount,
    pub me_rate: Rate,
    pub cancellation_policy: OrderCancellationPolicy,
    pub allow_taker: bool,
    // These fields are for the smart contracts
    pub source: AssetAmount,
    pub destination: AssetofPrecision,
    pub rate: Rate,
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

/// Helper type to hold all nonces for payload construction and make
/// passing them as arguments more descriptive.
#[derive(Clone, Debug, Copy)]
pub struct PayloadNonces {
    pub nonce_from: Nonce,
    pub nonce_to: Nonce,
    pub order_nonce: Nonce,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarketName {
    pub name: String
}

/// Response from server once we have placed a limit order
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlaceOrderResponse {
    #[serde(rename = "ordersTillSignState")]
    pub remaining_orders: u64,
    #[serde(rename = "id")]
    pub order_id: String,
    pub status: OrderStatus,
    pub placed_at: DateTime<Utc>,
    #[serde(rename = "type")]
    pub order_type: OrderType,
    pub buy_or_sell: BuyOrSell,
    pub market: MarketName,
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
impl NashProtocol for LimitOrderRequest {
    type Response = PlaceOrderResponse;

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
        let market = state.get_market(&self.market)?;
        let order_precision = 8;
        let fee_precision = market.min_trade_size_b.asset.precision;
        let query = builder.signed_graphql_request(nonces, time, affiliate, state.signer()?, order_precision, fee_precision)?;
        let json = serializable_to_json(&query);
        json
    }

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        _state: Arc<RwLock<State>>,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<PlaceOrderResponse, place_limit_order::ResponseData>(response)
    }

    /// Update the number of orders remaining before state sync
    async fn process_response(
        &self,
        response: &Self::Response,
        state: Arc<RwLock<State>>,
    ) -> Result<()> {
        let state = state.read().await;
        state.set_remaining_orders(response.remaining_orders);
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
        get_required_hooks(state, &self.market).await.map(Some)
    }
}

#[async_trait]
impl NashProtocol for MarketOrderRequest {
    type Response = PlaceOrderResponse;

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
        let market = state.get_market(&self.market)?;
        let order_precision = 8;
        let fee_precision = market.min_trade_size_b.asset.precision;
        let query = builder.signed_graphql_request(nonces, time, affiliate, state.signer()?, order_precision, fee_precision)?;
        serializable_to_json(&query)
    }

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        _state: Arc<RwLock<State>>,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<PlaceOrderResponse, place_market_order::ResponseData>(response)
    }

    /// Update the number of orders remaining before state sync
    async fn process_response(
        &self,
        response: &Self::Response,
        state: Arc<RwLock<State>>,
    ) -> Result<()> {
        let state = state.read().await;
        // TODO: Incorporate error into process response
        state.set_remaining_orders(response.remaining_orders);
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
