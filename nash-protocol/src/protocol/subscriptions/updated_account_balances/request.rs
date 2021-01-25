use crate::graphql;
use graphql::updated_account_balances;
use graphql_client::GraphQLQuery;

/// Initiate subscription to new account *trading* balances
#[derive(Clone, Debug)]
pub struct SubscribeAccountBalances {
    // FIXME: This should be an Option, but because of a bug, if the value is None, the request will fail.
    pub symbol: String,
}

impl SubscribeAccountBalances {
    pub fn make_query(&self) -> graphql_client::QueryBody<updated_account_balances::Variables> {
        graphql::UpdatedAccountBalances::build_query(updated_account_balances::Variables {
            payload: updated_account_balances::UpdatedAccountBalancesParams {
                currency: Some(self.symbol.clone()),
            }
        })
    }
}
