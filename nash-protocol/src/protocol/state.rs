//! The `State` struct captures all mutable state within protocol, such as asset nonces
//! r-values, blockchain keys, and so on.

use crate::errors::{ProtocolError, Result};
use crate::types::{Asset, Market};
use std::collections::HashMap;

use super::signer::Signer;

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
    pub state_sign_refresh: bool // TODO, not 100% we need this
}

impl State {
    pub fn new(keys_path: Option<&str>) -> Result<Self> {
        let signer = match keys_path {
            Some(path) => Some(Signer::new(path)?),
            None => None,
        };
        Ok(Self {
            signer,
            remaining_orders: 0,
            affiliate_code: None,
            markets: None,
            assets: None,
            asset_nonces: None,
            assets_nonces_refresh: false,
            state_sign_refresh: false
        })
    }

    pub fn from_key_data(secret: &str, session: &str) -> Result<Self> {
        let signer = Some(Signer::from_data(secret, session)?);
        Ok(Self {
            signer,
            remaining_orders: 0,
            affiliate_code: None,
            markets: None,
            assets: None,
            asset_nonces: None,
            assets_nonces_refresh: false,
            state_sign_refresh: false
        })
    }

    pub fn signer(&mut self) -> Result<&mut Signer> {
        self.signer
            .as_mut()
            .ok_or(ProtocolError("Signer not initiated"))
    }

    pub fn get_market(&self, market_name: &str) -> Result<Market>{
        let market_map = self.markets.as_ref().ok_or(ProtocolError("Market map does not exist"))?;
        market_map.get(market_name).ok_or(ProtocolError("Market name does not exist")).map(|m| m.clone() )
    }
}
