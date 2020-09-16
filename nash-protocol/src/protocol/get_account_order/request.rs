use super::types::GetAccountOrderRequest;
use crate::graphql;
use crate::graphql::get_account_order;

use graphql_client::GraphQLQuery;

impl GetAccountOrderRequest {
    pub fn make_query(&self) -> graphql_client::QueryBody<get_account_order::Variables> {
        let get_order = get_account_order::Variables {
            payload: get_account_order::GetAccountOrderParams {
                order_id: self.order_id.clone(),
            },
        };
        graphql::GetAccountOrder::build_query(get_order)
    }
}
