use super::super::{general_canonical_string, RequestPayloadSignature};
use super::blockchain::sign_state_data;
use super::types::{ClientSignedState, SignStatesRequest};
use crate::errors::{ProtocolError, Result};
use crate::graphql;
use crate::graphql::sign_states;
use crate::types::Blockchain;
use crate::utils::{bigint_to_nash_r, bigint_to_nash_sig, current_time_as_i64};
use graphql_client::GraphQLQuery;

use super::super::signer::Signer;

impl SignStatesRequest {
    /// Create SignStates GraphQL request
    pub fn make_query(
        &self,
        signer: &Signer,
    ) -> Result<graphql_client::QueryBody<sign_states::Variables>> {
        // If we have state data from a previous request, sign it
        let (signed_orders, signed_states) = match &self.input_states {
            None => (vec![], vec![]),
            Some(states) => {
                let mut signed_orders = Vec::new();
                for order in &states.recycled_orders {
                    if let false = order.verify() {
                        return Err(ProtocolError(
                            "Recycled order payload failed to verify. Refusing to sign",
                        ));
                    }
                    let signed = sign_state_data(order.state(), signer)?;
                    signed_orders.push(signed);
                }

                let mut signed_states = Vec::new();
                for state in &states.states {
                    if let false = state.verify() {
                        return Err(ProtocolError(
                            "State balance payload failed to verify. Refusing to sign",
                        ));
                    }
                    let signed = sign_state_data(state.state(), signer)?;
                    signed_states.push(signed);
                }

                (signed_orders, signed_states)
            }
        };
        let mut params = sign_states::Variables {
            payload: sign_states::SignStatesParams {
                timestamp: current_time_as_i64(),
                sync_all: Some(true),
                signed_recycled_orders: Some(
                    signed_orders.iter().map(|x| Some(x.into())).collect(),
                ),
                client_signed_states: Some(signed_states.iter().map(|x| Some(x.into())).collect()),
            },
            signature: RequestPayloadSignature::empty().into(),
        };
        let sig_payload = sign_states_canonical_string(&params);
        let sig = signer.sign_canonical_string(&sig_payload);
        params.signature = sig.into();
        Ok(graphql::SignStates::build_query(params))
    }
}

/// Convert ugly generated `sign_states::Signature` type into common signature
impl From<RequestPayloadSignature> for sign_states::Signature {
    fn from(sig: RequestPayloadSignature) -> Self {
        sign_states::Signature {
            signed_digest: sig.signed_digest,
            public_key: sig.public_key,
        }
    }
}

/// Transform our nicer representation to the ugly wrapped one for the backend
impl From<&ClientSignedState> for sign_states::ClientSignedMessage {
    fn from(signed_state: &ClientSignedState) -> Self {
        Self {
            message: Some(signed_state.message.clone()),
            blockchain: Some(signed_state.blockchain.into()),
            r: Some(bigint_to_nash_r(signed_state.r.clone())),
            signature: Some(bigint_to_nash_sig(signed_state.signature.clone())),
        }
    }
}

/// Generate canonical payload string for sign states GraphQL request
pub fn sign_states_canonical_string(variables: &sign_states::Variables) -> String {
    let serialized_all = serde_json::to_string(variables).unwrap();
    general_canonical_string(
        "sign_states".to_string(),
        serde_json::from_str(&serialized_all).unwrap(),
        vec![
            "client_signed_states".to_string(),
            "signed_recycled_orders".to_string(),
            "sync_all".to_string(),
        ],
    )
}

impl From<Blockchain> for sign_states::Blockchain {
    fn from(chain: Blockchain) -> Self {
        match chain {
            Blockchain::Ethereum => Self::ETH,
            Blockchain::NEO => Self::NEO,
            Blockchain::Bitcoin => Self::BTC,
        }
    }
}
