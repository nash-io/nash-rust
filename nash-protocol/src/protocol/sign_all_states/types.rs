use super::super::{
    asset_nonces::types::AssetNoncesRequest,
    dh_fill_pool::types::DhFillPoolRequest,
    list_markets::types::ListMarketsRequest,
    sign_states::types::{SignStatesRequest, SignStatesResponseData},
    NashProtocol, NashProtocolPipeline, ResponseOrError, State,
};
use super::super::{NashProtocolRequest, ProtocolHook};
use crate::errors::{ProtocolError, Result};
use crate::types::Blockchain;

use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct SignAllStates;

impl SignAllStates {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Clone, Debug)]
pub struct SignAllPipelineState {
    pub num_requests: u32,
    pub previous_response: Option<SignStatesResponseData>,
}

#[async_trait]
impl NashProtocolPipeline for SignAllStates {
    type PipelineState = SignAllPipelineState;
    type ActionType = SignStatesRequest;
    /// Initialize pipeline state to None
    async fn init_state(&self, _state: Arc<Mutex<State>>) -> Self::PipelineState {
        SignAllPipelineState {
            num_requests: 0,
            previous_response: None,
        }
    }
    // Get next step in the pipline
    async fn next_step(
        &self,
        pipeline_state: &Self::PipelineState,
        _client_state: Arc<Mutex<State>>,
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

        for chain in Blockchain::all() {
            if state.signer()?.remaining_r_vals(chain) <= 10 {
                hooks.push(ProtocolHook::Protocol(NashProtocolRequest::DhFill(
                    DhFillPoolRequest::new(chain)?,
                )))
            }
        }
        Ok(Some(hooks))
    }
    // After running this pipeline, update asset nonces
    async fn run_after(&self, _state: Arc<Mutex<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        Ok(Some(vec![ProtocolHook::Protocol(
            NashProtocolRequest::AssetNonces(AssetNoncesRequest::new()),
        )]))
    }
}
