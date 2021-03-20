use crate::errors::Result;
use crate::graphql::place_limit_order;
use crate::graphql::place_market_order;
use super::super::State;
use super::types::{
    LimitOrdersConstructor, LimitOrdersRequest,
    MarketOrdersConstructor, MarketOrdersRequest
};

use tokio::sync::RwLock;
use std::sync::Arc;

use std::collections::HashMap;
use crate::protocol::multi_request::DynamicQueryBody;

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

impl LimitOrdersConstructor {
    /// Create a GraphQL request with everything filled in besides blockchain order payloads
    /// and signatures (for both the overall request and blockchain payloads)
    pub fn graphql_request(
        &self,
        current_time: i64,
        affiliate: Option<String>,
    ) -> Result<Vec<place_limit_order::Variables>> {
        let mut variables = Vec::new();
        for (index, variable) in self.constructors.iter().enumerate() {
            variables.push(variable.graphql_request(current_time + index as i64, affiliate.clone())?);
        }
        Ok(variables)
    }

    /// Create a signed GraphQL request with blockchain payloads that can be submitted
    /// to Nash
    pub async fn signed_graphql_request(
        &self,
        current_time: i64,
        affiliate: Option<String>,
        state: Arc<RwLock<State>>,
    ) -> Result<DynamicQueryBody> {
        let variables = self.graphql_request(current_time, affiliate)?;
        let mut map = HashMap::new();
        let mut params = String::new();
        let mut calls = String::new();
        for (index, (variable, constructor)) in variables.into_iter().zip(self.constructors.iter()).enumerate() {
            // FIXME: This current_time + index for nonces is replicated in graphql_request. We would benefit to abstract this logic somewhere.
            let nonces = constructor.make_payload_nonces(state.clone(), current_time + index as i64).await?;
            let state = state.read().await;
            let signer = state.signer()?;
            let variable = constructor.sign_graphql_request(variable, nonces, signer)?;

            // FIXME: This is also replicated in MarketOrdersConstructor::signed_graphql_request
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
        Ok(DynamicQueryBody {
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
        let mut variables = Vec::new();
        for (index, variable) in self.constructors.iter().enumerate() {
            variables.push(variable.graphql_request(current_time + index as i64, affiliate.clone())?);
        }
        Ok(variables)
    }

    /// Create a signed GraphQL request with blockchain payloads that can be submitted
    /// to Nash
    pub async fn signed_graphql_request(
        &self,
        current_time: i64,
        affiliate: Option<String>,
        state: Arc<RwLock<State>>,
    ) -> Result<DynamicQueryBody> {
        let variables = self.graphql_request(current_time, affiliate)?;
        let mut map = HashMap::new();
        let mut params = String::new();
        let mut calls = String::new();
        for (index, (variable, constructor)) in variables.into_iter().zip(self.constructors.iter()).enumerate() {
            // FIXME: This current_time + index for nonces is replicated in graphql_request. We would benefit to abstract this logic somewhere.
            let nonces = constructor.make_payload_nonces(state.clone(), current_time + index as i64).await?;
            let state = state.read().await;
            let signer = state.signer()?;
            let variable = constructor.sign_graphql_request(variable, nonces, signer)?;

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
        Ok(DynamicQueryBody {
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
