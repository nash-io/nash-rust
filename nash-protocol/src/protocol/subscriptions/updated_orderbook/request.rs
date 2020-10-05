use crate::graphql;
use crate::types::Market;
use graphql::updated_orderbook;
use graphql_client::GraphQLQuery;
#[derive(Clone, Debug)]
pub struct SubscribeOrderbook {
    pub market: Market,
}

impl SubscribeOrderbook {
    pub fn make_query(&self) -> graphql_client::QueryBody<updated_orderbook::Variables> {
        graphql::UpdatedOrderbook::build_query(updated_orderbook::Variables {
            market_name: self.market.market_name(),
        })
    }
}
