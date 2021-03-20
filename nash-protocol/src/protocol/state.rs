//! The `State` struct captures all mutable state within protocol, such as asset nonces
//! r-values, blockchain keys, and so on.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_recursion::async_recursion;
use tracing::{info, trace};

use super::signer::Signer;
use crate::errors::{ProtocolError, Result};
use crate::protocol::dh_fill_pool::DhFillPoolRequest;
use crate::types::{Asset, Blockchain, Market};

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
    #[async_recursion]
    pub async fn acquire_fill_pool_schedules(
        &self,
        chains: Option<&'async_recursion Vec<Blockchain>>,
        r_val_fill_pool_threshold: Option<u32>,
    ) -> Result<Vec<(DhFillPoolRequest, tokio::sync::OwnedSemaphorePermit)>> {
        let mut schedules = Vec::new();
        let mut schedules_pool_types = HashSet::new();
        let threshold = r_val_fill_pool_threshold.unwrap_or(R_VAL_FILL_POOL_THRESHOLD);
        for chain in chains.unwrap_or(Blockchain::all().as_ref()) {
            let remaining = self.signer()?.get_remaining_r_vals(chain);
            info!("{:?}: {}", chain, remaining);
            if remaining < threshold {
                let (semaphore, pool_type) = match chain {
                    Blockchain::Bitcoin => (&self.k1_fill_pool_semaphore, RValPoolTypes::K1),
                    Blockchain::Ethereum => (&self.k1_fill_pool_semaphore, RValPoolTypes::K1),
                    Blockchain::NEO => (&self.r1_fill_pool_semaphore, RValPoolTypes::R1),
                };
                // Don't schedule for the same pool twice
                if !schedules_pool_types.contains(&pool_type) {
                    if let Ok(permit) = semaphore.clone().try_acquire_owned() {
                        let fill_size = MAX_R_VAL_POOL_SIZE - remaining;
                        schedules.push((DhFillPoolRequest::new(chain.clone(), fill_size)?, permit));
                        schedules_pool_types.insert(pool_type);
                        trace!(?chain, %remaining ,%fill_size, "created fill pool request");
                    } else {
                        // A bit hacky but we usually only run into this when we try to place
                        // orders immediately after starting the fill-loop. If the fill-loop
                        // is running the fill-request and we are in the place-order protocol
                        // we have to wait here for the keys to arrive.
                        let _ = semaphore
                            .acquire()
                            .await
                            .expect("Who closed the semaphore?");
                        return self
                            .acquire_fill_pool_schedules(chains, r_val_fill_pool_threshold)
                            .await;
                    }
                }
            }
        }
        Ok(schedules)
    }
}

#[derive(Hash, PartialEq, Eq)]
enum RValPoolTypes {
    R1,
    K1,
}

pub const MAX_R_VAL_POOL_SIZE: u32 = 100;
pub const R_VAL_FILL_POOL_THRESHOLD: u32 = 60;
