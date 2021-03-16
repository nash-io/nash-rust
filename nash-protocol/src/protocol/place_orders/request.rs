use crate::errors::{ProtocolError, Result};
use crate::graphql;
use crate::graphql::place_limit_order;
use crate::graphql::place_market_order;
use crate::types::neo::PublicKey as NeoPublicKey;
use crate::types::PublicKey;
use crate::types::{
    Asset, Blockchain, BuyOrSell, Nonce, OrderCancellationPolicy, Rate
};
use graphql_client::GraphQLQuery;
use std::convert::TryInto;

use super::super::signer::Signer;
use super::super::{general_canonical_string, RequestPayloadSignature, State};
use crate::protocol::place_order::blockchain::{btc, eth, neo, FillOrder};
use super::types::{
    LimitOrdersConstructor, LimitOrdersRequest,
    MarketOrdersConstructor, MarketOrdersRequest
};

use tokio::sync::RwLock;
use std::sync::Arc;

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::protocol::place_order::types::PayloadNonces;
use crate::protocol::place_orders::types::OrdersRequest;

/// The form in which queries are sent over HTTP in most implementations. This will be built using the [`GraphQLQuery`] trait normally.
#[derive(Debug, Serialize, Deserialize)]
pub struct MultiQueryBody {
    /// The values for the variables. They must match those declared in the queries. This should be the `Variables` struct from the generated module corresponding to the query.
    pub variables: HashMap<String, serde_json::Value>,
    /// The GraphQL query, as a string.
    pub query: String,
    /// The GraphQL operation name, as a string.
    #[serde(rename = "operationName")]
    pub operation_name: &'static str,
}

type LimitOrdersMutation = MultiQueryBody;
type MarketOrdersMutation = MultiQueryBody;

impl LimitOrdersRequest {
    // Buy or sell `amount` of `A` in price of `B` for an A/B market. Returns a builder struct
    // of `LimitOrderConstructor` that can be used to create smart contract and graphql payloads
    pub async fn make_constructor(&self, state: Arc<RwLock<State>>) -> Result<LimitOrdersConstructor> {
        let mut constructors = Vec::new();
        for request in &self.requests {
            constructors.push(request.make_constructor(state.clone()).await?);
        }
        Ok(LimitOrdersConstructor { constructors })
    }
}

impl MarketOrdersRequest {
    // Buy or sell `amount` of `A` in price of `B` for an A/B market. Returns a builder struct
    // of `MarketOrderConstructor` that can be used to create smart contract and graphql payloads
    pub async fn make_constructor(&self, state: Arc<RwLock<State>>) -> Result<MarketOrdersConstructor> {
        let mut constructors = Vec::new();
        for request in &self.requests {
            constructors.push(request.make_constructor(state.clone()).await?);
        }
        Ok(MarketOrdersConstructor { constructors })
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

impl LimitOrdersConstructor {
    /// Create a GraphQL request with everything filled in besides blockchain order payloads
    /// and signatures (for both the overall request and blockchain payloads)
    pub fn graphql_request(
        &self,
        current_time: i64,
        affiliate: Option<String>,
    ) -> Result<Vec<place_limit_order::Variables>> {
        let mut result = Vec::new();
        for (index, request) in self.constructors.iter().enumerate() {
            let cancel_at = match request.cancellation_policy {
                OrderCancellationPolicy::GoodTilTime(time) => Some(format!("{:?}", time)),
                _ => None,
            };
            result.push(place_limit_order::Variables {
                payload: place_limit_order::PlaceLimitOrderParams {
                    client_order_id: request.client_order_id.clone(),
                    allow_taker: request.allow_taker,
                    buy_or_sell: request.buy_or_sell.into(),
                    cancel_at,
                    cancellation_policy: request.cancellation_policy.into(),
                    market_name: request.market.market_name(),
                    amount: request.me_amount.clone().try_into()?,
                    // These two nonces are deprecated...
                    nonce_from: 1234,
                    nonce_to: 1234,
                    nonce_order: (current_time as u32) as i64 + index as i64, // 4146194029, // Fixme: what do we validate on this?
                    timestamp: current_time,
                    limit_price: place_limit_order::CurrencyPriceParams {
                        // This format is confusing, but prices are always in
                        // B for an A/B market, so reverse the normal thing
                        currency_a: request.market.asset_b.asset.name().to_string(),
                        currency_b: request.market.asset_a.asset.name().to_string(),
                        amount: request.me_rate.to_bigdecimal()?.to_string(),
                    },
                    blockchain_signatures: vec![],
                },
                affiliate: affiliate.clone(),
                signature: RequestPayloadSignature::empty().into(),
            });
        }
        Ok(result)
    }

    /// Create a signed GraphQL request with blockchain payloads that can be submitted
    /// to Nash
    pub async fn signed_graphql_request(
        &self,
        current_time: i64,
        affiliate: Option<String>,
        state: Arc<RwLock<State>>,
    ) -> Result<LimitOrdersMutation> {
        let variables = self.graphql_request(current_time, affiliate)?;
        let mut map = HashMap::new();
        let mut params = String::new();
        let mut calls = String::new();
        for (index, (mut variable, constructor)) in variables.into_iter().zip(self.constructors.iter()).enumerate() {
            // FIXME: This current_time + index for nonces is replicated in graphql_request. We would benefit to abstract this logic somewhere.
            let nonces = constructor.make_payload_nonces(state.clone(), current_time + index as i64).await?;
            let state = state.read().await;
            let signer = state.signer()?;
            // compute and add blockchain signatures
            let bc_sigs = constructor.blockchain_signatures(signer, &nonces)?;
            variable.payload.blockchain_signatures = bc_sigs;
            // now compute overall request payload signature
            let canonical_string = limit_order_canonical_string(&variable)?;
            let sig: place_limit_order::Signature =
                signer.sign_canonical_string(&canonical_string).into();
            variable.signature = sig;

            let payload = format!("payload{}", index);
            let signature = format!("signature{}", index);
            let affiliate = format!("affiliate{}", index);
            params = if index == 0 { params } else { format!("{}, ", params)};
            params = format!("{}${}: PlaceLimitOrderParams!, ${}: Signature!, ${}: AffiliateDeveloperCode", params, payload, signature, affiliate);
            calls = format!(r#"
                {}
                response{}: placeLimitOrder(payload: ${}, signature: ${}, affiliateDeveloperCode: ${}) {{
                    id
                    status
                    ordersTillSignState,
                    buyOrSell,
                    market {{
                        name
                    }},
                    placedAt,
                    type
                }}
                "#, calls, index, payload, signature, affiliate);
            map.insert(payload, serde_json::to_value(variable.payload).unwrap());
            map.insert(signature, serde_json::to_value(variable.signature).unwrap());
            map.insert(affiliate, serde_json::to_value(variable.affiliate).unwrap());
        }
        Ok(LimitOrdersMutation {
            variables: map,
            operation_name: "PlaceLimitOrder",
            query: format!(r#"
                mutation PlaceLimitOrder({}) {{
                    {}
                }}
            "#, params, calls)
        })
    }
}

impl MarketOrdersConstructor {
    /// Create a GraphQL request with everything filled in besides blockchain order payloads
    /// and signatures (for both the overall request and blockchain payloads)
    pub fn graphql_request(
        &self,
        current_time: i64,
        affiliate: Option<String>,
    ) -> Result<Vec<place_market_order::Variables>> {
        let mut result = Vec::new();
        for (index, request) in self.constructors.iter().enumerate() {
            result.push(place_market_order::Variables {
                payload: place_market_order::PlaceMarketOrderParams {
                    buy_or_sell: BuyOrSell::Sell.into(),
                    client_order_id: request.client_order_id.clone(),
                    market_name: request.market.market_name(),
                    amount: request.me_amount.clone().try_into()?,
                    // These two nonces are deprecated...
                    nonce_from: Some(0),
                    nonce_to: Some(0),
                    nonce_order: (current_time as u32) as i64 + index as i64, // 4146194029, // Fixme: what do we validate on this?
                    timestamp: current_time,
                    blockchain_signatures: vec![],
                },
                affiliate: affiliate.clone(),
                signature: RequestPayloadSignature::empty().into(),
            });
        }
        Ok(result)
    }

    /// Create a signed GraphQL request with blockchain payloads that can be submitted
    /// to Nash
    pub async fn signed_graphql_request(
        &self,
        current_time: i64,
        affiliate: Option<String>,
        state: Arc<RwLock<State>>,
    ) -> Result<MarketOrdersMutation> {
        let variables = self.graphql_request(current_time, affiliate)?;
        let mut map = HashMap::new();
        let mut params = String::new();
        let mut calls = String::new();
        for (index, (mut variable, constructor)) in variables.into_iter().zip(self.constructors.iter()).enumerate() {
            // FIXME: This current_time + index for nonces is replicated in graphql_request. We would benefit to abstract this logic somewhere.
            let nonces = constructor.make_payload_nonces(state.clone(), current_time + index as i64).await?;
            let state = state.read().await;
            let signer = state.signer()?;
            // compute and add blockchain signatures
            let bc_sigs = constructor.blockchain_signatures(signer, &nonces)?;
            variable.payload.blockchain_signatures = bc_sigs;
            // now compute overall request payload signature
            let canonical_string = market_order_canonical_string(&variable)?;
            let sig: place_market_order::Signature =
                signer.sign_canonical_string(&canonical_string).into();
            variable.signature = sig;

            let payload = format!("payload{}", index);
            let signature = format!("signature{}", index);
            let affiliate = format!("affiliate{}", index);
            params = if index == 0 { params } else { format!("{}, ", params)};
            params = format!("{}${}: PlaceMarketOrderParams!, ${}: Signature!, ${}: AffiliateDeveloperCode", params, payload, signature, affiliate);
            calls = format!(r#"
                {}
                response{}: placeMarketOrder(payload: ${}, signature: ${}, affiliateDeveloperCode: ${}) {{
                    id
                    status
                    ordersTillSignState,
                    buyOrSell,
                    market {{
                        name
                    }},
                    placedAt,
                    type
                }}
                "#, calls, index, payload, signature, affiliate);
            map.insert(payload, serde_json::to_value(variable.payload).unwrap());
            map.insert(signature, serde_json::to_value(variable.signature).unwrap());
            map.insert(affiliate, serde_json::to_value(variable.affiliate).unwrap());
        }
        Ok(MarketOrdersMutation {
            variables: map,
            operation_name: "PlaceMarketOrder",
            query: format!(r#"
                mutation PlaceMarketOrder({}) {{
                    {}
                }}
            "#, params, calls)
        })
    }
}

pub fn limit_order_canonical_string(variables: &place_limit_order::Variables) -> Result<String> {
    let serialized_all = serde_json::to_string(variables).map_err(|_|ProtocolError("Failed to serialize limit order into canonical string"))?;

    Ok(general_canonical_string(
        "place_limit_order".to_string(),
        serde_json::from_str(&serialized_all).map_err(|_|ProtocolError("Failed to deserialize limit order into canonical string"))?,
        vec!["blockchain_signatures".to_string()],
    ))
}

pub fn market_order_canonical_string(variables: &place_market_order::Variables) -> Result<String> {
    let serialized_all = serde_json::to_string(variables).map_err(|_|ProtocolError("Failed to serialize market order into canonical string"))?;

    Ok(general_canonical_string(
        "place_market_order".to_string(),
        serde_json::from_str(&serialized_all).map_err(|_|ProtocolError("Failed to deserialize market order into canonical string"))?,
        vec!["blockchain_signatures".to_string()],
    ))
}

