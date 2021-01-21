use super::super::super::ResponseOrError;
use super::request::SubscribeAccountBalances;
use crate::errors::Result;
use crate::graphql;
use graphql::updated_account_balances;
use crate::types::Asset;
use bigdecimal::BigDecimal;
use std::str::FromStr;
use std::collections::HashMap;
use crate::protocol::traits::TryFromState;
use crate::protocol::state::State;
use std::sync::Arc;
use tokio::sync::RwLock;
use async_trait::async_trait;

/// List of new incoming trades for a market via subscription.
#[derive(Clone, Debug)]
pub struct AccountBalancesResponse {
    pub balances: HashMap<Asset, BigDecimal>,
}

#[async_trait]
impl TryFromState<updated_account_balances::ResponseData> for HashMap<Asset, BigDecimal> {
    async fn from(response: updated_account_balances::ResponseData, _state: Arc<RwLock<State>>) -> Result<HashMap<Asset, BigDecimal>> {
        let mut balances = HashMap::new();
        for balance in response.updated_account_balances {
            let symbol = balance.asset.unwrap().symbol;
            if let Ok(asset) = Asset::from_str(&symbol) {
                let state_channel_amount = BigDecimal::from_str(&balance.available.unwrap().amount).unwrap();
                balances.insert(asset, state_channel_amount);
            }
        }

        Ok(balances)
    }
}

impl SubscribeAccountBalances {
    pub async fn response_from_graphql(
        &self,
        response: ResponseOrError<updated_account_balances::ResponseData>,
        state: Arc<RwLock<State>>
    ) -> Result<ResponseOrError<AccountBalancesResponse>> {
        Ok(match response {
            ResponseOrError::Response(data) => {
                let response = data.data;
                ResponseOrError::from_data(AccountBalancesResponse {
                    balances: TryFromState::from(response, state).await?,
                })
            }
            ResponseOrError::Error(error) => ResponseOrError::Error(error),
        })
    }
}