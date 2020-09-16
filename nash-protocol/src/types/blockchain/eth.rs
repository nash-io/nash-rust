//! Ethereum specific types shared across protocol requests

use crate::errors::{ProtocolError, Result};
use mpc_wallet_lib::curves::secp256_k1::Secp256k1Point;
use mpc_wallet_lib::curves::traits::ECPoint;
use sha3::{Digest, Keccak256};

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

    pub fn from_bytes(bytes: [u8; 20]) -> Result<Self> {
        let _to_validate = hex::encode(bytes);
        // FIXME: do some validation here
        Ok(Self { inner: bytes })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PublicKey {
    inner: Secp256k1Point,
}

impl PublicKey {
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
