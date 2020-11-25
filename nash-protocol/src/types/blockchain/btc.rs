//! Bitcoin specific types shared across protocol requests

use crate::errors::{ProtocolError, Result};

#[cfg(feature = "secp256k1")]
use nash_mpc::curves::secp256_k1::{Secp256k1Point, Secp256k1Scalar};
#[cfg(feature = "k256")]
use nash_mpc::curves::secp256_k1_rust::{Secp256k1Point, Secp256k1Scalar};

use nash_mpc::curves::traits::ECPoint;

/// Placeholder for BTC address
#[derive(Clone, Debug, PartialEq)]
pub struct Address {
    pub(crate) inner: String,
}

impl Address {
    pub fn new(s: &str) -> Result<Self> {
        Ok(Self {
            inner: s.to_string(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PublicKey {
    inner: Secp256k1Point,
}

impl PublicKey {
    pub fn new(hex_str: &str) -> Result<Self> {
        let inner = Secp256k1Point::from_hex(hex_str).map_err(|_| {
            ProtocolError("Could not create public key (Secp256k1Point) from hex string")
        })?;
        Ok(Self { inner })
    }

    pub fn to_address(&self) -> Result<Address> {
        Err(ProtocolError("This has not been implemented for BTC"))
    }

    pub fn to_hex(&self) -> String {
        self.inner.to_hex()
    }
}

#[cfg(test)]
mod tests {
    use super::Address;
    #[test]
    fn address() {
        let _btc_addr = Address::new("3DxbL9tNd2yCn6yqCghgkGYnUcJihMbjtw").unwrap();
    }
}
