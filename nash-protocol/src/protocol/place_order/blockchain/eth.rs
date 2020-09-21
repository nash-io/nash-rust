use super::{bigdecimal_to_nash_u64, nash_u64_to_bigdecimal};
use crate::errors::{ProtocolError, Result};
use crate::graphql::place_limit_order;
use crate::types::eth::Address;
use crate::types::{Amount, Asset, AssetofPrecision, Blockchain, Nonce, OrderRate, Rate};
use crate::types::{AssetOrCrosschain, Prefix};
use crate::utils::{bigint_to_nash_r, bigint_to_nash_sig, hash_eth_message};
use byteorder::{BigEndian, ReadBytesExt};
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

impl Rate {
    /// Convert any Rate into bytes for encoding in a Ethereum payload
    pub fn to_be_bytes(&self) -> Result<[u8; 8]> {
        let zero_bytes = (0 as f64).to_be_bytes();
        let bytes = match self {
            Self::OrderRate(rate) | Self::FeeRate(rate) => rate.to_be_bytes()?,
            Self::MinOrderRate | Self::MinFeeRate => zero_bytes,
            Self::MaxOrderRate => [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            // 0.0025 * 10^8 = 250,000
            Self::MaxFeeRate => [0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xD0, 0x90],
        };
        Ok(bytes)
    }
}

impl OrderRate {
    /// Serialize the OrderRate to bytes for payload creation. We always use a
    /// precision of 8 and multiplication factor of 10^8
    fn to_be_bytes(&self) -> Result<[u8; 8]> {
        let bytes = bigdecimal_to_nash_u64(&self.to_bigdecimal(), 8)?.to_be_bytes();
        Ok(bytes)
    }
}

impl Amount {
    /// Serialize Amount to BigEndian bytes for ETH payload creation.
    /// This will depend on its precision within a given market.
    pub fn to_be_bytes(&self) -> Result<[u8; 8]> {
        let bytes = bigdecimal_to_nash_u64(&self.to_bigdecimal(), 8)?.to_be_bytes();
        Ok(bytes)
    }

    pub fn from_bytes(bytes: [u8; 8], precision: u32) -> Result<Self> {
        let value = nash_u64_to_bigdecimal(
            (&bytes[..])
                .read_u64::<BigEndian>()
                .map_err(|_| ProtocolError("Could not convert bytes to u64"))?,
            precision,
        );
        Ok(Self { value, precision })
    }
}

impl Nonce {
    /// Serialize Nonce for ETH payload as BigEndian bytes
    pub fn to_be_bytes(&self) -> [u8; 4] {
        match self {
            Self::Value(value) => value.to_be_bytes(),
            Self::Crosschain => Nonce::crosschain().to_be_bytes(),
        }
    }

    pub fn from_be_bytes(bytes: [u8; 4]) -> Result<Self> {
        let value = (&bytes[..])
            .read_u32::<BigEndian>()
            .map_err(|_| ProtocolError("Could not read bytes into u32 for Nonce"))?;
        if value == Nonce::crosschain() {
            Ok(Nonce::Crosschain)
        } else {
            Ok(Nonce::Value(value))
        }
    }
}

impl Asset {
    /// This maps assets onto their representation in the ETH SC protocol.
    /// Each asset is represented by two bytes which serve as an identifier
    pub fn to_eth_bytes(&self) -> [u8; 2] {
        // FIXME: add the rest
        match self {
            Self::ETH => [0x00, 0x00],
            Self::BAT => [0x00, 0x01],
            Self::OMG => [0x00, 0x02],
            Self::USDC => [0x00, 0x03],
            Self::ZRX => [0x00, 0x04],
            Self::LINK => [0x00, 0x05],
            Self::QNT => [0x00, 0x06],
            Self::RLC => [0x00, 0x0a],
            Self::ANT => [0x00, 0x11],
            Self::BTC => [0xff, 0xff],
            Self::NEO => [0xff, 0xff],
            Self::GAS => [0xff, 0xff],
        }
    }

    /// Given two bytes asset id, return asset
    pub fn from_eth_bytes(bytes: [u8; 2]) -> Result<Self> {
        match bytes {
            [0x00, 0x00] => Ok(Self::ETH),
            [0x00, 0x01] => Ok(Self::BAT),
            [0x00, 0x02] => Ok(Self::OMG),
            [0x00, 0x03] => Ok(Self::USDC),
            [0x00, 0x04] => Ok(Self::ZRX),
            [0x00, 0x05] => Ok(Self::LINK),
            [0x00, 0x06] => Ok(Self::QNT),
            [0x00, 0x0a] => Ok(Self::RLC),
            [0x00, 0x11] => Ok(Self::ANT),
            _ => Err(ProtocolError("Invalid Asset ID in bytes")),
        }
    }
}

impl AssetOrCrosschain {
    /// Convert asset to id in bytes interpretable by the Ethereum
    /// smart contract, or `0xffff` if it is a cross-chain asset
    pub fn to_eth_bytes(&self) -> [u8; 2] {
        match self {
            Self::Crosschain => [0xff, 0xff],
            Self::Asset(asset) => asset.to_eth_bytes(),
        }
    }
    /// Read asset bytes from a protocol payload and convert into
    /// an Asset or mark as cross-chain
    pub fn from_eth_bytes(bytes: [u8; 2]) -> Result<Self> {
        Ok(match bytes {
            [0xff, 0xff] => Self::Crosschain,
            _ => Self::Asset(Asset::from_eth_bytes(bytes)?),
        })
    }
}

impl From<Asset> for AssetOrCrosschain {
    fn from(asset: Asset) -> Self {
        Self::Asset(asset)
    }
}

impl From<AssetofPrecision> for AssetOrCrosschain {
    fn from(asset_prec: AssetofPrecision) -> Self {
        Self::Asset(asset_prec.asset)
    }
}