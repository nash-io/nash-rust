use super::types::ListAccountBalancesRequest;
use crate::graphql;
use crate::graphql::list_account_balances;

use graphql_client::GraphQLQuery;

impl ListAccountBalancesRequest {
    pub fn make_query(&self) -> graphql_client::QueryBody<list_account_balances::Variables> {
        let list_balances = list_account_balances::Variables {
            payload: list_account_balances::ListAccountBalancesParams {
                ignore_low_balance: true,
                filter: None,
            },
        };
        graphql::ListAccountBalances::build_query(list_balances)
    }
}
