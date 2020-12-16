//! Request and response types associated with state signing,
//! updating account values in the Nash state channel

use super::super::{
    serializable_to_json, try_response_from_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::sign_states;
use crate::types::Blockchain;
use async_trait::async_trait;
use futures::lock::Mutex;
use nash_mpc::rust_bigint::BigInt;
use std::sync::Arc;

/// Type to generate a new sign states request. It takes an optional
/// set of `input_states` to sign. The server will return states for the
/// client to sign until no unsigned states remain.
#[derive(Clone, Debug)]
pub struct SignStatesRequest {
    pub input_states: Option<SignStatesResponse>,
}

impl SignStatesRequest {
    /// Create new SignStates protocol request
    pub fn new() -> Self {
        Self { input_states: None }
    }
    /// Create new SignStates request using response from a prior request
    pub fn from_response(sign_states: SignStatesResponse) -> Self {
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
pub struct SignStatesResponse {
    pub recycled_orders: Vec<RecycledOrder>,
    pub server_signed_states: Vec<ServerSignedData>,
    pub states: Vec<ContractBalanceState>,
}

impl SignStatesResponse {
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
impl RecycledOrder {
    pub fn state(&self) -> &StateData {
        &self.0
    }
}

/// Smart contract balance state
#[derive(Clone, Debug)]
pub struct ContractBalanceState(pub StateData);
impl ContractBalanceState {
    pub fn state(&self) -> &StateData {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct ServerSignedData {
    pub signed_data: String,
    pub blockchain: Blockchain,
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
    pub fn new(message: &str, blockchain: Blockchain, r: BigInt, signature: BigInt) -> Self {
        Self {
            message: message.to_string(),
            blockchain,
            r,
            signature,
        }
    }
}

/// Implement protocol bindings for SignStatesRequest
#[async_trait]
impl NashProtocol for SignStatesRequest {
    type Response = SignStatesResponse;
    /// Serialize a SignStates protocol request to a GraphQL string
    async fn graphql(&self, state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let mut state = state.lock().await;
        let signer = state.signer()?;
        let query = self.make_query(signer)?;
        serializable_to_json(&query)
    }
    /// Deserialize response to SignStates protocol request
    async fn response_from_json(
        &self,
        response: serde_json::Value,
        _state: Arc<Mutex<State>>
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<SignStatesResponse, sign_states::ResponseData>(response)
    }
}
