use super::super::{
    serializable_to_json, try_response_from_json, NashProtocol, ResponseOrError, State,
};
use crate::errors::Result;
use crate::graphql::list_account_balances;
use crate::types::{Asset};
use bigdecimal::BigDecimal;
use async_trait::async_trait;
use futures::lock::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

/// List account balances. Optionally filter by asset symbol string
#[derive(Clone, Debug)]
pub struct ListAccountBalancesRequest {
    pub filter: Option<String>,
}

/// Account balances in personal wallet, pending transactions to state channel,
/// and funds at rest in the state channel.
#[derive(Clone, Debug)]
pub struct ListAccountBalancesResponse {
    // Funds available in state channel
    pub state_channel: HashMap<Asset, BigDecimal>,
    // Funds pending in state channel
    pub pending: HashMap<Asset, BigDecimal>,
    // Funds in personal wallet
    pub personal: HashMap<Asset, BigDecimal>,
    // Funds in current orders
    pub in_orders: HashMap<Asset, BigDecimal>
}

#[async_trait]
impl NashProtocol for ListAccountBalancesRequest {
    type Response = ListAccountBalancesResponse;

    async fn graphql(&self, _state: Arc<Mutex<State>>) -> Result<serde_json::Value> {
        let query = self.make_query();
        serializable_to_json(&query)
    }

    async fn response_from_json(
        &self,
        response: serde_json::Value,
        _state: Arc<Mutex<State>>
    ) -> Result<ResponseOrError<Self::Response>> {
        try_response_from_json::<ListAccountBalancesResponse, list_account_balances::ResponseData>(
            response,
        )
    }
}
