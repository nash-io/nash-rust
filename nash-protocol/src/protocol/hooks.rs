//! These module contains types that allow protocol requests and pipelines to call other
//! requests and piplines via before and after hooks. These need to be explicity encoded
//! in enum types to make the compiler happy.

use super::asset_nonces::types::{AssetNoncesRequest, AssetNoncesResponse};
use super::cancel_all_orders::types::{CancelAllOrders, CancelAllOrdersResponse};
use super::dh_fill_pool::types::{DhFillPoolRequest, DhFillPoolResponse};
use super::list_markets::types::{ListMarketsRequest, ListMarketsResponse};
use super::orderbook::types::{OrderbookRequest, OrderbookResponse};
use super::place_order::types::{LimitOrderRequest, LimitOrderResponse};
use super::sign_all_states::{types::SignAllPipelineState, SignAllStates};
use super::sign_states::types::{SignStatesRequest, SignStatesResponseData};
use super::traits::{NashProtocol, NashProtocolPipeline};
use super::{ResponseOrError, State};

use crate::errors::{ProtocolError, Result};
use async_trait::async_trait;

use futures::lock::Mutex;
use std::sync::Arc;

/// An enum wrapping all the different protocol requests
#[derive(Clone, Debug)]
pub enum NashProtocolRequest {
    AssetNonces(AssetNoncesRequest),
    DhFill(DhFillPoolRequest),
    LimitOrder(LimitOrderRequest),
    Orderbook(OrderbookRequest),
    CancelOrders(CancelAllOrders),
    SignState(SignStatesRequest),
    ListMarkets(ListMarketsRequest),
}

/// An enum wrapping all the different protocol responses
#[derive(Debug)]
pub enum NashProtocolResponse {
    AssetNonces(AssetNoncesResponse),
    DhFill(DhFillPoolResponse),
    LimitOrder(LimitOrderResponse),
    Orderbook(OrderbookResponse),
    CancelOrders(CancelAllOrdersResponse),
    SignState(SignStatesResponseData),
    ListMarkets(ListMarketsResponse),
}

/// Implement NashProtocol for the enum, threading through to the base implementation
/// for each of the captured types. This could probably be automated wiht a macro.
#[async_trait]
impl NashProtocol for NashProtocolRequest {
    type Response = NashProtocolResponse;

    async fn graphql(&self, state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        match self {
            Self::AssetNonces(nonces) => nonces.graphql(state).await,
            Self::DhFill(dh_req) => dh_req.graphql(state).await,
            Self::LimitOrder(limit_order) => limit_order.graphql(state).await,
            Self::Orderbook(orderbook) => orderbook.graphql(state).await,
            Self::SignState(sign_state) => sign_state.graphql(state).await,
            Self::CancelOrders(cancel_all) => cancel_all.graphql(state).await,
            Self::ListMarkets(list_markets) => list_markets.graphql(state).await,
        }
    }

    fn response_from_json(
        &self,
        response: serde_json::Value,
    ) -> Result<ResponseOrError<Self::Response>> {
        match self {
            Self::AssetNonces(nonces) => Ok(nonces
                .response_from_json(response)?
                .map(Box::new(|res| NashProtocolResponse::AssetNonces(res)))),
            Self::DhFill(dh_req) => Ok(dh_req
                .response_from_json(response)?
                .map(Box::new(|res| NashProtocolResponse::DhFill(res)))),
            Self::LimitOrder(limit_order) => Ok(limit_order
                .response_from_json(response)?
                .map(Box::new(|res| NashProtocolResponse::LimitOrder(res)))),
            Self::Orderbook(orderbook) => Ok(orderbook
                .response_from_json(response)?
                .map(Box::new(|res| NashProtocolResponse::Orderbook(res)))),
            Self::SignState(sign_state) => Ok(sign_state
                .response_from_json(response)?
                .map(Box::new(|res| NashProtocolResponse::SignState(res)))),
            Self::CancelOrders(cancel_all) => Ok(cancel_all
                .response_from_json(response)?
                .map(Box::new(|res| NashProtocolResponse::CancelOrders(res)))),
            Self::ListMarkets(list_markets) => Ok(list_markets
                .response_from_json(response)?
                .map(Box::new(|res| NashProtocolResponse::ListMarkets(res)))),
        }
    }

    async fn process_response(
        &self,
        response: &Self::Response,
        state: Arc<Mutex<State>>,
    ) -> Result<()> {
        match (self, response) {
            (Self::AssetNonces(nonces), NashProtocolResponse::AssetNonces(response)) => {
                nonces.process_response(response, state).await?
            }
            (Self::DhFill(dh_req), NashProtocolResponse::DhFill(response)) => {
                dh_req.process_response(response, state).await?
            }
            (Self::SignState(sign_req), NashProtocolResponse::SignState(response)) => {
                sign_req.process_response(response, state).await?
            }
            (Self::LimitOrder(limit_order), NashProtocolResponse::LimitOrder(response)) => {
                limit_order.process_response(response, state).await?
            }
            (Self::CancelOrders(cancel_all), NashProtocolResponse::CancelOrders(response)) => {
                cancel_all.process_response(response, state).await?
            }
            (Self::ListMarkets(list_markets), NashProtocolResponse::ListMarkets(response)) => {
                list_markets.process_response(response, state).await?
            }
            _ => {
                return Err(ProtocolError(
                    "Attempting to process a differently typed response. This should never happen.
                    If you are seeing this error, there is something wrong with the client
                    implementation of the generic protocol runtime loop.",
                ))
            }
        };
        Ok(())
    }

    async fn run_before(&self, state: Arc<Mutex<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        match self {
            Self::AssetNonces(nonces) => NashProtocol::run_before(nonces, state).await,
            Self::DhFill(dh_req) => NashProtocol::run_before(dh_req, state).await,
            Self::LimitOrder(limit_order) => NashProtocol::run_before(limit_order, state).await,
            Self::Orderbook(orderbook) => NashProtocol::run_before(orderbook, state).await,
            Self::SignState(sign_state) => NashProtocol::run_before(sign_state, state).await,
            Self::CancelOrders(cancel_all) => NashProtocol::run_before(cancel_all, state).await,
            Self::ListMarkets(list_markets) => NashProtocol::run_before(list_markets, state).await,
        }
    }

    async fn run_after(&self, state: Arc<Mutex<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        match self {
            Self::AssetNonces(nonces) => NashProtocol::run_after(nonces, state).await,
            Self::DhFill(dh_req) => NashProtocol::run_after(dh_req, state).await,
            Self::LimitOrder(limit_order) => NashProtocol::run_after(limit_order, state).await,
            Self::Orderbook(orderbook) => NashProtocol::run_after(orderbook, state).await,
            Self::SignState(sign_state) => NashProtocol::run_after(sign_state, state).await,
            Self::CancelOrders(cancel_all) => NashProtocol::run_after(cancel_all, state).await,
            Self::ListMarkets(list_markets) => NashProtocol::run_after(list_markets, state).await,
        }
    }
}

/// Captures and protocol request or pipeline that can be executed in a `run_before`
/// or `run_after` hook for the `NashProtocol` and `NashProtocolPipeline` traits.
#[derive(Clone, Debug)]
pub enum ProtocolHook {
    SignAllState(SignAllStates),
    Protocol(NashProtocolRequest),
}

/// State representation for `ProtocolHook`
pub enum ProtocolHookState {
    SignAllStates(<SignAllStates as NashProtocolPipeline>::PipelineState),
    Protocol(<NashProtocolRequest as NashProtocolPipeline>::PipelineState),
}

/// Implement `NashProtocolPipeline` for `ProtocolHook` so that hooks can be run as typical pipelines
#[async_trait]
impl NashProtocolPipeline for ProtocolHook {
    type PipelineState = ProtocolHookState;
    type ActionType = NashProtocolRequest;

    async fn init_state(&self, state: Arc<Mutex<State>>) -> Self::PipelineState {
        match self {
            Self::SignAllState(sign_all) => {
                ProtocolHookState::SignAllStates(sign_all.init_state(state).await)
            }
            Self::Protocol(protocol) => {
                ProtocolHookState::Protocol(protocol.init_state(state).await)
            }
        }
    }

    async fn next_step(
        &self,
        pipeline_state: &Self::PipelineState,
        client_state: Arc<Mutex<State>>,
    ) -> Result<Option<Self::ActionType>> {
        match (self, pipeline_state) {
            (Self::SignAllState(sign_all), ProtocolHookState::SignAllStates(sign_all_state)) => {
                Ok(sign_all
                    .next_step(sign_all_state, client_state)
                    .await?
                    .map(|x| NashProtocolRequest::SignState(x)))
            }
            (Self::Protocol(protocol), ProtocolHookState::Protocol(protocol_state)) => {
                protocol.next_step(protocol_state, client_state).await
            }
            _ => Err(ProtocolError("Protocol does not align with action")),
        }
    }

    async fn process_step(
        &self,
        result: <NashProtocolRequest as NashProtocol>::Response,
        pipeline_state: &mut Self::PipelineState,
    ) {
        match (self, pipeline_state) {
            (ProtocolHook::SignAllState(request), ProtocolHookState::SignAllStates(state)) => {
                if let NashProtocolResponse::SignState(response) = result {
                    request.process_step(response, state).await
                }
            }
            (ProtocolHook::Protocol(request), ProtocolHookState::Protocol(state)) => {
                request.process_step(result, state).await
            }
            _ => {}
        }
    }

    fn output(
        &self,
        pipeline_state: Self::PipelineState,
    ) -> Result<ResponseOrError<NashProtocolResponse>> {
        match pipeline_state {
            ProtocolHookState::SignAllStates(SignAllPipelineState {
                previous_response: Some(response),
                ..
            }) => Ok(ResponseOrError::from_data(NashProtocolResponse::SignState(
                response,
            ))),
            ProtocolHookState::Protocol(Some(protocol_state)) => Ok(protocol_state),
            _ => Err(ProtocolError("Pipeline did not return state")),
        }
    }

    async fn run_before(&self, state: Arc<Mutex<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        match self {
            Self::Protocol(protocol) => NashProtocol::run_before(protocol, state).await,
            Self::SignAllState(sign_all) => NashProtocolPipeline::run_before(sign_all, state).await,
        }
    }

    async fn run_after(&self, state: Arc<Mutex<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        match self {
            Self::Protocol(protocol) => NashProtocol::run_after(protocol, state).await,
            Self::SignAllState(sign_all) => NashProtocolPipeline::run_after(sign_all, state).await,
        }
    }
}
