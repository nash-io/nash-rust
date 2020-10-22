use graphql_client::GraphQLQuery;
use crate::graphql;
use crate::graphql::list_trades;
use super::types::ListTradesRequest;

impl ListTradesRequest {
    pub fn make_query(&self) -> graphql_client::QueryBody<list_trades::Variables> {
        graphql::ListTrades::build_query(list_trades::Variables {
            market_name: self.market.market_name(),
            limit: self.limit,
            before: self.before.clone(),
        })
    }
}
