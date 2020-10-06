//! Ethereum specific types shared across protocol requests

use super::super::{Amount, Asset, AssetOrCrosschain, AssetofPrecision, Nonce, OrderRate, Rate};
use super::{bigdecimal_to_nash_u64, nash_u64_to_bigdecimal};
use crate::errors::{ProtocolError, Result};
use byteorder::{BigEndian, ReadBytesExt};
use nash_mpc::curves::secp256_k1::Secp256k1Point;
use nash_mpc::curves::traits::ECPoint;
use sha3::{Digest, Keccak256};

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

    /// Create Rate from big endian bytes in Ethereum FillOrder payload
    pub fn from_be_bytes(bytes: [u8; 8]) -> Result<Self> {
        Ok(Self::OrderRate(OrderRate::from_be_bytes(bytes)?))
    }
}

impl OrderRate {
    /// Serialize the OrderRate to bytes for payload creation. We always use a
    /// precision of 8 and multiplication factor of 10^8
    pub fn to_be_bytes(&self) -> Result<[u8; 8]> {
        let bytes = bigdecimal_to_nash_u64(&self.to_bigdecimal(), 8)?.to_be_bytes();
        Ok(bytes)
    }

    /// Create OrderRate from big endian bytes in Ethereum FillOrder payload
    pub fn from_be_bytes(bytes: [u8; 8]) -> Result<Self> {
        let num = u64::from_be_bytes(bytes);
        let big_num = nash_u64_to_bigdecimal(num, 8);
        Ok(OrderRate::from_bigdecimal(big_num))
    }
}

impl Amount {
    /// Serialize Amount to Big Endian bytes for ETH payload creation.
    pub fn to_be_bytes(&self) -> Result<[u8; 8]> {
        let bytes = bigdecimal_to_nash_u64(&self.to_bigdecimal(), 8)?.to_be_bytes();
        Ok(bytes)
    }
    /// Create an amount of given precision from ETH payload bytes
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
    /// Create a nonce from ETH payload bytes
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
        match self {
            Self::ETH => [0x00, 0x00],
            Self::BAT => [0x00, 0x01],
            Self::OMG => [0x00, 0x02],
            Self::USDC => [0x00, 0x03],
            Self::USDT => [0x00, 0x11],
            Self::ZRX => [0x00, 0x04],
            Self::LINK => [0x00, 0x05],
            Self::QNT => [0x00, 0x06],
            Self::RLC => [0x00, 0x0a],
            Self::ANT => [0x00, 0x0e],
            Self::TRAC => [0x00, 0x14],
            Self::GUNTHY => [0x00, 0x15],
            Self::BTC => [0xff, 0xff],
            Self::NEO => [0xff, 0xff],
            Self::GAS => [0xff, 0xff],
            Self::NNN => [0xff, 0xff],
        }
    }

    /// Given two bytes asset id in ETH payload, return asset
    pub fn from_eth_bytes(bytes: [u8; 2]) -> Result<Self> {
        match bytes {
            [0x00, 0x00] => Ok(Self::ETH),
            [0x00, 0x01] => Ok(Self::BAT),
            [0x00, 0x02] => Ok(Self::OMG),
            [0x00, 0x03] => Ok(Self::USDC),
            [0x00, 0x11] => Ok(Self::USDT),
            [0x00, 0x04] => Ok(Self::ZRX),
            [0x00, 0x05] => Ok(Self::LINK),
            [0x00, 0x06] => Ok(Self::QNT),
            [0x00, 0x0a] => Ok(Self::RLC),
            [0x00, 0x0e] => Ok(Self::ANT),
            [0x00, 0x14] => Ok(Self::TRAC),
            [0x00, 0x15] => Ok(Self::GUNTHY),
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

/// Ethereum addresses are 20 bytes of data that follow a specific
/// encoding, usually represented by a hex string
#[derive(Clone, Debug, PartialEq)]
pub struct Address {
    pub(crate) inner: [u8; 20],
}

impl Address {
    /// Construct a new address from a string. FIXME: probably not a major
    /// issue, but we could consider other validation here as well
    /// FIXME: we could take a public key / Secp256k1Point as input (or Secp256r1Point
    /// in case of NEO) instead of an address, and convert that point to the blockchain-specific address then.
    /// In this way, we would at least ensure that the input represents a valid point on the curve.
    pub fn new(s: &str) -> Result<Self> {
        let hex_bytes = hex::decode(s)
            .map_err(|_| ProtocolError("Could not decode string to hex in ETH address"))?;
        match hex_bytes.len() {
            20 => {
                let mut inner: [u8; 20] = [0; 20];
                inner[..20].copy_from_slice(&hex_bytes[..20]);
                Ok(Self { inner })
            }
            _ => Err(ProtocolError("Invalid address, string hex length != 20")),
        }
    }

    /// Serialize the address into bytes for payload creation
    pub fn to_bytes(&self) -> [u8; 20] {
        self.inner
    }

    /// Create an address from ETH payload bytes
    pub fn from_bytes(bytes: [u8; 20]) -> Result<Self> {
        let _to_validate = hex::encode(bytes);
        // FIXME: do some validation here
        Ok(Self { inner: bytes })
    }
}

/// Public key representation for Ethereum
#[derive(Clone, Debug, PartialEq)]
pub struct PublicKey {
    inner: Secp256k1Point,
}

impl PublicKey {
    /// Create a new Ethereum public key from a hex string
    pub fn new(hex_str: &str) -> Result<Self> {
        let inner = Secp256k1Point::from_hex(hex_str).map_err(|_| {
            ProtocolError("Could not create public key (Secp256r1Point) from hex string")
        })?;
        Ok(Self { inner })
    }

    /// generate Ethereum address from public key
    pub fn to_address(&self) -> Address {
        // remove leading byte (0x04) that indicates an uncompressed public key
        let pk_bytes = &self.inner.pk_to_key_slice()[1..];
        // hash public key
        let hash: [u8; 32] = Keccak256::digest(&pk_bytes).into();
        // last 20 hex-encoded bytes of hash are the address
        Address::new(&hex::encode(&hash[12..])).unwrap()
    }
    /// Convert Ethereum public key into a hex string
    pub fn to_hex(&self) -> String {
        self.inner.to_hex()
    }
}

#[cfg(test)]
mod tests {
    use super::{Address, PublicKey};
    use crate::types::{Amount, Asset, Nonce};

    #[test]
    fn test_serialize_and_deserialize() {
        let nonce = Nonce::Value(35);
        assert_eq!(nonce, Nonce::from_be_bytes(nonce.to_be_bytes()).unwrap());
        let address = Address::new("D58547F100B67BB99BBE8E94523B6BB4FDA76954").unwrap();
        assert_eq!(address, Address::from_bytes(address.to_bytes()).unwrap());
        let asset = Asset::ETH;
        assert_eq!(asset, Asset::from_eth_bytes(asset.to_eth_bytes()).unwrap());
        let amount = Amount::new("45", 8).unwrap();
        assert_eq!(
            amount,
            Amount::from_bytes(amount.to_be_bytes().unwrap(), 8).unwrap()
        );
    }

    #[test]
    fn test_pk_to_addr() {
        assert_eq!(
            PublicKey::new("04be641c583207c310739a23973fb7cb7336d2b835517ede791e9fa53fa5b0fc46390ebb4dab62e8b01352f37308dbff1512615856bffd3c752db95737d3bc93a4").unwrap().to_address(),
            Address::new("55D16CA38DFB219141AB6617B2872B978AF84702").unwrap(),
        );
        assert_eq!(
            PublicKey::new("0475f8a0f4eda35b7194d265df33720cf80164f196765980fae29795f713b340d93a0fa0632f56b265450c8d9d3a631c9a79089a57f40316bb3b66a09c51ba9fcb").unwrap().to_address(),
            Address::new("CA7E5135E04371D048A78C4592BA3B61A984B563").unwrap(),
        );
    }
}
