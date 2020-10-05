use crate::errors::{ProtocolError, Result};
use crate::types::blockchain::{eth::Address, Prefix};
use crate::types::{Amount, Asset, Nonce};
use std::convert::TryInto;

// State update payload returned by Nash ME which we should validate.
// State updates set `asset_id` to a new `balance`. The `nonce` used in
// the update must be higher than any previous nonce.
#[derive(Debug, PartialEq)]
pub struct StateUpdatePayloadEth {
    pub prefix: Prefix,   // 1 byte
    pub asset_id: Asset,  // 2 bytes
    pub balance: Amount,  // 8 bytes
    pub nonce: Nonce,     // 4 bytes
    pub address: Address, // 20 bytes
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
        let address = Address::from_bytes(bytes[15..35].try_into()?)?;
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
    use super::{Address, Amount, Asset, Nonce, Prefix, StateUpdatePayloadEth};

    #[test]
    fn test_new_payload() {
        let _state_update = StateUpdatePayloadEth::from_hex(
            "00000e000000000000f02c00000755cfe0b67c5ac477191cc1bae28f1f7b016027be20",
        )
        .unwrap();
    }

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
