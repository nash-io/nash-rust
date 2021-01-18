//! The `State` struct captures all mutable state within protocol, such as asset nonces
//! r-values, blockchain keys, and so on.

use crate::errors::{ProtocolError, Result};
use crate::types::{Asset, Market};
use std::collections::HashMap;

use super::signer::Signer;
use std::sync::Arc;

//****************************************//
//  Protocol state representation         //
//****************************************//

/// Client state shared across the protocol.
#[derive(Debug)]
pub struct State {
    // Inside here we will have an explicit definition of all mutable
    // protocol state. To see how any particular protocol request modifies
    // state, can look at the impl of `process_response`.
    // `signer` is an wrapper around keys used by the client for signing
    pub signer: Option<Signer>,
    // incrementing `asset_nonces` are used to invalidate old state in the channel
    // here we keep track of the latest nonce for each asset
    pub asset_nonces: Option<HashMap<String, Vec<u32>>>,
    // list of markets pulled from nash
    pub markets: Option<HashMap<String, Market>>,
    // list of assets supported for trading in nash
    pub assets: Option<Vec<Asset>>,
    // remaining orders before state signing is required
    pub remaining_orders: u64,
    // optional affiliate code, will recieve a share of fees generated
    pub affiliate_code: Option<String>, // FIXME: move r-pool from global indexmap here
    pub assets_nonces_refresh: bool,
    pub state_sign_refresh: bool, // TODO, not 100% we need this
    pub dont_sign_states: bool // flag only for market maker users

    pub place_order_semaphore: Arc<tokio::sync::Semaphore>,
    pub sign_all_states_semaphore: Arc<tokio::sync::Semaphore>,
}

impl State {
    pub fn new(signer: Option<Signer>) -> Self {
        Self {
            signer,
            remaining_orders: 0,
            affiliate_code: None,
            markets: None,
            assets: None,
            asset_nonces: None,
            assets_nonces_refresh: false,
            state_sign_refresh: false,
            dont_sign_states: false,
            // Set these here for now
            // 1 place_order pipeline at a time
            place_order_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
            // 1 sign_all_states pipeline at a time
            sign_all_states_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
        }
    }

    pub fn from_keys_path(keys_path: Option<&str>) -> Result<Self> {
        let signer = match keys_path {
            Some(path) => Some(Signer::new(path)?),
            None => None,
        };
        Ok(Self::new(signer))
    }

    pub fn from_keys(secret: &str, session: &str) -> Result<Self> {
        let signer = Some(Signer::from_data(secret, session)?);
        Ok(Self::new(signer))
    }

    pub fn signer(&self) -> Result<&Signer> {
        self.signer
            .as_ref()
            .ok_or(ProtocolError("Signer not initiated"))
    }

    pub fn signer_mut(&mut self) -> Result<&mut Signer> {
        self.signer
            .as_mut()
            .ok_or(ProtocolError("Signer not initiated"))
    }

    pub fn get_market(&self, market_name: &str) -> Result<Market>{
        let market_map = self.markets.as_ref().ok_or(ProtocolError("Market map does not exist"))?;
        market_map.get(market_name).ok_or(ProtocolError("Market name does not exist")).map(|m| m.clone() )
    }
}
