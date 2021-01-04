use crate::errors::{ProtocolError, Result};
use crate::graphql;
use crate::graphql::place_limit_order;
use crate::graphql::place_market_order;
use crate::types::neo::PublicKey as NeoPublicKey;
use crate::types::PublicKey;
use crate::types::{
    Asset, AssetAmount, Blockchain, BuyOrSell, Nonce, OrderCancellationPolicy, OrderRate, Rate,
};
use crate::utils::pad_zeros;
use graphql_client::GraphQLQuery;
use std::convert::TryInto;

use super::super::signer::Signer;
use super::super::{general_canonical_string, RequestPayloadSignature, State};
use super::blockchain::{btc, eth, neo, FillOrder};
use super::types::{
    LimitOrderConstructor, LimitOrderRequest, MarketOrderConstructor, MarketOrderRequest,
    PayloadNonces,
};

use futures::lock::Mutex;
use std::sync::Arc;

type LimitOrderMutation = graphql_client::QueryBody<place_limit_order::Variables>;
type MarketOrderMutation = graphql_client::QueryBody<place_market_order::Variables>;
type LimitBlockchainSignatures = Vec<Option<place_limit_order::BlockchainSignature>>;
type MarketBlockchainSignatures = Vec<Option<place_market_order::BlockchainSignature>>;

impl LimitOrderRequest {
    // Buy or sell `amount` of `A` in price of `B` for an A/B market. Returns a builder struct
    // of `LimitOrderConstructor` that can be used to create smart contract and graphql payloads
    pub async fn make_constructor(
        &self,
        state: Arc<Mutex<State>>,
    ) -> Result<LimitOrderConstructor> {
        let state = state.lock().await;
        let market = state.get_market(&self.market)?;

        // Amount of order always in asset A in ME. This will handle precision conversion also...
        let amount_of_a = market.asset_a.with_amount(&self.amount)?;

        // Price is always in terms of asset B in ME
        // TODO: add precision to rate and handle this better
        let format_user_price = pad_zeros(&self.price, market.asset_b.precision)?;
        let b_per_a: Rate = OrderRate::new(&format_user_price)?.into();

        let a_per_b = b_per_a.invert_rate(None)?;

        let amount_of_b = amount_of_a.exchange_at(&b_per_a, market.asset_b)?;

        let (source, rate, destination) = match self.buy_or_sell {
            BuyOrSell::Buy => {
                // Buying: in SC, source is B, rate is B, and moving to asset A
                (amount_of_b, a_per_b, market.asset_a)
            }
            BuyOrSell::Sell => {
                // Selling: in SC, source is A, rate is A, and moving to asset B
                (amount_of_a.clone(), b_per_a.clone(), market.asset_b)
            }
        };

        Ok(LimitOrderConstructor {
            me_amount: amount_of_a,
            me_rate: b_per_a,
            market,
            buy_or_sell: self.buy_or_sell,
            cancellation_policy: self.cancellation_policy,
            allow_taker: self.allow_taker,
            source,
            destination,
            rate,
        })
    }
}

impl MarketOrderRequest {
    // Buy or sell `amount` of `A` in price of `B` for an A/B market. Returns a builder struct
    // of `LimitOrderConstructor` that can be used to create smart contract and graphql payloads
    pub async fn make_constructor(
        &self,
        state: Arc<Mutex<State>>,
    ) -> Result<MarketOrderConstructor> {
        let state = state.lock().await;

        let market = match state.get_market(&self.market) {
            Ok(market) => market,
            Err(_) => {
                let reverse_market: Vec<&str> = self.market.split('_').rev().collect();
                let reverse_market = reverse_market.join("_");
                match state.get_market(&reverse_market) {
                    Ok(market) => market.invert(),
                    Err(err) => return Err(err),
                }
            }
        };

        let source = market.asset_a.with_amount(&self.amount)?;
        let destination = market.asset_b;

        Ok(MarketOrderConstructor {
            me_amount: source.clone(),
            market,
            source,
            destination,
        })
    }
}

// If an asset is on another chain, convert it into a crosschain nonce
// FIXME: maybe Nonce should also keep track of asset type to make this easier?
fn map_crosschain(nonce: Nonce, chain: Blockchain, asset: Asset) -> Nonce {
    if asset.blockchain() == chain {
        nonce
    } else {
        Nonce::Crosschain
    }
}

impl LimitOrderConstructor {
    /// Helper to transform a limit order into signed fillorder data on every blockchain
    pub fn make_fill_order(
        &self,
        chain: Blockchain,
        pub_key: &PublicKey,
        nonces: &PayloadNonces,
    ) -> Result<FillOrder> {
        // Rate is in "dest per source", so a higher rate is always beneficial to a user
        // Here we insure the minimum rate is the rate they specified
        let min_order = self.rate.clone();
        let max_order = Rate::MaxOrderRate;
        // Amount is specified in the "source" asset
        let amount = self.source.amount.clone();

        let min_order = min_order
            .subtract_fee(Rate::MaxFeeRate.to_bigdecimal()?)?
            .into();
        let fee_rate = Rate::MinFeeRate; // 0

        match chain {
            Blockchain::Ethereum => Ok(FillOrder::Ethereum(eth::FillOrder::new(
                pub_key.to_address()?.try_into()?,
                self.source.asset.into(),
                self.destination.into(),
                map_crosschain(nonces.nonce_from, chain, self.source.asset.into()),
                map_crosschain(nonces.nonce_to, chain, self.destination.into()),
                amount,
                min_order,
                max_order,
                fee_rate,
                nonces.order_nonce,
            ))),
            Blockchain::Bitcoin => Ok(FillOrder::Bitcoin(btc::FillOrder::new(
                map_crosschain(nonces.nonce_from, chain, self.source.asset.into()),
                map_crosschain(nonces.nonce_to, chain, self.destination.into()),
            ))),
            Blockchain::NEO => {
                // FIXME: this can still be improved...
                let neo_pub_key: NeoPublicKey = pub_key.clone().try_into()?;
                let neo_order = neo::FillOrder::new(
                    neo_pub_key,
                    self.source.asset.into(),
                    self.destination.into(),
                    map_crosschain(nonces.nonce_from, chain, self.source.asset.into()),
                    map_crosschain(nonces.nonce_to, chain, self.destination.into()),
                    amount,
                    min_order,
                    max_order,
                    fee_rate,
                    nonces.order_nonce,
                );
                Ok(FillOrder::NEO(neo_order))
            }
        }
    }

    /// Create a signed blockchain payload in the format expected by GraphQL when
    /// given `nonces` and a `Client` as `signer`. FIXME: handle other chains
    pub fn blockchain_signatures(
        &self,
        signer: &mut Signer,
        nonces: &[PayloadNonces],
    ) -> Result<LimitBlockchainSignatures> {
        let mut order_payloads = Vec::new();
        let blockchains = self.market.blockchains();
        for blockchain in blockchains {
            let pub_key = signer.child_public_key(blockchain)?;
            for nonce_group in nonces {
                let fill_order = self.make_fill_order(blockchain, &pub_key, nonce_group)?;
                order_payloads.push(Some(fill_order.to_blockchain_signature(signer)?))
            }
        }
        Ok(order_payloads)
    }

    /// Create a GraphQL request with everything filled in besides blockchain order payloads
    /// and signatures (for both the overall request and blockchain payloads)
    pub fn graphql_request(
        &self,
        current_time: i64,
        affiliate: Option<String>,
    ) -> Result<place_limit_order::Variables> {
        let cancel_at = match self.cancellation_policy {
            OrderCancellationPolicy::GoodTilTime(time) => Some(format!("{:?}", time)),
            _ => None,
        };
        let order_args = place_limit_order::Variables {
            payload: place_limit_order::PlaceLimitOrderParams {
                allow_taker: self.allow_taker,
                buy_or_sell: self.buy_or_sell.into(),
                cancel_at,
                cancellation_policy: self.cancellation_policy.into(),
                market_name: self.market.market_name(),
                amount: self.me_amount.clone().try_into()?,
                // These two nonces are deprecated...
                nonce_from: 1234,
                nonce_to: 1234,
                nonce_order: (current_time as u32) as i64, // 4146194029, // Fixme: what do we validate on this?
                timestamp: current_time,
                limit_price: place_limit_order::CurrencyPriceParams {
                    // This format is confusing, but prices are always in
                    // B for an A/B market, so reverse the normal thing
                    currency_a: self.market.asset_b.asset.name().to_string(),
                    currency_b: self.market.asset_a.asset.name().to_string(),
                    amount: self.me_rate.to_bigdecimal()?.to_string(),
                },
                blockchain_signatures: vec![],
            },
            affiliate,
            signature: RequestPayloadSignature::empty().into(),
        };
        Ok(order_args)
    }

    /// Create a signed GraphQL request with blockchain payloads that can be submitted
    /// to Nash
    pub fn signed_graphql_request(
        &self,
        nonces: Vec<PayloadNonces>,
        current_time: i64,
        affiliate: Option<String>,
        signer: &mut Signer,
    ) -> Result<LimitOrderMutation> {
        let mut request = self.graphql_request(current_time, affiliate)?;
        // compute and add blockchain signatures
        let bc_sigs = self.blockchain_signatures(signer, &nonces)?;
        request.payload.blockchain_signatures = bc_sigs;
        // now compute overall request payload signature
        let canonical_string = limit_order_canonical_string(&request)?;
        let sig: place_limit_order::Signature =
            signer.sign_canonical_string(&canonical_string).into();
        request.signature = sig;
        Ok(graphql::PlaceLimitOrder::build_query(request))
    }

    // Construct payload nonces with source as `from` asset name and destination as
    // `to` asset name. Nonces will be retrieved from current values in `State`
    pub async fn make_payload_nonces(
        &self,
        state: Arc<Mutex<State>>,
        current_time: i64,
    ) -> Result<Vec<PayloadNonces>> {
        let state = state.lock().await;
        let asset_nonces = state
            .asset_nonces
            .as_ref()
            .ok_or(ProtocolError("Asset nonce map does not exist"))?;
        let (from, to) = match self.buy_or_sell {
            BuyOrSell::Buy => (
                self.market.asset_b.asset.name(),
                self.market.asset_a.asset.name(),
            ),
            BuyOrSell::Sell => (
                self.market.asset_a.asset.name(),
                self.market.asset_b.asset.name(),
            ),
        };
        let nonce_froms: Vec<Nonce> = asset_nonces
            .get(from)
            .ok_or(ProtocolError("Asset nonce for source does not exist"))?
            .iter()
            .map(|nonce| Nonce::Value(*nonce))
            .collect();
        let nonce_tos: Vec<Nonce> = asset_nonces
            .get(to)
            .ok_or(ProtocolError(
                "Asset nonce for destination a does not exist",
            ))?
            .iter()
            .map(|nonce| Nonce::Value(*nonce))
            .collect();
        let mut nonce_combinations = Vec::new();
        for nonce_from in &nonce_froms {
            for nonce_to in &nonce_tos {
                nonce_combinations.push(PayloadNonces {
                    nonce_from: *nonce_from,
                    nonce_to: *nonce_to,
                    order_nonce: Nonce::Value(current_time as u32),
                })
            }
        }
        Ok(nonce_combinations)
    }
}

impl MarketOrderConstructor {
    /// Helper to transform a limit order into signed fillorder data on every blockchain
    pub fn make_fill_order(
        &self,
        chain: Blockchain,
        pub_key: &PublicKey,
        nonces: &PayloadNonces,
    ) -> Result<FillOrder> {
        // Rate is in "dest per source", so a higher rate is always beneficial to a user
        // Here we insure the minimum rate is the rate they specified
        let min_order = Rate::MinOrderRate;
        let max_order = Rate::MaxOrderRate;
        // Amount is specified in the "source" asset
        let amount = self.source.amount.clone();
        let fee_rate = Rate::MinOrderRate; // 0

        match chain {
            Blockchain::Ethereum => Ok(FillOrder::Ethereum(eth::FillOrder::new(
                pub_key.to_address()?.try_into()?,
                self.source.asset.into(),
                self.destination.into(),
                map_crosschain(nonces.nonce_from, chain, self.source.asset.into()),
                map_crosschain(nonces.nonce_to, chain, self.destination.into()),
                amount,
                min_order,
                max_order,
                fee_rate,
                nonces.order_nonce,
            ))),
            Blockchain::Bitcoin => Ok(FillOrder::Bitcoin(btc::FillOrder::new(
                map_crosschain(nonces.nonce_from, chain, self.source.asset.into()),
                map_crosschain(nonces.nonce_to, chain, self.destination.into()),
            ))),
            Blockchain::NEO => {
                // FIXME: this can still be improved...
                let neo_pub_key: NeoPublicKey = pub_key.clone().try_into()?;
                let neo_order = neo::FillOrder::new(
                    neo_pub_key,
                    self.source.asset.into(),
                    self.destination.into(),
                    map_crosschain(nonces.nonce_from, chain, self.source.asset.into()),
                    map_crosschain(nonces.nonce_to, chain, self.destination.into()),
                    amount,
                    min_order,
                    max_order,
                    fee_rate,
                    nonces.order_nonce,
                );
                Ok(FillOrder::NEO(neo_order))
            }
        }
    }

    /// Create a signed blockchain payload in the format expected by GraphQL when
    /// given `nonces` and a `Client` as `signer`. FIXME: handle other chains
    pub fn blockchain_signatures(
        &self,
        signer: &mut Signer,
        nonces: &[PayloadNonces],
    ) -> Result<MarketBlockchainSignatures> {
        let mut order_payloads = Vec::new();
        let blockchains = self.market.blockchains();
        for blockchain in blockchains {
            let pub_key = signer.child_public_key(blockchain)?;
            for nonce_group in nonces {
                let fill_order = self.make_fill_order(blockchain, &pub_key, nonce_group)?;
                order_payloads.push(Some(fill_order.to_market_blockchain_signature(signer)?))
            }
        }
        Ok(order_payloads)
    }

    /// Create a GraphQL request with everything filled in besides blockchain order payloads
    /// and signatures (for both the overall request and blockchain payloads)
    pub fn graphql_request(
        &self,
        current_time: i64,
        affiliate: Option<String>,
    ) -> Result<place_market_order::Variables> {
        let order_args = place_market_order::Variables {
            payload: place_market_order::PlaceMarketOrderParams {
                buy_or_sell: BuyOrSell::Sell.into(),
                market_name: self.market.market_name(),
                amount: self.me_amount.clone().try_into()?,
                // These two nonces are deprecated...
                nonce_from: Some(0),
                nonce_to: Some(0),
                nonce_order: (current_time as u32) as i64, // 4146194029, // Fixme: what do we validate on this?
                timestamp: current_time,
                blockchain_signatures: vec![],
            },
            affiliate,
            signature: RequestPayloadSignature::empty().into(),
        };
        Ok(order_args)
    }

    /// Create a signed GraphQL request with blockchain payloads that can be submitted
    /// to Nash
    pub fn signed_graphql_request(
        &self,
        nonces: Vec<PayloadNonces>,
        current_time: i64,
        affiliate: Option<String>,
        signer: &mut Signer,
    ) -> Result<MarketOrderMutation> {
        let mut request = self.graphql_request(current_time, affiliate)?;
        // compute and add blockchain signatures
        let bc_sigs = self.blockchain_signatures(signer, &nonces)?;
        request.payload.blockchain_signatures = bc_sigs;
        // now compute overall request payload signature
        let canonical_string = market_order_canonical_string(&request)?;
        let sig: place_market_order::Signature =
            signer.sign_canonical_string(&canonical_string).into();
        request.signature = sig;
        Ok(graphql::PlaceMarketOrder::build_query(request))
    }

    // Construct payload nonces with source as `from` asset name and destination as
    // `to` asset name. Nonces will be retrieved from current values in `State`
    pub async fn make_payload_nonces(
        &self,
        state: Arc<Mutex<State>>,
        current_time: i64,
    ) -> Result<Vec<PayloadNonces>> {
        let state = state.lock().await;
        let asset_nonces = state
            .asset_nonces
            .as_ref()
            .ok_or(ProtocolError("Asset nonce map does not exist"))?;
        let (from, to) = (
            self.market.asset_a.asset.name(),
            self.market.asset_b.asset.name(),
        );
        let nonce_froms: Vec<Nonce> = asset_nonces
            .get(from)
            .ok_or(ProtocolError("Asset nonce for source does not exist"))?
            .iter()
            .map(|nonce| Nonce::Value(*nonce))
            .collect();
        let nonce_tos: Vec<Nonce> = asset_nonces
            .get(to)
            .ok_or(ProtocolError(
                "Asset nonce for destination a does not exist",
            ))?
            .iter()
            .map(|nonce| Nonce::Value(*nonce))
            .collect();
        let mut nonce_combinations = Vec::new();
        for nonce_from in &nonce_froms {
            for nonce_to in &nonce_tos {
                nonce_combinations.push(PayloadNonces {
                    nonce_from: *nonce_from,
                    nonce_to: *nonce_to,
                    order_nonce: Nonce::Value(current_time as u32),
                })
            }
        }
        Ok(nonce_combinations)
    }
}

pub fn limit_order_canonical_string(variables: &place_limit_order::Variables) -> Result<String> {
    let serialized_all = serde_json::to_string(variables)
        .map_err(|_| ProtocolError("Failed to serialize limit order into canonical string"))?;

    Ok(general_canonical_string(
        "place_limit_order".to_string(),
        serde_json::from_str(&serialized_all).map_err(|_| {
            ProtocolError("Failed to deserialize limit order into canonical string")
        })?,
        vec!["blockchain_signatures".to_string()],
    ))
}

pub fn market_order_canonical_string(variables: &place_market_order::Variables) -> Result<String> {
    let serialized_all = serde_json::to_string(variables)
        .map_err(|_| ProtocolError("Failed to serialize market order into canonical string"))?;

    Ok(general_canonical_string(
        "place_market_order".to_string(),
        serde_json::from_str(&serialized_all).map_err(|_| {
            ProtocolError("Failed to deserialize market order into canonical string")
        })?,
        vec!["blockchain_signatures".to_string()],
    ))
}

impl Into<place_market_order::OrderBuyOrSell> for BuyOrSell {
    fn into(self) -> place_market_order::OrderBuyOrSell {
        match self {
            BuyOrSell::Buy => place_market_order::OrderBuyOrSell::BUY,
            BuyOrSell::Sell => place_market_order::OrderBuyOrSell::SELL,
        }
    }
}

impl From<RequestPayloadSignature> for place_market_order::Signature {
    fn from(sig: RequestPayloadSignature) -> Self {
        place_market_order::Signature {
            signed_digest: sig.signed_digest,
            public_key: sig.public_key,
        }
    }
}

impl TryInto<place_market_order::CurrencyAmountParams> for AssetAmount {
    type Error = ProtocolError;
    fn try_into(self) -> Result<place_market_order::CurrencyAmountParams> {
        Ok(place_market_order::CurrencyAmountParams {
            amount: pad_zeros(
                &self.amount.to_bigdecimal().to_string(),
                self.amount.precision,
            )?,
            // FIXME: asset.asset is ugly
            currency: self.asset.asset.name().to_string(),
        })
    }
}

impl Into<place_limit_order::OrderBuyOrSell> for BuyOrSell {
    fn into(self) -> place_limit_order::OrderBuyOrSell {
        match self {
            BuyOrSell::Buy => place_limit_order::OrderBuyOrSell::BUY,
            BuyOrSell::Sell => place_limit_order::OrderBuyOrSell::SELL,
        }
    }
}

impl From<RequestPayloadSignature> for place_limit_order::Signature {
    fn from(sig: RequestPayloadSignature) -> Self {
        place_limit_order::Signature {
            signed_digest: sig.signed_digest,
            public_key: sig.public_key,
        }
    }
}

impl From<OrderCancellationPolicy> for place_limit_order::OrderCancellationPolicy {
    fn from(policy: OrderCancellationPolicy) -> Self {
        match policy {
            OrderCancellationPolicy::FillOrKill => {
                place_limit_order::OrderCancellationPolicy::FILL_OR_KILL
            }
            OrderCancellationPolicy::GoodTilCancelled => {
                place_limit_order::OrderCancellationPolicy::GOOD_TIL_CANCELLED
            }
            OrderCancellationPolicy::GoodTilTime(_) => {
                place_limit_order::OrderCancellationPolicy::GOOD_TIL_TIME
            }
            OrderCancellationPolicy::ImmediateOrCancel => {
                place_limit_order::OrderCancellationPolicy::IMMEDIATE_OR_CANCEL
            }
        }
    }
}

impl TryInto<place_limit_order::CurrencyAmountParams> for AssetAmount {
    type Error = ProtocolError;
    fn try_into(self) -> Result<place_limit_order::CurrencyAmountParams> {
        Ok(place_limit_order::CurrencyAmountParams {
            amount: pad_zeros(
                &self.amount.to_bigdecimal().to_string(),
                self.amount.precision,
            )?,
            // FIXME: asset.asset is ugly
            currency: self.asset.asset.name().to_string(),
        })
    }
}
