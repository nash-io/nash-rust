//! Response processing for state signing

use super::types::{SignStatesResponseData, StateData, RecycledOrder, ContractBalanceState, ServerSignedData};
use crate::graphql::sign_states;
use crate::types::Blockchain;

/// Convert from the ugly auto-generated GraphQL type into a more ergonomic
/// response that we can manipulate
impl From<sign_states::ResponseData> for SignStatesResponseData {
    fn from(res: sign_states::ResponseData) -> Self {
        let mut recycled_orders = Vec::new();
        let mut server_signed_states = Vec::new();
        let mut states = Vec::new();
        let data = res.sign_states;
        for state in &data.recycled_orders {
            recycled_orders.push(RecycledOrder(StateData {
                payload_hash: state.payload_hash.clone(),
                payload: state.payload.clone(),
                blockchain: (&state.blockchain).into(),
            }));
        }
        for state in &data.server_signed_states {
            server_signed_states.push(ServerSignedData {
                signed_data: state.message.clone(),
                blockchain: (&state.blockchain).into(),
            });
        }
        for state in &data.states {
            states.push(ContractBalanceState(StateData {
                payload_hash: state.payload_hash.clone(),
                payload: state.payload.clone(),
                blockchain: (&state.blockchain).into(),
            }));
        }
        Self {
            recycled_orders,
            server_signed_states,
            states,
        }
    }
}

impl From<&sign_states::Blockchain> for Blockchain {
    fn from(chain: &sign_states::Blockchain) -> Self {
        match chain {
            sign_states::Blockchain::ETH => Self::Ethereum,
            sign_states::Blockchain::NEO => Self::NEO,
            sign_states::Blockchain::BTC => Self::Bitcoin,
            // This should never happen. Seems to be generated by the the rust graphql library, not the schema
            sign_states::Blockchain::Other(_) => {
                panic!("Nash API is serving non-supported blockchain")
            }
        }
    }
}
