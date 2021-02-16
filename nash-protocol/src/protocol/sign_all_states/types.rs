use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::errors::{ProtocolError, Result};

use super::super::{
    asset_nonces::AssetNoncesRequest,
    list_markets::ListMarketsRequest,
    NashProtocol,
    NashProtocolPipeline, ResponseOrError, sign_states::{SignStatesRequest, SignStatesResponse}, State,
};
use super::super::{NashProtocolRequest, ProtocolHook};

/// Request to initiate pipeline for signing all states
#[derive(Clone, Debug)]
pub struct SignAllStates;

impl SignAllStates {
    pub fn new() -> Self {
        Self {}
    }
}
/// State associated with pipeline for signing all the states. The pipeline is
/// finished when `previous_response` contains no more states to sign
#[derive(Clone, Debug)]
pub struct SignAllPipelineState {
    pub num_requests: u32,
    pub previous_response: Option<SignStatesResponse>,
}

#[async_trait]
impl NashProtocolPipeline for SignAllStates {
    type PipelineState = SignAllPipelineState;
    type ActionType = SignStatesRequest;

    async fn acquire_permit(
        &self,
        state: Arc<RwLock<State>>,
    ) -> Option<tokio::sync::OwnedSemaphorePermit> {
        state.read().await.sign_all_states_semaphore.clone().acquire_owned().await.ok()
    }

    /// Initialize pipeline state to None
    async fn init_state(&self, _state: Arc<RwLock<State>>) -> Self::PipelineState {
        SignAllPipelineState {
            num_requests: 0,
            previous_response: None,
        }
    }
    // Get next step in the pipeline
    async fn next_step(
        &self,
        pipeline_state: &Self::PipelineState,
        _client_state: Arc<RwLock<State>>,
    ) -> Result<Option<Self::ActionType>> {
        // If a previous response exists...
        if let Some(request_state) = &pipeline_state.previous_response {
            // And it is not empty, then we should make a new request to sign
            if request_state.has_states_to_sign() {
                Ok(Some(SignStatesRequest::from_response(
                    request_state.clone(),
                )))
            }
            // Otherwise if empty, we are done
            else {
                Ok(None)
            }
        }
        // And if no previous response exists, then we are just starting
        else {
            Ok(Some(SignStatesRequest::new()))
        }
    }
    // We update the previous response data and number of steps taken so far
    async fn process_step(
        &self,
        result: <Self::ActionType as NashProtocol>::Response,
        pipeline_state: &mut Self::PipelineState,
    ) {
        pipeline_state.previous_response = Some(result);
        pipeline_state.num_requests += 1;
    }
    // When done just returning right now the number of requests. There is nothing for any caller to act on
    fn output(
        &self,
        state: Self::PipelineState,
    ) -> Result<ResponseOrError<<SignStatesRequest as NashProtocol>::Response>> {
        if let Some(data) = state.previous_response {
            Ok(ResponseOrError::from_data(data))
        } else {
            Err(ProtocolError("Request did not complete"))
        }
    }
    // If have run out of r values, get more before running this pipeline
    async fn run_before(&self, state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        let state = state.read().await;
        let mut hooks = Vec::new();
        // If the client doesn't currently have a list of assets, run a list markets query to
        // get that. The assets will then be stored in client state
        if state.assets.is_none() {
            hooks.push(ProtocolHook::Protocol(NashProtocolRequest::ListMarkets(
                ListMarketsRequest,
            )));
        }
        Ok(Some(hooks))
    }
    // After running this pipeline, update asset nonces
    async fn run_after(&self, _state: Arc<RwLock<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        Ok(Some(vec![ProtocolHook::Protocol(
            NashProtocolRequest::AssetNonces(AssetNoncesRequest::new()),
        )]))
    }
}
