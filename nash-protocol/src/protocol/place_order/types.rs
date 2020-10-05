use super::super::asset_nonces::types::AssetNoncesRequest;
use super::super::dh_fill_pool::types::DhFillPoolRequest;
use super::super::list_markets::types::ListMarketsRequest;
use super::super::sign_all_states::types::SignAllStates;
use super::super::{
    serializable_to_json, try_response_from_json, NashProtocol, ResponseOrError, State,
};
use super::super::{NashProtocolRequest, ProtocolHook};
use crate::errors::Result;
use crate::graphql::place_limit_order;
use crate::types::{
    AssetAmount, AssetofPrecision, Blockchain, BuyOrSell, Market, Nonce, OrderCancellationPolicy,
    OrderStatus, OrderType, Rate,
};
use crate::utils::{current_time_as_i64, pad_zeros};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::lock::Mutex;
use std::sync::Arc;

/// Request to place limit orders on Nash exchange. On an A/B market
/// price amount will always be in terms of A and price in terms of B.
#[derive(Clone, Debug)]
pub struct LimitOrderRequest {
    pub market: Market,
    pub buy_or_sell: BuyOrSell,
    pub amount: String,
    pub price: String,
    pub cancellation_policy: OrderCancellationPolicy,
    pub allow_taker: bool,
}

impl LimitOrderRequest {
    pub fn new(
        market: Market,
        buy_or_sell: BuyOrSell,
        amount_a: &str,
        price_b: &str,
        cancellation_policy: OrderCancellationPolicy,
        allow_taker: bool,
    ) -> Result<Self> {
        // Convert input strings into the necessary precision for the market
        let amount_precision = pad_zeros(amount_a, market.asset_a.precision)?;
        let price_precision = pad_zeros(price_b, market.asset_b.precision)?;
        Ok(Self {
            market,
            buy_or_sell,
            amount: amount_precision,
            price: price_precision,
            cancellation_policy,
            allow_taker,
        })
    }
}

/// A helper type for constructing blockchain payloads and GraphQL requests
pub struct LimitOrderConstructor {
    // These fields are for GraphQL
    pub buy_or_sell: BuyOrSell,
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
pub struct LimitOrderResponse {
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
    type Response = LimitOrderResponse;

    async fn graphql(&self, state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let builder = self.make_constructor()?;
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

    fn response_from_json(
        &self,
        response: serde_json::Value,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<LimitOrderResponse, place_limit_order::ResponseData>(response)
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

        // If the client doesn't currently have a list of assets, run a list markets query to
        // get that. The assets will then be stored in client state
        if let None = state.assets {
            hooks.push(ProtocolHook::Protocol(NashProtocolRequest::ListMarkets(
                ListMarketsRequest,
            )));
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

        // If asset nonces don't exist, get them
        let mut assets_reqs = false;
        for asset in vec![self.market.asset_a.asset, self.market.asset_b.asset] {
            if state.asset_nonces.get(asset.name()) == None {
                assets_reqs = true;
            }
        }
        if assets_reqs {
            hooks.push(ProtocolHook::Protocol(NashProtocolRequest::AssetNonces(
                AssetNoncesRequest::new(),
            )));
        }

        // If have run out of orders...
        if state.remaining_orders == 0 {
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
