use crate::graphql;
use crate::types::Market;
use graphql::subscribe_trades;
use graphql_client::GraphQLQuery;

/// Initiate subscription to get new trades
#[derive(Clone, Debug)]
pub struct SubscribeTrades {
    pub market: String,
}

impl SubscribeTrades {
    pub fn make_query(&self) -> graphql_client::QueryBody<subscribe_trades::Variables> {
        graphql::SubscribeTrades::build_query(subscribe_trades::Variables {
            market_name: self.market.clone(),
        })
    }
}
