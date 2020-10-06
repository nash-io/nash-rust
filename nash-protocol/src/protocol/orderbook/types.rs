use crate::errors::Result;
use crate::types::{Market, OrderbookOrder};

use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

use super::super::{
    json_to_type_or_error, serializable_to_json, NashProtocol, ResponseOrError, State,
};

/// Get order book data for the provided market
#[derive(Clone, Debug)]
pub struct OrderbookRequest {
    pub market: Market,
}

/// An order book is a list of bid and ask orders
#[derive(Debug)]
pub struct OrderbookResponse {
    pub asks: Vec<OrderbookOrder>,
    pub bids: Vec<OrderbookOrder>,
}

#[async_trait]
impl NashProtocol for OrderbookRequest {
    type Response = OrderbookResponse;

    async fn graphql(&self, _state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        serializable_to_json(&query)
    }

    fn response_from_json(
        &self,
        response: serde_json::Value,
    ) -> Result<ResponseOrError<Self::Response>> {
        let as_graphql = json_to_type_or_error(response)?;
        self.response_from_graphql(as_graphql)
    }
}
