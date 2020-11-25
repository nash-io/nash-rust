use super::types::ListAccountBalancesResponse;
use crate::graphql::list_account_balances;
use crate::types::Asset;

use std::collections::HashMap;

impl From<list_account_balances::ResponseData> for ListAccountBalancesResponse {
    fn from(response: list_account_balances::ResponseData) -> Self {
        let balance_list = response.list_account_balances;
        let mut state_channel = HashMap::new();
        let mut pending = HashMap::new();
        let mut personal = HashMap::new();
        let mut in_orders = HashMap::new();
        // These unwraps are safe. ME_FIXME
        for balance in balance_list {
            let symbol = balance.asset.unwrap().symbol;
            if let Ok(asset) = Asset::from_str(&symbol) {
                let state_channel_amount = asset
                    .with_precision(8)
                    .with_amount(&balance.available.unwrap().amount)
                    .unwrap();
                state_channel.insert(asset, state_channel_amount);
                let pending_amount = asset
                    .with_precision(8)
                    .with_amount(&balance.pending.unwrap().amount)
                    .unwrap();
                pending.insert(asset, pending_amount);
                let personal_amount = asset
                    .with_precision(8)
                    .with_amount(&balance.personal.unwrap().amount)
                    .unwrap();
                personal.insert(asset, personal_amount);
                let in_order_amount = asset
                    .with_precision(8)
                    .with_amount(&balance.in_orders.unwrap().amount)
                    .unwrap();
                in_orders.insert(asset, in_order_amount);
            }
        }
        Self {
            state_channel,
            personal,
            pending,
            in_orders
        }
    }
}
