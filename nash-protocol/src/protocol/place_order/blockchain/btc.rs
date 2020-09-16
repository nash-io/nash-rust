use mpc_wallet_lib::bigints::BigInt;

use crate::errors::Result;
use crate::graphql::place_limit_order;
use crate::types::{Blockchain, Nonce};
use crate::utils::{bigint_to_nash_r, bigint_to_nash_sig};

use super::super::super::signer::Signer;

/// Right now we don't implement FillOrder for BTC
#[derive(Clone, Debug, PartialEq)]
pub struct FillOrder {
    pub nonce_from: Nonce,
    pub nonce_to: Nonce,
}

impl FillOrder {
    pub fn new(nonce_from: Nonce, nonce_to: Nonce) -> Self {
        Self {
            nonce_from,
            nonce_to,
        }
    }

    pub fn to_blockchain_signature(
        &self,
        signer: &mut Signer,
    ) -> Result<place_limit_order::BlockchainSignature> {
        // The only way I could get this to work with backend is to sign garbage data...
        let (sig, r, pub_key) =
            signer.sign_child_key(BigInt::from(0 as u64), Blockchain::Bitcoin)?;
        let graphql_output = place_limit_order::BlockchainSignature {
            blockchain: place_limit_order::Blockchain::BTC,
            nonce_from: Some(self.nonce_from.into()),
            nonce_to: Some(self.nonce_to.into()),
            public_key: Some(pub_key),
            signature: bigint_to_nash_sig(sig),
            r: Some(bigint_to_nash_r(r)),
        };
        Ok(graphql_output)
    }
}
