use super::types::TickerRequest;
use crate::graphql;
use crate::graphql::get_ticker;
use graphql_client::GraphQLQuery;

impl TickerRequest {
    pub fn make_query(&self) -> graphql_client::QueryBody<get_ticker::Variables> {
        graphql::GetTicker::build_query(get_ticker::Variables {
            market_name: self.market.market_name(),
        })
    }
}
