use crate::graphql;
use graphql::subscribe_trades;
use graphql_client::GraphQLQuery;
use crate::types::Market;

#[derive(Clone, Debug)]
pub struct SubscribeTrades {
    pub market: Market,
}

impl SubscribeTrades {
    pub fn make_query(&self) -> graphql_client::QueryBody<subscribe_trades::Variables> {
        graphql::SubscribeTrades::build_query(subscribe_trades::Variables {
            market_name: self.market.market_name(),
        })
    }
}
