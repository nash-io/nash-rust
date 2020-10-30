use super::types::OrderbookRequest;
use crate::graphql;
use crate::graphql::get_orderbook;
use graphql_client::GraphQLQuery;

impl OrderbookRequest {
    pub fn make_query(&self) -> graphql_client::QueryBody<get_orderbook::Variables> {
        graphql::GetOrderbook::build_query(get_orderbook::Variables {
            market_name: self.market.to_string()
        })
    }
}
