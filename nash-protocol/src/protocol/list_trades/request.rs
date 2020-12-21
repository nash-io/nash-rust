use super::types::ListTradesRequest;
use crate::graphql;
use crate::graphql::list_trades;
use graphql_client::GraphQLQuery;

impl ListTradesRequest {
    pub fn make_query(&self) -> graphql_client::QueryBody<list_trades::Variables> {
        graphql::ListTrades::build_query(list_trades::Variables {
            market_name: self.market.clone(),
            limit: self.limit,
            before: self.before.clone(),
        })
    }
}
