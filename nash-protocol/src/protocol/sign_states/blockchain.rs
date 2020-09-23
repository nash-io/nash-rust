use super::types::{ClientSignedState, StateData};
use crate::errors::{ProtocolError, Result};
use crate::types::eth;
use crate::types::Prefix;
use crate::types::{Amount, Asset, Blockchain, Nonce};
use crate::utils::{decode_hexstr, hash_eth_message, hash_neo_message};
use mpc_wallet_lib::rust_bigint::BigInt;
use std::convert::TryInto;

use super::super::signer::Signer;
#[cfg(feature = "num_bigint")]
use num_traits::Num;

use super::types::StateUpdatePayloadEth;

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

impl StateUpdatePayloadEth {
    pub fn from_hex(hex_str: &str) -> Result<Self> {
        let bytes = hex::decode(hex_str)
            .map_err(|_| ProtocolError("Could not decode StateUpdate hex to bytes"))?;
        let prefix = Prefix::from_bytes(bytes[..1].try_into()?)?;
        let asset_id = Asset::from_eth_bytes(bytes[1..3].try_into()?)?;
        // SC always encodes at 8 precision
        let balance = Amount::from_bytes(bytes[3..11].try_into()?, 8)?;
        let nonce = Nonce::from_be_bytes(bytes[11..15].try_into()?)?;
        let address = eth::Address::from_bytes(bytes[15..35].try_into()?)?;
        Ok(Self {
            prefix,
            asset_id,
            balance,
            nonce,
            address,
        })
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok([
            &self.asset_id.to_eth_bytes()[..],
            &self.balance.to_be_bytes()?[..],
            &self.nonce.to_be_bytes()[..],
            &self.address.to_bytes()[..],
        ]
        .concat())
    }
}

#[cfg(test)]
mod tests {
    use super::{eth::Address, Amount, Asset, Nonce, Prefix, StateUpdatePayloadEth};

    #[test]
    fn test_eth() {
        let eth_update = StateUpdatePayloadEth {
            prefix: Prefix::SyncState,
            asset_id: Asset::ETH,
            balance: Amount::new("0", 8).unwrap(),
            nonce: Nonce::Value(44),
            address: Address::new("D58547F100B67BB99BBE8E94523B6BB4FDA76954").unwrap(),
        };
        let state_update = StateUpdatePayloadEth::from_hex(
            "00000000000000000000000000002cd58547f100b67bb99bbe8e94523b6bb4fda76954",
        )
        .unwrap();
        println!("{:?}", state_update);
        assert_eq!(eth_update, state_update);
    }

    #[test]
    fn test_bat() {
        let bat_state = StateUpdatePayloadEth::from_hex(
            "0000010000000177825f000000000ed58547f100b67bb99bbe8e94523b6bb4fda76954",
        )
        .unwrap();
        println!("{:?}", bat_state);
        let bat_update = StateUpdatePayloadEth {
            prefix: Prefix::SyncState,
            asset_id: Asset::BAT,
            balance: Amount::new("63.0", 8).unwrap(),
            nonce: Nonce::Value(14),
            address: Address::new("D58547F100B67BB99BBE8E94523B6BB4FDA76954").unwrap(),
        };
        assert_eq!(bat_state, bat_update);
    }
}
