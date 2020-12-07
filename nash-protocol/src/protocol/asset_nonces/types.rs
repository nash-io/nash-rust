use super::super::list_markets::ListMarketsRequest;
use super::super::{
    serializable_to_json, try_response_from_json, NashProtocol, NashProtocolRequest, ProtocolHook,
    ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::get_assets_nonces;
use std::collections::HashMap;

use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

/// Retrieve asset nonces from Nash server. Asset nonces increment over time with
/// the property that orders and state signed with older nonces is considered
/// invalid by smart contracts on Ethereum and NEO.
#[derive(Clone, Debug)]
pub struct AssetNoncesRequest;

impl AssetNoncesRequest {
    pub fn new() -> Self {
        Self {}
    }
}

/// Response contains a `HashMap` which connects the symbol of an asset with its
/// associate nonce.
#[derive(Clone, Debug)]
pub struct AssetNoncesResponse {
    pub nonces: HashMap<String, Vec<u32>>,
}

/// Implement protocol bindings for SignStatesRequest
#[async_trait]
impl NashProtocol for AssetNoncesRequest {
    type Response = AssetNoncesResponse;
    /// Serialize a SignStates protocol request to a GraphQL string
    async fn graphql(&self, state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let mut state = state.lock().await;
        // A bit of a hack, but if the client has a list of known assets aquired from
        // doing a ListMarkets request, we will extract that and use it in the query.
        // If not, request generation will fail
        let assets = state.assets.clone();
        let signer = state.signer()?;
        let query = self.make_query(signer, assets)?;
        serializable_to_json(&query)
    }
    /// Deserialize response to SignStates protocol request
    async fn response_from_json(
        &self,
        response: serde_json::Value,
        _state: Arc<Mutex<State>>
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<AssetNoncesResponse, get_assets_nonces::ResponseData>(response)
    }
    /// Asset nonces in state
    async fn process_response(
        &self,
        response: &Self::Response,
        state: Arc<Mutex<State>>,
    ) -> Result<()> {
        let mut nonces_map = HashMap::new();
        for (key, value) in &response.nonces {
            nonces_map.insert(key.clone(), value.clone());
        }
        let mut state = state.lock().await;
        state.asset_nonces = Some(nonces_map);
        state.assets_nonces_refresh = false; // set to false as we just grabbed nonces
        Ok(())
    }

    /// If doing an AssetNonces request, insert a ListMarketsRequest before that to store asset list in client
    async fn run_before(&self, _state: Arc<Mutex<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        Ok(Some(vec![ProtocolHook::Protocol(
            NashProtocolRequest::ListMarkets(ListMarketsRequest),
        )]))
    }
}
