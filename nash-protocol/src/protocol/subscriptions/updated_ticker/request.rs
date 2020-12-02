use crate::graphql;
use graphql::updated_ticker;
use graphql_client::GraphQLQuery;

// Subscribe to ticker updates on `Market`.
#[derive(Clone, Debug)]
pub struct SubscribeTicker {
    pub market: String,
}

impl SubscribeTicker {
    pub fn make_query(&self) -> graphql_client::QueryBody<updated_ticker::Variables> {
        graphql::UpdatedTicker::build_query(updated_ticker::Variables {
            market_name: self.market.clone(),
        })
    }
}
