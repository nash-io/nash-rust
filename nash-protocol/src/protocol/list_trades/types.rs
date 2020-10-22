use std::sync::Arc;
use async_trait::async_trait;
use futures::lock::Mutex;
use crate::errors::Result;
use crate::graphql::list_trades;
use crate::types::{Market, Trade};
use super::super::{
    NashProtocol, ResponseOrError, serializable_to_json, State, try_response_from_json,
};

/// Get trades associated with market, filtering on several optional fields.
#[derive(Clone, Debug)]
pub struct ListTradesRequest {
    pub market: Market,
    /// max trades to return
    pub limit: Option<i64>,
    /// page before if using pagination
    pub before: Option<String>,
}

/// List of trades as well as an optional link to the next page of data.
#[derive(Debug)]
pub struct ListTradesResponse {
    pub trades: Vec<Trade>,
    pub next_page: Option<String>,
}

#[async_trait]
impl NashProtocol for ListTradesRequest {
    type Response = ListTradesResponse;

    async fn graphql(&self, _state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        serializable_to_json(&query)
    }

    fn response_from_json(
        &self,
        response: serde_json::Value,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<ListTradesResponse, list_trades::ResponseData>(response)
    }
}
