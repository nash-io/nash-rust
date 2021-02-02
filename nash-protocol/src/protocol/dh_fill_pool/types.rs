use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

#[cfg(feature = "secp256k1")]
use nash_mpc::curves::secp256_k1::{Secp256k1Point, Secp256k1Scalar};
#[cfg(feature = "k256")]
use nash_mpc::curves::secp256_k1_rust::{Secp256k1Point, Secp256k1Scalar};
use nash_mpc::curves::secp256_r1::{Secp256r1Point, Secp256r1Scalar};

use crate::errors::{ProtocolError, Result};
use crate::graphql::dh_fill_pool;
use crate::types::Blockchain;

use super::super::{
    serializable_to_json, try_response_from_json, NashProtocol, ResponseOrError, State,
};
use super::response;
use std::time::Duration;

/// DhFillPool requests coordinate between the user's client and the Nash server to
/// gather a set of shared secret R values. The user sends a list of public ECDSA
/// Points to the Nash server. The server sends back its own list of public Points.
/// Both parties then multply the public point by the secret value to construct the
/// same shared secret value (diffie-hellman). Bitcoin and Ethereum both use the
/// Secp256k1 curve, while NEO users the Secp256r1 curve. While this request type
/// holds both the secret and the public values, only the public values are used in
/// creating the GraphQL request. The secrets are used to process a response. Pool
/// requests will generate N new R values.
#[derive(Clone, Debug)]
pub enum DhFillPoolRequest {
    Bitcoin(K1FillPool),
    Ethereum(K1FillPool),
    NEO(R1FillPool),
}

impl DhFillPoolRequest {
    /// Create a new DhFillPool request for a given blockchain
    pub fn new(chain: Blockchain, size: u32) -> Result<Self> {
        match chain {
            Blockchain::Ethereum => Ok(Self::Ethereum(K1FillPool::new(size)?)),
            Blockchain::Bitcoin => Ok(Self::Bitcoin(K1FillPool::new(size)?)),
            Blockchain::NEO => Ok(Self::NEO(R1FillPool::new(size)?)),
        }
    }
    /// Get blockchain associated with DH request
    pub fn blockchain(&self) -> Blockchain {
        match self {
            Self::Bitcoin(_) => Blockchain::Bitcoin,
            Self::Ethereum(_) => Blockchain::Ethereum,
            Self::NEO(_) => Blockchain::NEO,
        }
    }
}

/// Values for k1 curve (Bitcoin and Ethereum)
#[derive(Clone, Debug)]
pub struct K1FillPool {
    pub publics: Vec<Secp256k1Point>,
    pub secrets: Vec<Secp256k1Scalar>,
}

impl K1FillPool {
    pub fn new(size: u32) -> Result<Self> {
        let (secrets, publics) = nash_mpc::common::dh_init_secp256k1(size)
            .map_err(|_| ProtocolError("Could not initialize k1 values"))?;
        Ok(Self { publics, secrets })
    }
}

/// Values for r1 curve (NEO)
#[derive(Clone, Debug)]
pub struct R1FillPool {
    pub publics: Vec<Secp256r1Point>,
    pub secrets: Vec<Secp256r1Scalar>,
}

impl R1FillPool {
    pub fn new(size: u32) -> Result<Self> {
        let (secrets, publics) = nash_mpc::common::dh_init_secp256r1(size)
            .map_err(|_| ProtocolError("Could not initialize r1 values"))?;
        Ok(Self { publics, secrets })
    }
}

/// Nash server returns a list of public values that we can use to
/// compute a DH shared secret
#[derive(Clone, Debug)]
pub struct DhFillPoolResponse {
    pub server_publics: Vec<String>,
}

// TODO: perhaps use this type in the response and perform conversion
/// Representation of server public keys that can be generated from a
/// response using the appropriate curve
pub enum ServerPublics {
    Bitcoin(Vec<Secp256k1Point>),
    Ethereum(Vec<Secp256k1Point>),
    NEO(Vec<Secp256r1Point>),
}

#[async_trait]
impl NashProtocol for DhFillPoolRequest {
    type Response = DhFillPoolResponse;
    /// Serialize a SignStates protocol request to a GraphQL string
    async fn graphql(&self, _state: Arc<RwLock<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        serializable_to_json(&query)
    }
    /// Deserialize response to DhFillPool protocol response
    async fn response_from_json(
        &self,
        response: serde_json::Value,
        _state: Arc<RwLock<State>>,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<DhFillPoolResponse, dh_fill_pool::ResponseData>(response)
    }
    /// Update pool with response from server
    async fn process_response(
        &self,
        response: &Self::Response,
        state: Arc<RwLock<State>>,
    ) -> Result<()> {
        let server_publics = ServerPublics::from_hexstrings(self.blockchain(), response)?;
        response::fill_pool(self, server_publics, state.clone()).await?;
        tokio::time::sleep(Duration::from_secs(5)).await;
        // Update state to indicate we now have N new r values
        state
            .read()
            .await
            .signer()?
            .fill_r_vals(self.blockchain(), response.server_publics.len() as u32);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::executor;
    use tokio::sync::RwLock;

    use crate::protocol::signer::MAX_R_VAL_POOL_SIZE;
    use crate::protocol::State;

    use super::{Blockchain, DhFillPoolRequest, NashProtocol};

    #[test]
    fn serialize_dh_fill_pool() {
        let state = Arc::new(RwLock::new(
            State::from_keys_path(Some("../nash-native-client/test_data/keyfile.json")).unwrap(),
        ));
        let async_block = async {
            println!(
                "{:?}",
                DhFillPoolRequest::new(Blockchain::Ethereum, MAX_R_VAL_POOL_SIZE)
                    .unwrap()
                    .graphql(state)
                    .await
                    .unwrap()
            );
        };
        executor::block_on(async_block);
    }
}
