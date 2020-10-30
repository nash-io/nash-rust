use super::super::{
    serializable_to_json, try_response_from_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::list_markets;
use crate::types::Market;
use async_trait::async_trait;
use futures::lock::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Request to list markets running on Nash.
#[derive(Clone, Debug)]
pub struct ListMarketsRequest;

/// Contains a HashMap that connects market symbol to `Market`, which tracks precision data.
#[derive(Clone, Debug)]
pub struct ListMarketsResponse {
    pub markets: HashMap<String, Market>,
}

#[async_trait]
impl NashProtocol for ListMarketsRequest {
    type Response = ListMarketsResponse;

    async fn graphql(&self, _state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        let mut out = serializable_to_json(&query)?;
        // override null with an empty object because ME is weird
        *out.get_mut("variables").unwrap() = serde_json::json!({});
        Ok(out)
    }

    async fn process_response(
        &self,
        response: &Self::Response,
        state: Arc<Mutex<State>>,
    ) -> Result<()> {
        let mut state = state.lock().await;
        let markets: Vec<Market> = response.markets.iter().map(|(_k, v)| v.clone()).collect();
        let mut assets = HashSet::new();
        let mut market_map = HashMap::new();
        for market in &markets {
            assets.insert(market.asset_a.asset);
            assets.insert(market.asset_b.asset);
            market_map.insert(market.market_name(), market.clone());
        }
        // store market and asset list in the client
        state.markets = Some(market_map);
        state.assets = Some(assets.into_iter().collect());
        Ok(())
    }

    fn response_from_json(
        &self,
        response: serde_json::Value,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<ListMarketsResponse, list_markets::ResponseData>(response)
    }
}
