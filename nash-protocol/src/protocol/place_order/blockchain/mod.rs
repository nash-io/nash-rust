pub mod btc;
pub mod eth;
pub mod neo;

use super::super::signer::Signer;
use crate::errors::Result;
use crate::graphql::place_limit_order;

/// Generic representation of FillOrder payloads across blockchains. These enable
/// Nash to settle active orders directly with the smart contract if necessary
#[derive(Clone, Debug, PartialEq)]
pub enum FillOrder {
    Ethereum(eth::FillOrder),
    Bitcoin(btc::FillOrder),
    NEO(neo::FillOrder),
}

impl FillOrder {
    pub fn to_hex(&self) -> Result<String> {
        match self {
            Self::Ethereum(fill_order) => fill_order.to_hex(),
            Self::Bitcoin(_) => Ok("".to_string()),
            Self::NEO(fill_order) => fill_order.to_hex(),
        }
    }

    pub fn to_blockchain_signature(
        &self,
        signer: &mut Signer,
    ) -> Result<place_limit_order::BlockchainSignature> {
        match self {
            Self::Ethereum(fill_order) => fill_order.to_blockchain_signature(signer),
            Self::Bitcoin(fill_order) => fill_order.to_blockchain_signature(signer),
            Self::NEO(fill_order) => fill_order.to_blockchain_signature(signer),
        }
    }
}
