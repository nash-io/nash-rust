use super::super::{
    serializable_to_json, try_response_from_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::list_candles;
use crate::types::{Candle, CandleInterval, DateTimeRange, Market};
use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

/// Get candles associated with market, filtering on several optional fields.
#[derive(Clone, Debug)]
pub struct ListCandlesRequest {
    pub market: Market,
    /// page before if using pagination
    pub before: Option<String>,
    pub chronological: Option<bool>,
    /// what kind of candle interval do we want?
    pub interval: Option<CandleInterval>,
    /// max trades to return
    pub limit: Option<i64>,
    /// range of time to get candles
    pub range: Option<DateTimeRange>,
}

/// List of candles as well as an optional link to the next page of data.
#[derive(Debug)]
pub struct ListCandlesResponse {
    pub candles: Vec<Candle>,
    pub next_page: Option<String>,
}

#[async_trait]
impl NashProtocol for ListCandlesRequest {
    type Response = ListCandlesResponse;

    async fn graphql(&self, _state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        let mut serde_value = serializable_to_json(&query)?;
        if self.interval == None {
            // hack to remove interval field if it is null
            let variables = serde_value
                .as_object_mut()
                .unwrap()
                .get_mut("variables")
                .unwrap();
            variables.as_object_mut().unwrap().remove("interval");
        }
        Ok(serde_value)
    }

    fn response_from_json(
        &self,
        response: serde_json::Value,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<ListCandlesResponse, list_candles::ResponseData>(response)
    }
}
