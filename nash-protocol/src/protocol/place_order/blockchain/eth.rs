use crate::errors::{ProtocolError, Result};
use crate::graphql::place_limit_order;
use crate::graphql::place_market_order;
use crate::types::eth::Address;
use crate::types::{Amount, AssetOrCrosschain, Blockchain, Nonce, Prefix, Rate};
use crate::utils::{bigint_to_nash_r, bigint_to_nash_sig, hash_eth_message};
use nash_mpc::rust_bigint::BigInt;
use std::convert::TryInto;

use super::super::super::signer::Signer;

/// FillOrder data allows an Ethereum smart contract to execute an order that
/// has been placed for an Ethereum blockchain asset. In practice, the Nash ME
/// does not tend to directly execute these orders and instead relies on clients
/// syncing aggregate state periodically via `syncState`.

#[derive(Clone, Debug, PartialEq)]
pub struct FillOrder {
    pub prefix: Prefix,                // 1 byte,
    pub address: Address,              // 20 bytes,
    pub asset_from: AssetOrCrosschain, // 2 bytes,
    pub asset_to: AssetOrCrosschain,   // 2 bytes
    pub nonce_from: Nonce,             // 4 bytes
    pub nonce_to: Nonce,               // 4 bytes
    pub amount: Amount,                // 8 bytes
    pub min_order: Rate,               // 8 bytes
    pub max_order: Rate,               // 8 bytes
    pub fee_rate: Rate,                // 8 bytes
    pub order_nonce: Nonce,            // 4 bytes
}

impl FillOrder {
    /// Constructor for FillOrder payload
    pub fn new(
        address: Address,
        asset_from: AssetOrCrosschain,
        asset_to: AssetOrCrosschain,
        nonce_from: Nonce,
        nonce_to: Nonce,
        amount: Amount,
        min_order: Rate,
        max_order: Rate,
        fee_rate: Rate,
        order_nonce: Nonce,
    ) -> Self {
        Self {
            prefix: Prefix::FillOrder,
            address,
            asset_from,
            asset_to,
            nonce_from,
            nonce_to,
            amount,
            min_order,
            max_order,
            fee_rate,
            order_nonce,
        }
    }

    /// Serialize FillOrder payload to bytes interpretable by the Ethereum smart contract
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok([
            &self.prefix.to_bytes()[..],
            &self.address.to_bytes()[..],
            &self.asset_from.to_eth_bytes()[..],
            &self.asset_to.to_eth_bytes()[..],
            &self.nonce_from.to_be_bytes()[..],
            &self.nonce_to.to_be_bytes()[..],
            &self.amount.to_be_bytes()?[..],
            &self.min_order.to_be_bytes()?[..],
            &self.max_order.to_be_bytes()?[..],
            &self.fee_rate.to_be_bytes()?[..],
            &self.order_nonce.to_be_bytes()[..],
        ]
        .concat())
    }

    /// Serialize FillOrder payload to bytes as a hex string, interpretable by a smart contract
    pub fn to_hex(&self) -> Result<String> {
        Ok(hex::encode(self.to_bytes()?).to_uppercase())
    }

    pub fn from_hex(hex_str: &str) -> Result<Self> {
        let bytes = hex::decode(hex_str)
            .map_err(|_| ProtocolError("Could not decode FillOrder hex to bytes"))?;
        let prefix = Prefix::from_bytes(bytes[..1].try_into()?)?;
        let address = Address::from_bytes(bytes[1..21].try_into()?)?;
        let asset_from = AssetOrCrosschain::from_eth_bytes(bytes[21..23].try_into()?)?;
        let asset_to = AssetOrCrosschain::from_eth_bytes(bytes[23..25].try_into()?)?;
        let nonce_from = Nonce::from_be_bytes(bytes[25..29].try_into()?)?;
        let nonce_to = Nonce::from_be_bytes(bytes[29..33].try_into()?)?;
        let amount = Amount::from_bytes(bytes[33..41].try_into()?, 8)?;
        let min_order = Rate::from_be_bytes(bytes[41..49].try_into()?)?;
        let max_order = Rate::from_be_bytes(bytes[49..57].try_into()?)?;
        let fee_rate = Rate::from_be_bytes(bytes[57..65].try_into()?)?;
        let order_nonce = Nonce::from_be_bytes(bytes[65..69].try_into()?)?;

        Ok(Self {
            prefix,
            address,
            asset_from,
            asset_to,
            nonce_from,
            nonce_to,
            amount,
            min_order,
            max_order,
            fee_rate,
            order_nonce,
        })
    }

    /// Hash a FillOrder for signing with an Ethereum private key or Nash MPC protocol
    pub fn hash(&self) -> Result<BigInt> {
        let bytes = self.to_bytes()?;
        Ok(hash_eth_message(&bytes))
    }

    /// Construct GraphQL object corresponding to a blockchain signature on ETH fillorder data.
    pub fn to_blockchain_signature(
        &self,
        signer: &mut Signer,
    ) -> Result<place_limit_order::BlockchainSignature> {
        let payload_hash = self.hash()?;
        let (sig, r, pub_key) = signer.sign_child_key(payload_hash, Blockchain::Ethereum)?;
        let graphql_output = place_limit_order::BlockchainSignature {
            blockchain: place_limit_order::Blockchain::ETH,
            nonce_from: Some(self.nonce_from.into()),
            nonce_to: Some(self.nonce_to.into()),
            public_key: Some(pub_key),
            signature: bigint_to_nash_sig(sig),
            r: Some(bigint_to_nash_r(r)),
        };
        Ok(graphql_output)
    }

    pub fn to_market_blockchain_signature(
        &self,
        signer: &mut Signer,
    ) -> Result<place_market_order::BlockchainSignature> {
        let payload_hash = self.hash()?;
        let (sig, r, pub_key) = signer.sign_child_key(payload_hash, Blockchain::Ethereum)?;
        let graphql_output = place_market_order::BlockchainSignature {
            blockchain: place_market_order::Blockchain::ETH,
            nonce_from: Some(self.nonce_from.into()),
            nonce_to: Some(self.nonce_to.into()),
            public_key: Some(pub_key),
            signature: bigint_to_nash_sig(sig),
            r: Some(bigint_to_nash_r(r)),
        };
        Ok(graphql_output)
    }
}

#[cfg(test)]
mod tests {
    use super::{Address, Amount, FillOrder, Nonce, Rate};
    use crate::types::Asset;
    #[test]
    fn fillorder_generate_and_parse() {
        let order = FillOrder::new(
            Address::new("D58547F100B67BB99BBE8E94523B6BB4FDA76954").unwrap(),
            Asset::USDC.into(),
            Asset::ETH.into(),
            Nonce::Value(0),
            Nonce::Value(1),
            Amount::new("100.0", 8).unwrap(),
            Rate::MinOrderRate,
            Rate::MaxOrderRate,
            Rate::MaxFeeRate,
            Nonce::Value(23),
        );
        let order_hex = order.to_hex().unwrap();
        let parsed_order = FillOrder::from_hex(&order_hex).unwrap();
        let to_hex_again = parsed_order.to_hex().unwrap();
        println!("{:?}", order);
        println!("{:?}", parsed_order);
        assert_eq!(order_hex, to_hex_again);
    }

    #[test]
    fn decode_fillorder() {
        let x = FillOrder::from_hex("0136c8b049a6a32421f8a2e16c1f3c4e5efea1706000030000000000010000000100000000077359400000000af2066ba0ffffffffffffffff000000000003d09057b78d6c").unwrap();
        println!("{:#?}", x);
    }
}
