use super::super::asset_nonces::AssetNoncesRequest;
use super::super::dh_fill_pool::DhFillPoolRequest;
use super::super::list_markets::ListMarketsRequest;
use super::super::sign_all_states::SignAllStates;
use super::super::{
    serializable_to_json, try_response_from_json, NashProtocol, ResponseOrError, State,
};
use super::super::{NashProtocolRequest, ProtocolHook};
use crate::errors::Result;
use crate::graphql::place_limit_order;
use crate::graphql::place_market_order;
use crate::types::{
    AssetAmount, AssetofPrecision, Blockchain, BuyOrSell, Market, Nonce, OrderCancellationPolicy,
    OrderStatus, OrderType, Rate,
};
use crate::utils::current_time_as_i64;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::lock::Mutex;
use std::sync::Arc;

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
    pub amount: String
}

impl LimitOrderRequest {
    pub fn new(
        market: String,
        buy_or_sell: BuyOrSell,
        amount_a: &str,
        price_b: &str,
        cancellation_policy: OrderCancellationPolicy,
        allow_taker: bool,
        client_order_id: Option<String>
    ) -> Result<Self> {
        Ok(Self {
            market,
            buy_or_sell,
            amount: amount_a.to_string(),
            price: price_b.to_string(),
            cancellation_policy,
            allow_taker,
            client_order_id
        })
    }
}



impl MarketOrderRequest {
    pub fn new(
        market: String,
        amount_a: &str,
        client_order_id: Option<String>,
    ) -> Result<Self> {
        Ok(Self {
            market,
            amount: amount_a.to_string(),
            client_order_id
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

/// Response from server once we have placed a limit order
#[derive(Clone, Debug)]
pub struct PlaceOrderResponse {
    pub remaining_orders: u64,
    pub order_id: String,
    pub status: OrderStatus,
    pub placed_at: DateTime<Utc>,
    pub order_type: OrderType,
    pub buy_or_sell: BuyOrSell,
    pub market_name: String,
}

#[async_trait]
impl NashProtocol for LimitOrderRequest {
    type Response = PlaceOrderResponse;

    async fn graphql(&self, state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let builder = self.make_constructor(state.clone()).await?;
        let time = current_time_as_i64();
        let nonces = builder.make_payload_nonces(state.clone(), time).await?;
        let mut state = state.lock().await;
        // TODO: move this to process response, and incorporate error into process response
        // would be much cleaner
        if state.remaining_orders > 0 {
            state.remaining_orders -= 1;
        }
        let affiliate = state.affiliate_code.clone();
        let signer = state.signer()?;
        let query = builder.signed_graphql_request(nonces, time, affiliate, signer)?;
        serializable_to_json(&query)
    }

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        _state: Arc<Mutex<State>>
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<PlaceOrderResponse, place_limit_order::ResponseData>(response)
    }
    /// Update the number of orders remaining before state sync
    async fn process_response(
        &self,
        response: &Self::Response,
        state: Arc<Mutex<State>>,
    ) -> Result<()> {
        let mut state = state.lock().await;
        state.remaining_orders = response.remaining_orders;
        Ok(())
    }
    /// Potentially get more r values or sign states before placing an order
    async fn run_before(&self, state: Arc<Mutex<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        let mut state = state.lock().await;
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
        for chain in Blockchain::all() {
            // 10 here is too much, but we can use multiple r-values in a single request
            if state.signer()?.remaining_r_vals(chain) <= 10 {
                hooks.push(ProtocolHook::Protocol(NashProtocolRequest::DhFill(
                    DhFillPoolRequest::new(chain)?,
                )));
            }
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

        // If we are about to out of orders...
        if !state.dont_sign_states && state.remaining_orders < 20 {
            // Need to sign states
            hooks.push(ProtocolHook::SignAllState(SignAllStates::new()));
            // After signing states, need to update nonces again
            hooks.push(ProtocolHook::Protocol(NashProtocolRequest::AssetNonces(
                AssetNoncesRequest::new(),
            )));
        }

        Ok(Some(hooks))
    }
}


#[async_trait]
impl NashProtocol for MarketOrderRequest {
    type Response = PlaceOrderResponse;

    async fn graphql(&self, state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let builder = self.make_constructor(state.clone()).await?;
        let time = current_time_as_i64();
        let nonces = builder.make_payload_nonces(state.clone(), time).await?;
        let mut state = state.lock().await;
        // TODO: move this to process response, and incorporate error into process response
        // would be much cleaner
        if state.remaining_orders > 0 {
            state.remaining_orders -= 1;
        }
        let affiliate = state.affiliate_code.clone();
        let signer = state.signer()?;
        let query = builder.signed_graphql_request(nonces, time, affiliate, signer)?;
        serializable_to_json(&query)
    }

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        _state: Arc<Mutex<State>>
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<PlaceOrderResponse, place_market_order::ResponseData>(response)
    }
    /// Update the number of orders remaining before state sync
    async fn process_response(
        &self,
        response: &Self::Response,
        state: Arc<Mutex<State>>,
    ) -> Result<()> {
        let mut state = state.lock().await;
        state.remaining_orders = response.remaining_orders;
        Ok(())
    }
    /// Potentially get more r values or sign states before placing an order
    async fn run_before(&self, state: Arc<Mutex<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        let mut state = state.lock().await;
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
        for chain in Blockchain::all() {
            // 10 here is too much, but we can use multiple r-values in a single request
            if state.signer()?.remaining_r_vals(chain) <= 10 {
                hooks.push(ProtocolHook::Protocol(NashProtocolRequest::DhFill(
                    DhFillPoolRequest::new(chain)?,
                )));
            }
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

        // If have run out of orders... (temp setting conservatively)
        if !state.dont_sign_states && state.remaining_orders == 20 {
            // Need to sign states
            hooks.push(ProtocolHook::SignAllState(SignAllStates::new()));
            // After signing states, need to update nonces again
            hooks.push(ProtocolHook::Protocol(NashProtocolRequest::AssetNonces(
                AssetNoncesRequest::new(),
            )));
        }

        Ok(Some(hooks))
    }
}
