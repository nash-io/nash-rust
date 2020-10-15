use super::super::place_order::blockchain::eth::FillOrder as FillOrderEth;
use super::types::{ClientSignedState, ContractBalanceState, RecycledOrder, StateData};
use crate::errors::{ProtocolError, Result};
use crate::types::Blockchain;
use crate::utils::{decode_hexstr, hash_eth_message, hash_neo_message};
use nash_mpc::rust_bigint::BigInt;

use super::super::signer::Signer;
#[cfg(feature = "num_bigint")]
use num_traits::Num;

pub mod eth;
pub mod neo;

use eth::StateUpdatePayloadEth;

/// Representation of state update payload. User signs this payload to
/// confirm the current balance of their account and aggregate previous
/// previous state changes that may have occurred via orders.
pub enum StateUpdatePayload {
    Eth(StateUpdatePayloadEth),
}

impl ContractBalanceState {
    pub fn verify(&self) -> bool {
        let state_data = &self.0;
        match state_data.blockchain {
            Blockchain::Ethereum => {
                if let Ok(_state) = StateUpdatePayloadEth::from_hex(&state_data.payload) {
                    // TODO: verify other properties here
                    true
                } else {
                    false
                }
            }
            // TODO:
            Blockchain::NEO => true,
            Blockchain::Bitcoin => true,
        }
    }
}

impl RecycledOrder {
    pub fn verify(&self) -> bool {
        let order_data = &self.0;
        match order_data.blockchain {
            Blockchain::Ethereum => {
                if let Ok(_order) = FillOrderEth::from_hex(&order_data.payload) {
                    // TODO: verify other properties here
                    true
                } else {
                    false
                }
            }
            Blockchain::NEO => true,
            Blockchain::Bitcoin => true,
        }
    }
}

/// Sign off on a piece of state data such as an account balance or a recycled open order
/// with a higher nonce. These states can be submitted by the ME to the settlement contract
pub fn sign_state_data(state_data: &StateData, signer: &mut Signer) -> Result<ClientSignedState> {
    let data = match state_data.blockchain {
        // Recycled orders should not be double hashed, but that is what we are doing here
        Blockchain::Ethereum => hash_eth_message(&decode_hexstr(&state_data.payload)?),
        Blockchain::NEO => hash_neo_message(&decode_hexstr(&state_data.payload)?),
        // No hashing for BTC. This should be fixed in ME
        Blockchain::Bitcoin => BigInt::from_str_radix(&state_data.payload, 16)
            .map_err(|_| ProtocolError("Could not parse BTC hash as BigInt"))?,
    };
    let (sig, r, _pub) = signer.sign_child_key(data, state_data.blockchain)?;
    Ok(ClientSignedState::from_state_data(state_data, r, sig))
}

impl From<std::array::TryFromSliceError> for ProtocolError {
    fn from(_error: std::array::TryFromSliceError) -> Self {
        ProtocolError("Could not convert slice into correct number of bytes")
    }
}
