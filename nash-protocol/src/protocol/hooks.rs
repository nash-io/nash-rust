//! These module contains types that allow protocol requests and pipelines to call other
//! requests and piplines via before and after hooks. These need to be explicity encoded
//! in enum types to make the compiler happy.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{Mutex, RwLock};

use crate::errors::{ProtocolError, Result};
use crate::protocol::ErrorResponse;

use super::asset_nonces::{AssetNoncesRequest, AssetNoncesResponse};
use super::cancel_all_orders::{CancelAllOrders, CancelAllOrdersResponse};
use super::dh_fill_pool::{DhFillPoolRequest, DhFillPoolResponse};
use super::list_markets::{ListMarketsRequest, ListMarketsResponse};
use super::orderbook::{OrderbookRequest, OrderbookResponse};
use super::place_order::{LimitOrderRequest, PlaceOrderResponse};
use super::sign_all_states::{SignAllPipelineState, SignAllStates};
use super::sign_states::{SignStatesRequest, SignStatesResponse};
use super::traits::{NashProtocol, NashProtocolPipeline};
use super::{ResponseOrError, State};

/// An enum wrapping all the different protocol requests
#[derive(Debug, Clone)]
pub enum NashProtocolRequest {
    AssetNonces(AssetNoncesRequest),
    DhFill(
        DhFillPoolRequest,
        Option<Arc<Mutex<Option<tokio::sync::OwnedSemaphorePermit>>>>,
    ),
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
    LimitOrder(PlaceOrderResponse),
    Orderbook(OrderbookResponse),
    CancelOrders(CancelAllOrdersResponse),
    SignState(SignStatesResponse),
    ListMarkets(ListMarketsResponse),
}

/// Implement NashProtocol for the enum, threading through to the base implementation
/// for each of the captured types. This could probably be automated wiht a macro.
#[async_trait]
impl NashProtocol for NashProtocolRequest {
    type Response = NashProtocolResponse;

    async fn acquire_permit(
        &self,
        state: Arc<RwLock<State>>,
    ) -> Option<tokio::sync::OwnedSemaphorePermit> {
        match self {
            Self::AssetNonces(nonces) => NashProtocol::acquire_permit(nonces, state).await,
            Self::DhFill(dh_fill, permit) => match permit {
                Some(permit) => permit.lock().await.take(),
                None => NashProtocol::acquire_permit(dh_fill, state).await,
            },
            Self::LimitOrder(limit_order) => NashProtocol::acquire_permit(limit_order, state).await,
            Self::Orderbook(orderbook) => NashProtocol::acquire_permit(orderbook, state).await,
            Self::SignState(sign_state) => NashProtocol::acquire_permit(sign_state, state).await,
            Self::CancelOrders(cancel_all) => NashProtocol::acquire_permit(cancel_all, state).await,
            Self::ListMarkets(list_markets) => {
                NashProtocol::acquire_permit(list_markets, state).await
            }
        }
    }

    async fn graphql(&self, state: Arc<RwLock<State>>) -> Result<serde_json::Value> {
        match self {
            Self::AssetNonces(nonces) => nonces.graphql(state).await,
            Self::DhFill(dh_fill, _permit) => dh_fill.graphql(state).await,
            Self::LimitOrder(limit_order) => limit_order.graphql(state).await,
            Self::Orderbook(orderbook) => orderbook.graphql(state).await,
            Self::SignState(sign_state) => sign_state.graphql(state).await,
            Self::CancelOrders(cancel_all) => cancel_all.graphql(state).await,
            Self::ListMarkets(list_markets) => list_markets.graphql(state).await,
        }
    }

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        state: Arc<RwLock<State>>,
    ) -> Result<ResponseOrError<Self::Response>> {
        match self {
            Self::AssetNonces(nonces) => Ok(nonces
                .response_from_json(response, state)
                .await?
                .map(Box::new(|res| NashProtocolResponse::AssetNonces(res)))),
            Self::DhFill(dh_fill, _permit) => Ok(dh_fill
                .response_from_json(response, state)
                .await?
                .map(Box::new(|res| NashProtocolResponse::DhFill(res)))),
            Self::LimitOrder(limit_order) => Ok(limit_order
                .response_from_json(response, state)
                .await?
                .map(Box::new(|res| NashProtocolResponse::LimitOrder(res)))),
            Self::Orderbook(orderbook) => Ok(orderbook
                .response_from_json(response, state)
                .await?
                .map(Box::new(|res| NashProtocolResponse::Orderbook(res)))),
            Self::SignState(sign_state) => Ok(sign_state
                .response_from_json(response, state)
                .await?
                .map(Box::new(|res| NashProtocolResponse::SignState(res)))),
            Self::CancelOrders(cancel_all) => Ok(cancel_all
                .response_from_json(response, state)
                .await?
                .map(Box::new(|res| NashProtocolResponse::CancelOrders(res)))),
            Self::ListMarkets(list_markets) => Ok(list_markets
                .response_from_json(response, state)
                .await?
                .map(Box::new(|res| NashProtocolResponse::ListMarkets(res)))),
        }
    }

    async fn process_response(
        &self,
        response: &Self::Response,
        state: Arc<RwLock<State>>,
    ) -> Result<()> {
        match (self, response) {
            (Self::AssetNonces(nonces), NashProtocolResponse::AssetNonces(response)) => {
                nonces.process_response(response, state).await?
            }
            (Self::DhFill(dh_fill, _permit), NashProtocolResponse::DhFill(response)) => {
                dh_fill.process_response(response, state).await?
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

    async fn process_error(
        &self,
        response: &ErrorResponse,
        state: Arc<RwLock<State>>,
    ) -> Result<()> {
        match self {
            Self::AssetNonces(nonces) => nonces.process_error(response, state).await,
            Self::DhFill(dh_fill, _permit) => dh_fill.process_error(response, state).await,
            Self::LimitOrder(limit_order) => limit_order.process_error(response, state).await,
            Self::Orderbook(orderbook) => orderbook.process_error(response, state).await,
            Self::SignState(sign_state) => sign_state.process_error(response, state).await,
            Self::CancelOrders(cancel_all) => cancel_all.process_error(response, state).await,
            Self::ListMarkets(list_markets) => list_markets.process_error(response, state).await,
        }
    }

    async fn run_before(&self, state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        match self {
            Self::AssetNonces(nonces) => NashProtocol::run_before(nonces, state).await,
            Self::DhFill(dh_fill, _permit) => NashProtocol::run_before(dh_fill, state).await,
            Self::LimitOrder(limit_order) => NashProtocol::run_before(limit_order, state).await,
            Self::Orderbook(orderbook) => NashProtocol::run_before(orderbook, state).await,
            Self::SignState(sign_state) => NashProtocol::run_before(sign_state, state).await,
            Self::CancelOrders(cancel_all) => NashProtocol::run_before(cancel_all, state).await,
            Self::ListMarkets(list_markets) => NashProtocol::run_before(list_markets, state).await,
        }
    }

    async fn run_after(&self, state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        match self {
            Self::AssetNonces(nonces) => NashProtocol::run_after(nonces, state).await,
            Self::DhFill(dh_fill, _permit) => NashProtocol::run_after(dh_fill, state).await,
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

    async fn acquire_permit(
        &self,
        state: Arc<RwLock<State>>,
    ) -> Option<tokio::sync::OwnedSemaphorePermit> {
        match self {
            Self::SignAllState(sign_all) => {
                NashProtocolPipeline::acquire_permit(sign_all, state).await
            }
            Self::Protocol(protocol) => NashProtocol::acquire_permit(protocol, state).await,
        }
    }

    async fn init_state(&self, state: Arc<RwLock<State>>) -> Self::PipelineState {
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
        client_state: Arc<RwLock<State>>,
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

    async fn run_before(&self, state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        match self {
            Self::Protocol(protocol) => NashProtocol::run_before(protocol, state).await,
            Self::SignAllState(sign_all) => NashProtocolPipeline::run_before(sign_all, state).await,
        }
    }

    async fn run_after(&self, state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        match self {
            Self::Protocol(protocol) => NashProtocol::run_after(protocol, state).await,
            Self::SignAllState(sign_all) => NashProtocolPipeline::run_after(sign_all, state).await,
        }
    }
}
