use crate::graphql;
use graphql::updated_account_balances;
use graphql_client::GraphQLQuery;

/// Initiate subscription to new account *trading* balances
#[derive(Clone, Debug)]
pub struct SubscribeAccountBalances {
    pub symbol: Option<String>,
}

impl SubscribeAccountBalances {
    pub fn make_query(&self) -> graphql_client::QueryBody<updated_account_balances::Variables> {
        graphql::UpdatedAccountBalances::build_query(updated_account_balances::Variables {
            payload: updated_account_balances::UpdatedAccountBalancesParams {
                currency: self.symbol.clone(),
            }
        })
    }
}
