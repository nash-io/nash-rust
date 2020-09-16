//! NEO specific types shared across protocol requests

use crate::errors::{ProtocolError, Result};
use bs58::{decode, encode};
use mpc_wallet_lib::bigints::traits::Converter;
use mpc_wallet_lib::bigints::BigInt;
use mpc_wallet_lib::curves::secp256_r1::Secp256r1Point;
use mpc_wallet_lib::curves::traits::ECPoint;
use ripemd160::Ripemd160;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq)]
pub struct Address {
    inner: BigInt,
}

impl Address {
    /// Create address from the typical base58 encoded string
    pub fn new(addr: &str) -> Result<Self> {
        // base56check implements some basic verification
        let bytes = decode(addr)
            .with_check(None)
            .into_vec()
            .map_err(|_| ProtocolError("Could not base58check decode NEO address"))?;
        let hex_str = hex::encode(bytes);
        // Copied from neon-js
        Self::from_script_hash(&hex_str[2..42])
    }

    /// Create address from a script hash
    pub fn from_script_hash(s: &str) -> Result<Self> {
        Ok(Self {
            // FIXME: also do some verification
            inner: BigInt::from_hex(s)
                .map_err(|_| ProtocolError("Could not parse NEO script hash as BigInt"))?,
        })
    }

    /// Serialize the address into bytes for payload creation
    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.to_bytes()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PublicKey {
    // FIXME: why?
    pub inner: Secp256r1Point,
}

impl PublicKey {
    pub fn new(hex_str: &str) -> Result<Self> {
        let inner = Secp256r1Point::from_hex(hex_str).map_err(|_| {
            ProtocolError("Could not create public key (Secp256r1Point) from hex string")
        })?;
        Ok(Self { inner })
    }

    /// generate Neo address from public key
    pub fn to_address(&self) -> Address {
        // 0x21 (PUSHBYTES21 opcode = size of compressed public key) | compressed public key | 0xac (CHECKSIG opcode)
        let addr_script = [
            vec![0x21],
            self.inner.bytes_compressed_to_big_int().to_bytes(),
            vec![0xac],
        ]
        .concat();

        // compute script hash: sha256 first, then ripemd160
        let hash = Ripemd160::digest(&Sha256::digest(&addr_script));

        // prefix hash with some NEO-specific version (0x17)
        let neo_hash = [vec![0x17], hash.to_vec()].concat();

        // base58check encode
        let address = encode(neo_hash).with_check().into_string();
        Address::new(&address).unwrap()
    }

    pub fn to_point(&self) -> Secp256r1Point {
        self.inner
    }

    pub fn to_hex(&self) -> String {
        self.inner.to_hex()
    }
}

#[cfg(test)]
mod tests {
    use super::{Address, PublicKey};

    #[test]
    fn test_pk_to_addr() {
        assert_eq!(
            PublicKey::new("035a928f201639204e06b4368b1a93365462a8ebbff0b8818151b74faab3a2b61a")
                .unwrap()
                .to_address(),
            Address::new("AXaXZjZGA3qhQRTCsyG5uFKr9HeShgVhTF").unwrap(),
        );
        assert_eq!(
            PublicKey::new("027973267230b7cba0724589653e667ddea7aa8479c01a82bf8dd398cec93508ef")
                .unwrap()
                .to_address(),
            Address::new("AayaivCAcYnM8q79JCrfpRGXrCEHJRN5bV").unwrap(),
        );
        assert_eq!(
            PublicKey::new("0208035f32c5d0e59f71c55b1e060f6d504c004222c25670ff2b788d76a84af2f2")
                .unwrap()
                .to_address(),
            Address::new("AVSFwsxsedGGip1FgJdjavVBuFHHRXD2Uz").unwrap(),
        );
        assert_eq!(
            PublicKey::new("03a41d39a4209e18eb79245ea368674edf335a4ebeb8ed344b87d3ccaf3a2c730e")
                .unwrap()
                .to_address(),
            Address::new("ANwZ2RFRKrASBvZifGgjqqJNUbfJUW6gbn").unwrap(),
        );
    }
}
