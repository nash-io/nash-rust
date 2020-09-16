use crate::graphql;
use graphql::dh_fill_pool;
use graphql_client::GraphQLQuery;
use mpc_wallet_lib::curves::traits::ECPoint;

use super::types::{DhFillPoolRequest, K1FillPool, R1FillPool};
use crate::types::Blockchain;

impl DhFillPoolRequest {
    /// Build a GraphQL request to get R values for MPC signing. These values can be for
    /// `secp256k1` or `secp256r1` depending on `curve`. This sets the state `r1` or `k1`
    /// in the client with the secret values associated with DH, which will be used on
    /// a future request to `set_pool`
    pub fn make_query(&self) -> graphql_client::QueryBody<dh_fill_pool::Variables> {
        // FIXME: this is ugly
        let dh_publics = match self {
            Self::Bitcoin(K1FillPool {
                publics,
                secrets: _,
            }) => publics.iter().map(|x| Some(x.to_hex())).collect(),
            Self::Ethereum(K1FillPool {
                publics,
                secrets: _,
            }) => publics.iter().map(|x| Some(x.to_hex())).collect(),
            Self::NEO(R1FillPool {
                publics,
                secrets: _,
            }) => publics.iter().map(|x| Some(x.to_hex())).collect(),
        };
        let dh_args = dh_fill_pool::Variables {
            dh_publics,
            blockchain: self.blockchain().into(),
        };
        graphql::DhFillPool::build_query(dh_args)
    }
}

impl Into<dh_fill_pool::Blockchain> for Blockchain {
    fn into(self) -> dh_fill_pool::Blockchain {
        match self {
            Blockchain::Ethereum => dh_fill_pool::Blockchain::ETH,
            Blockchain::Bitcoin => dh_fill_pool::Blockchain::BTC,
            Blockchain::NEO => dh_fill_pool::Blockchain::NEO,
        }
    }
}
