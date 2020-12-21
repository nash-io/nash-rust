use super::super::State;
use super::{DhFillPoolRequest, DhFillPoolResponse, ServerPublics};
use crate::errors::{ProtocolError, Result};
use crate::graphql::dh_fill_pool;
use crate::types::Blockchain;
#[cfg(feature = "secp256k1")]
use nash_mpc::curves::secp256_k1::Secp256k1Point;
#[cfg(feature = "k256")]
use nash_mpc::curves::secp256_k1_rust::Secp256k1Point;
use nash_mpc::curves::secp256_r1::Secp256r1Point;
use nash_mpc::curves::traits::ECPoint;

use futures::lock::Mutex;
use std::sync::Arc;

impl ServerPublics {
    /// Transform a list of strings provided by Nash server into points on the required
    /// ECDSA curve.
    pub fn from_hexstrings(chain: Blockchain, response: &DhFillPoolResponse) -> Result<Self> {
        match chain {
            Blockchain::Bitcoin => {
                let k1s: Result<Vec<Secp256k1Point>> = response
                    .server_publics
                    .iter()
                    .map(|hex_str| k1_from_hexstring(hex_str))
                    .collect();
                Ok(Self::Bitcoin(k1s?))
            }
            Blockchain::Ethereum => {
                let k1s: Result<Vec<Secp256k1Point>> = response
                    .server_publics
                    .iter()
                    .map(|hex_str| k1_from_hexstring(hex_str))
                    .collect();
                Ok(Self::Ethereum(k1s?))
            }
            Blockchain::NEO => {
                let r1s: Result<Vec<Secp256r1Point>> = response
                    .server_publics
                    .iter()
                    .map(|hex_str| r1_from_hexstring(hex_str))
                    .collect();
                Ok(Self::NEO(r1s?))
            }
        }
    }
    /// Get k1 values if this wrapper contains them or error
    pub fn publics_for_k1(&self) -> Result<Vec<Secp256k1Point>> {
        match self {
            Self::Bitcoin(values) => Ok(values.clone()),
            Self::Ethereum(values) => Ok(values.clone()),
            Self::NEO(_) => Err(ProtocolError("Tried to get k1 values for r1 chain")),
        }
    }
    /// Get r1 values if this wrapper contains them or error
    pub fn publics_for_r1(&self) -> Result<Vec<Secp256r1Point>> {
        match self {
            Self::NEO(values) => Ok(values.clone()),
            Self::Bitcoin(_) | Self::Ethereum(_) => {
                Err(ProtocolError("Tried to get r1 values for k1 chain"))
            }
        }
    }
}

fn r1_from_hexstring(hex_str: &str) -> Result<Secp256r1Point> {
    Secp256r1Point::from_hex(hex_str)
        .map_err(|_| ProtocolError("Could not parse Secp256r1Point from hex"))
}

fn k1_from_hexstring(hex_str: &str) -> Result<Secp256k1Point> {
    Secp256k1Point::from_hex(hex_str)
        .map_err(|_| ProtocolError("Could not parse Secp256k1Point from hex"))
}

impl From<dh_fill_pool::ResponseData> for DhFillPoolResponse {
    fn from(res: dh_fill_pool::ResponseData) -> Self {
        let server_publics = res.dh_fill_pool.iter().map(|value| value.clone()).collect();
        Self { server_publics }
    }
}

/// Fill the pool of r values with server responses
pub async fn fill_pool(
    request: &DhFillPoolRequest,
    server_publics: ServerPublics,
    state: Arc<Mutex<State>>,
) -> Result<()> {
    let mut state = state.lock().await;
    let paillier_pk = state.signer()?.paillier_pk().clone();
    // FIXME: State should manage the pools
    match request {
        DhFillPoolRequest::Bitcoin(request) | DhFillPoolRequest::Ethereum(request) => {
            let k1_secrets = request.secrets.clone();
            let k1_server_publics = server_publics.publics_for_k1()?;
            tokio::task::spawn_blocking(move || {
                nash_mpc::client::fill_rpool_secp256k1(k1_secrets, &k1_server_publics, &paillier_pk)
                    .map_err(|_| ProtocolError("Error filling k1 pool"))
            })
            .await
            .map_err(|_| ProtocolError("Error filling k1 pool"))?
        }
        DhFillPoolRequest::NEO(request) => {
            let r1_secrets = request.secrets.clone();
            let r1_server_publics = server_publics.publics_for_r1()?;
            tokio::task::spawn_blocking(move || {
                nash_mpc::client::fill_rpool_secp256r1(r1_secrets, &r1_server_publics, &paillier_pk)
                    .map_err(|_| ProtocolError("Error filling r1 pool"))
            })
            .await
            .map_err(|_| ProtocolError("Error filling r1 pool"))?
        }
    }
}
