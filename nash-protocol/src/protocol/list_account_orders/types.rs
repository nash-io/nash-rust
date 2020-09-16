use super::super::{serializable_to_json, NashProtocol, ResponseOrError, State, try_response_from_json};
use crate::graphql::list_account_orders;
use crate::errors::Result;
use crate::types::{BuyOrSell, DateTimeRange, Market, Order, OrderStatus, OrderType};

use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ListAccountOrdersRequest {
    /// FIXME: this is a required field because of a backend bug, it should be optional
    pub market: Market,
    /// page before if using pagination
    pub before: Option<String>,
    pub buy_or_sell: Option<BuyOrSell>,
    /// max orders to return
    pub limit: Option<i64>,
    pub status: Option<Vec<OrderStatus>>,
    pub order_type: Option<Vec<OrderType>>,
    pub range: Option<DateTimeRange>,
}

#[derive(Debug)]
pub struct ListAccountOrdersResponse {
    pub orders: Vec<Order>,
    pub next_page: Option<String>,
}

#[async_trait]
impl NashProtocol for ListAccountOrdersRequest {
    type Response = ListAccountOrdersResponse;

    async fn graphql(&self, _state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        serializable_to_json(&query)
    }

    fn response_from_json(
        &self,
        response: serde_json::Value,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<ListAccountOrdersResponse, list_account_orders::ResponseData>(response)
    }
}
