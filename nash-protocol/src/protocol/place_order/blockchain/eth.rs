use crate::errors::Result;
use crate::graphql::place_limit_order;
use crate::types::eth::Address;
use crate::types::{Amount, Blockchain, Nonce, Rate, AssetOrCrosschain, Prefix};
use crate::utils::{bigint_to_nash_r, bigint_to_nash_sig, hash_eth_message};
use mpc_wallet_lib::rust_bigint::BigInt;

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
}

