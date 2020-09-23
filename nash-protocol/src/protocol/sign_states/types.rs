//! Request and response types associated with state signing,
//! updating account values in the Nash state channel

use super::super::{
    try_response_from_json, serializable_to_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::sign_states;
use crate::types::{eth, Amount, Asset, Blockchain, Nonce, Prefix};
use async_trait::async_trait;
use futures::lock::Mutex;
use mpc_wallet_lib::rust_bigint::BigInt;
use std::sync::Arc;

/// Type to generate a new sign states request. It takes an optional
/// set of `input_states` to sign. The server will return states for the
/// client to sign until no unsigned states remain.
#[derive(Clone, Debug)]
pub struct SignStatesRequest {
    pub input_states: Option<SignStatesResponseData>,
}

impl SignStatesRequest {
    /// Create new SignStates protocol request
    pub fn new() -> Self {
        Self { input_states: None }
    }
    /// Create new SignStates request using response from a prior request
    pub fn from_response(sign_states: SignStatesResponseData) -> Self {
        Self {
            input_states: Some(sign_states),
        }
    }
}

/// A call to signStates will return a list of recycled orders, a list of states
/// that have already been signed by the server, and a list of new states for a
/// client to sign. When the list of `states` and `recycled_orders` is empty, this
/// means the client has nothing else to sign. This is also used as an optional input
/// argument when creating a `SignStatesRequest`, as the function is used both to get
/// a list of states to sign as well as to submit client signed states.
#[derive(Clone, Debug)]
pub struct SignStatesResponseData {
    pub recycled_orders: Vec<RecycledOrder>,
    pub server_signed_states: Vec<ServerSignedData>,
    pub states: Vec<ContractBalanceState>,
}

impl SignStatesResponseData {
    /// Return true if response data has states for the client to sign
    pub fn has_states_to_sign(&self) -> bool {
        self.recycled_orders.len() > 0 || self.states.len() > 0
    }
}
/// Common representation of smart contract state data
#[derive(Clone, Debug)]
pub struct StateData {
    pub payload: String,
    pub payload_hash: String,
    pub blockchain: Blockchain,
}

/// A representation of account and order state data coming in from the ME
/// that the client should sign and return
#[derive(Clone, Debug)]
pub struct RecycledOrder(pub StateData);
impl RecycledOrder { pub fn state(&self) -> &StateData { &self.0 } }

/// Smart contract balance state
#[derive(Clone, Debug)]
pub struct ContractBalanceState(pub StateData);
impl ContractBalanceState { pub fn state(&self) -> &StateData { &self.0 } }


#[derive(Clone, Debug)]
pub struct ServerSignedData {
    pub signed_data: String,
    pub blockchain: Blockchain
}

/// Signed state data. This may be for a state balance update or a recycled
/// order. MPC signatures always include an r value
pub struct ClientSignedState {
    pub message: String,
    pub blockchain: Blockchain,
    pub r: BigInt,
    pub signature: BigInt,
}

impl ClientSignedState {
    /// Construct signed state from a `StateData` and a signature.
    pub fn from_state_data(state_data: &StateData, r: BigInt, signature: BigInt) -> Self {
        Self {
            message: state_data.payload.clone(),
            blockchain: state_data.blockchain,
            r,
            signature,
        }
    }
}

/// Implement protocol bindings for SignStatesRequest
#[async_trait]
impl NashProtocol for SignStatesRequest {
    type Response = SignStatesResponseData;
    /// Serialize a SignStates protocol request to a GraphQL string
    async fn graphql(&self, state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let mut state = state.lock().await;
        let signer = state.signer()?;
        let query = self.make_query(signer)?;
        serializable_to_json(&query)
    }
    /// Deserialize response to SignStates protocol request
    fn response_from_json(
        &self,
        response: serde_json::Value,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<SignStatesResponseData, sign_states::ResponseData>(response)
    }
}
