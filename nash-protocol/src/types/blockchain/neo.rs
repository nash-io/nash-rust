//! NEO specific types shared across protocol requests

use super::super::{Amount, Asset, AssetOrCrosschain, Nonce, OrderRate, Rate};
use super::bigdecimal_to_nash_u64;
use crate::errors::{ProtocolError, Result};
use bs58::{decode, encode};
use nash_mpc::curves::secp256_r1::Secp256r1Point;
use nash_mpc::curves::traits::ECPoint;
use nash_mpc::rust_bigint::traits::Converter;
use nash_mpc::rust_bigint::BigInt;
use ripemd160::Ripemd160;
use sha2::{Digest, Sha256};

impl Rate {
    /// Convert any Rate into bytes for encoding in a NEO payload
    pub fn to_le_bytes(&self) -> Result<[u8; 8]> {
        let zero_bytes = (0 as f64).to_le_bytes();
        let bytes = match self {
            Self::OrderRate(rate) | Self::FeeRate(rate) => rate.to_le_bytes()?,
            Self::MinOrderRate | Self::MinFeeRate => zero_bytes,
            Self::MaxOrderRate => [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
            // 0.0025 * 10^8 = 250,000
            Self::MaxFeeRate => [0x90, 0xD0, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00],
        };
        Ok(bytes)
    }
}

impl OrderRate {
    /// Serialize the OrderRate to bytes for payload creation. We always use a
    /// precision of 8 and multiplication factor of 10^8
    pub fn to_le_bytes(&self) -> Result<[u8; 8]> {
        let bytes = bigdecimal_to_nash_u64(&self.to_bigdecimal(), 8)?.to_le_bytes();
        Ok(bytes)
    }
}

impl Asset {
    /// This maps assets onto their representation in the NEO SC protocol.
    /// Each asset is represented by two bytes which serve as an identifier
    pub fn to_neo_bytes(&self) -> Vec<u8> {
        match self {
            Self::ETH => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::BAT => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::OMG => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::USDC => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::USDT => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::ZRX => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::LINK => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::QNT => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::RLC => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::ANT => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::TRAC => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::GUNTHY => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::BTC => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::NOIA => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::NEO => {
                BigInt::from_hex("9B7CFFDAA674BEAE0F930EBE6085AF9093E5FE56B34A5C220CCDCF6EFC336FC5")
                    .unwrap()
                    .to_bytes()
            }
            Self::GAS => {
                BigInt::from_hex("E72D286979EE6CB1B7E65DFDDFB2E384100B8D148E7758DE42E4168B71792C60")
                    .unwrap()
                    .to_bytes()
            }
            Self::NNN => BigInt::from_hex("045fab3389daf5602fa0953b4d7db3ef7b57b753")
                .unwrap()
                .to_bytes(),
        }
    }

    /// Given two bytes asset id, return asset
    pub fn from_neo_bytes(bytes: &[u8; 32]) -> Result<Self> {
        match bytes {
            [0x9B, 0x7C, 0xFF, 0xDA, 0xA6, 0x74, 0xBE, 0xAE, 0x0F, 0x93, 0x0E, 0xBE, 0x60, 0x85, 0xAF, 0x90, 0x93, 0xE5, 0xFE, 0x56, 0xB3, 0x4A, 0x5C, 0x22, 0x0C, 0xCD, 0xCF, 0x6E, 0xFC, 0x33, 0x6F, 0xC5] => {
                Ok(Self::NEO)
            }
            [0xE7, 0x2D, 0x28, 0x69, 0x79, 0xEE, 0x6C, 0xB1, 0xB7, 0xE6, 0x5D, 0xFD, 0xDF, 0xB2, 0xE3, 0x84, 0x10, 0x0B, 0x8D, 0x14, 0x8E, 0x77, 0x58, 0xDE, 0x42, 0xE4, 0x16, 0x8B, 0x71, 0x79, 0x2C, 0x60] => {
                Ok(Self::GAS)
            }
            _ => Err(ProtocolError("Invalid Asset ID in bytes")),
        }
    }
}

impl AssetOrCrosschain {
    /// Convert asset to id in bytes interpretable by the NEO
    /// smart contract, or `0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF` if it is a cross-chain asset
    pub fn to_neo_bytes(&self) -> Vec<u8> {
        match self {
            Self::Crosschain => BigInt::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
                .unwrap()
                .to_bytes(),
            Self::Asset(asset) => asset.to_neo_bytes().to_vec(),
        }
    }
    /// Read asset bytes from a protocol payload and convert into
    /// an Asset or mark as cross-chain
    pub fn from_neo_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() == 20
            && bytes
                == [
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
                ]
                .to_vec()
        {
            Ok(Self::Crosschain)
        } else if bytes.len() == 32 {
            // convert vector to fixed-size array
            let mut arr = [0; 32];
            let bytes = &bytes[..arr.len()];
            arr.copy_from_slice(bytes);
            Ok(Self::Asset(Asset::from_neo_bytes(&arr)?))
        } else {
            Err(ProtocolError("Invalid Asset ID in bytes"))
        }
    }
}

impl Amount {
    /// Serialize Amount to Little Endian bytes for NEO payload creation.
    pub fn to_le_bytes(&self) -> Result<[u8; 8]> {
        let bytes = bigdecimal_to_nash_u64(&self.to_bigdecimal(), 8)?.to_le_bytes();
        Ok(bytes)
    }
}

impl Nonce {
    /// Serialize Nonce for NEO payload as 8 bytes LittleEndian
    pub fn to_le_bytes(&self) -> [u8; 8] {
        match self {
            Self::Value(value) => u64::from(*value).to_le_bytes(),
            Self::Crosschain => u64::from(Nonce::crosschain()).to_le_bytes(),
        }
    }
}

/// NEO address representation
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

/// NEO public key representation
#[derive(Clone, Debug, PartialEq)]
pub struct PublicKey {
    // FIXME: why?
    pub inner: Secp256r1Point,
}

impl PublicKey {
    /// Create a new NEO public key from a hex string
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
            self.inner.to_vec(),
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

    /// Get Secp256r1 point associated with NEO public key
    pub fn to_point(&self) -> Secp256r1Point {
        self.inner.clone()
    }

    /// Conver NEO public key to hex string
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
