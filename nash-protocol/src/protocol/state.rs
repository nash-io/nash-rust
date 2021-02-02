//! The `State` struct captures all mutable state within protocol, such as asset nonces
//! r-values, blockchain keys, and so on.

use std::collections::HashMap;
use std::sync::Arc;

use tracing::trace;

use crate::errors::{ProtocolError, Result};
use crate::protocol::dh_fill_pool::DhFillPoolRequest;
use crate::types::{Asset, Blockchain, Market};

use super::signer::Signer;
use std::sync::atomic::{AtomicU64, Ordering};

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
    // FIXME: move r-pool from global indexmap here
    pub remaining_orders: AtomicU64,
    // optional affiliate code, will receive a share of fees generated
    pub affiliate_code: Option<String>,
    pub assets_nonces_refresh: bool,
    pub dont_sign_states: bool, // flag only for market maker users

    pub place_order_semaphore: Arc<tokio::sync::Semaphore>,
    pub sign_all_states_semaphore: Arc<tokio::sync::Semaphore>,
    pub k1_fill_pool_semaphore: Arc<tokio::sync::Semaphore>,
    pub r1_fill_pool_semaphore: Arc<tokio::sync::Semaphore>,
}

impl State {
    pub fn new(signer: Option<Signer>) -> Self {
        Self {
            signer,
            asset_nonces: None,
            markets: None,
            assets: None,
            remaining_orders: AtomicU64::new(0),
            affiliate_code: None,
            assets_nonces_refresh: false,
            dont_sign_states: false,
            // Set these here for now
            place_order_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
            sign_all_states_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
            k1_fill_pool_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
            r1_fill_pool_semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
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

    pub fn get_market(&self, market_name: &str) -> Result<Market> {
        let market_map = self
            .markets
            .as_ref()
            .ok_or(ProtocolError("Market map does not exist"))?;
        market_map
            .get(market_name)
            .ok_or(ProtocolError("Market name does not exist"))
            .map(|m| m.clone())
    }

    pub fn get_remaining_orders(&self) -> u64 {
        return self.remaining_orders.load(Ordering::Relaxed);
    }

    pub fn set_remaining_orders(&self, n: u64) {
        return self.remaining_orders.store(n, Ordering::Relaxed);
    }

    pub fn decr_remaining_orders(&self) {
        self.remaining_orders.fetch_sub(1, Ordering::Relaxed);
    }

    /// Check if pools need a refill
    pub fn get_fill_pool_schedules(
        &self,
        chains: Option<&Vec<Blockchain>>,
        r_val_fill_pool_threshold: Option<u32>,
    ) -> Result<Vec<(DhFillPoolRequest, tokio::sync::OwnedSemaphorePermit)>> {
        let mut result = Vec::new();
        let threshold = r_val_fill_pool_threshold.unwrap_or(R_VAL_FILL_POOL_THRESHOLD);
        for chain in chains.unwrap_or(Blockchain::all().as_ref()) {
            let remaining = self.signer()?.get_remaining_r_vals(chain);
            println!("{:?}: {}", chain, remaining);
            if remaining < threshold {
                let semaphore = match chain {
                    Blockchain::Bitcoin => &self.k1_fill_pool_semaphore,
                    Blockchain::Ethereum => &self.k1_fill_pool_semaphore,
                    Blockchain::NEO => &self.r1_fill_pool_semaphore,
                };
                if let Ok(permit) = semaphore.clone().try_acquire_owned() {
                    let fill_size = MAX_R_VAL_POOL_SIZE - remaining;
                    trace!(?chain, %remaining ,%fill_size, "created fill pool request");
                    result.push((DhFillPoolRequest::new(chain.clone(), fill_size)?, permit));
                }
            }
        }
        Ok(result)
    }
}

pub const MAX_R_VAL_POOL_SIZE: u32 = 100;
pub const R_VAL_FILL_POOL_THRESHOLD: u32 = 60;
