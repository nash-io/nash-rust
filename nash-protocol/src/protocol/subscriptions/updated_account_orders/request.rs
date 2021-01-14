use crate::graphql;
use graphql::updated_account_orders;
use graphql_client::GraphQLQuery;

/// Initiate subscription to get new orders for an account
#[derive(Clone, Debug)]
pub struct SubscribeAccountOrders {
    pub market: Option<String>,
}

impl SubscribeAccountOrders {
    pub fn make_query(&self) -> graphql_client::QueryBody<updated_account_orders::Variables> {
        graphql::UpdatedAccountOrders::build_query(updated_account_orders::Variables {
            payload: updated_account_orders::UpdatedAccountOrdersParams {
                market_name: self.market.clone(),
                buy_or_sell: None,
                range_start: None,
                range_stop: None,
                status: None,
                type_: None
            }
        })
    }
}
