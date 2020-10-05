use super::super::{
    serializable_to_json, try_response_from_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::list_account_trades;
use crate::types::{DateTimeRange, Market, Trade};

use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ListAccountTradesRequest {
    /// FIXME: this is a required field because of a backend bug, it should be optional
    pub market: Market,
    /// page before if using pagination
    pub before: Option<String>,
    /// max trades to return
    pub limit: Option<i64>,
    pub range: Option<DateTimeRange>,
}

#[derive(Debug)]
pub struct ListAccountTradesResponse {
    pub trades: Vec<Trade>,
    pub next_page: Option<String>,
}

#[async_trait]
impl NashProtocol for ListAccountTradesRequest {
    type Response = ListAccountTradesResponse;

    async fn graphql(&self, _state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        serializable_to_json(&query)
    }

    fn response_from_json(
        &self,
        response: serde_json::Value,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<ListAccountTradesResponse, list_account_trades::ResponseData>(
            response,
        )
    }
}
