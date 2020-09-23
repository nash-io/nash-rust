use super::types::{ClientSignedState, StateData, ContractBalanceState, RecycledOrder};
use crate::errors::{ProtocolError, Result};
use crate::types::Blockchain;
use crate::utils::{decode_hexstr, hash_eth_message, hash_neo_message};
use mpc_wallet_lib::rust_bigint::BigInt;


use super::super::signer::Signer;
#[cfg(feature = "num_bigint")]
use num_traits::Num;

pub mod eth;
pub mod neo;

use eth::StateUpdatePayloadEth;

pub enum StateUpdatePayload {
    Eth(StateUpdatePayloadEth)
}

impl ContractBalanceState {
    pub fn verify(&self) -> bool {
        let state_data = &self.0;
        match state_data.blockchain {
            Blockchain::Ethereum => {
                println!("{}", state_data.payload);
                if let Ok(state) = StateUpdatePayloadEth::from_hex(&state_data.payload) {
                    println!("verified ETH: {:?}", state);
                    true
                } else {
                    false
                }
            },
            Blockchain::NEO => true,
            Blockchain::Bitcoin => true
        }
    }
}

impl RecycledOrder {
    pub fn verify(&self) -> bool {
        match self.0.blockchain {
            Blockchain::Ethereum => true,
            Blockchain::NEO => true,
            Blockchain::Bitcoin => true
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
