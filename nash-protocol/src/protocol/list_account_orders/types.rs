use super::super::hooks::{NashProtocolRequest, ProtocolHook};
use super::super::list_markets::ListMarketsRequest;
use super::super::{
    serializable_to_json, try_response_with_state_from_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::list_account_orders;
use crate::types::{BuyOrSell, DateTimeRange, Order, OrderStatus, OrderType};
use async_trait::async_trait;
use futures::lock::Mutex;
use std::sync::Arc;

/// List orders associated with current account session filtered by several optional fields.
/// ```
/// use nash_protocol::protocol::list_account_orders::ListAccountOrdersRequest;
/// use nash_protocol::types::{Market, BuyOrSell, OrderStatus, OrderType, DateTimeRange};
/// use chrono::{DateTime, Utc, TimeZone};
/// let request = ListAccountOrdersRequest {
///   market: Market::eth_usdc(), // require for now, this needs to be fixed on backend
///   before: None, // used for paging
///   buy_or_sell: Some(BuyOrSell::Buy), // just buy orders
///   limit: Some(10), // just return 10 orders
///   status: Some(vec![OrderStatus::Filled]), // only filled orders
///   order_type: Some(vec![OrderType::Limit]), // only limit orders
///   range: Some(DateTimeRange {
///     start: Utc.ymd(2020, 9, 12).and_hms(0, 0, 0), // after 9/12/2020
///     stop: Utc.ymd(2020, 9, 16).and_hms(0, 10, 0), // before 9/16/2020
///   })
/// };
/// ```
#[derive(Clone, Debug)]
pub struct ListAccountOrdersRequest {
    pub market: Option<String>,
    /// page before if using pagination
    pub before: Option<String>,
    pub buy_or_sell: Option<BuyOrSell>,
    /// max orders to return
    pub limit: Option<i64>,
    pub status: Option<Vec<OrderStatus>>,
    pub order_type: Option<Vec<OrderType>>,
    pub range: Option<DateTimeRange>,
}

/// List of orders that meet critera of the request. Includes an optional paging field
/// if more orders exist.
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

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        state: Arc<Mutex<State>>,
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_with_state_from_json::<
            ListAccountOrdersResponse,
            list_account_orders::ResponseData,
        >(response, state)
        .await
    }

    async fn run_before(&self, state: Arc<Mutex<State>>) -> Result<Option<Vec<ProtocolHook>>> {
        let state = state.lock().await;
        let mut hooks = Vec::new();
        if let None = state.markets {
            hooks.push(ProtocolHook::Protocol(NashProtocolRequest::ListMarkets(
                ListMarketsRequest,
            )))
        }
        Ok(Some(hooks))
    }
}
