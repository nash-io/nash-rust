use super::types::ListMarketsRequest;
use crate::graphql;
use crate::graphql::list_markets;
use graphql_client::GraphQLQuery;

impl ListMarketsRequest {
    pub fn make_query(&self) -> graphql_client::QueryBody<list_markets::Variables> {
        graphql::ListMarkets::build_query(list_markets::Variables {})
    }
}
